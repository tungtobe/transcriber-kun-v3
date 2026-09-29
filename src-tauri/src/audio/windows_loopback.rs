//! Windows system capture from the default Console and Communications render
//! endpoints. The WASAPI worker owns every COM object and submits one mixed,
//! timestamp-aligned mono stream to the shared audio pipeline.

use std::collections::{HashMap, HashSet, VecDeque};

#[cfg(not(target_os = "windows"))]
use super::OUTPUT_SAMPLE_RATE;
#[cfg(target_os = "windows")]
use super::{
    InputBlock, InputCallback, InputErrorCallback, InputFormat, InputSide, PreparedInput,
    PreparedStream, OUTPUT_SAMPLE_RATE,
};
#[cfg(target_os = "windows")]
use crate::core::error::AppError;

const HNS_PER_SECOND: u64 = 10_000_000;
const ENDPOINT_SILENCE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(80);
const MAX_BUFFERED_GAP_FRAMES: i64 = OUTPUT_SAMPLE_RATE as i64;
#[cfg(target_os = "windows")]
const STARTUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
#[cfg(target_os = "windows")]
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(5);
#[cfg(target_os = "windows")]
const DEVICE_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
#[cfg(target_os = "windows")]
const LOOPBACK_BUFFER_HNS: i64 = 100_000;

fn wait_for_worker_startup(
    startup: &std::sync::mpsc::Receiver<Result<(), String>>,
    timeout: std::time::Duration,
) -> Result<(), String> {
    match startup.recv_timeout(timeout) {
        Ok(result) => result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(format!(
            "the WASAPI loopback worker did not initialize an endpoint within {} seconds",
            timeout.as_secs_f32()
        )),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err("the WASAPI loopback worker exited before reporting startup status".to_owned())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum EndpointRole {
    Console,
    Communications,
}

impl EndpointRole {
    #[cfg(target_os = "windows")]
    fn name(self) -> &'static str {
        match self {
            Self::Console => "Console",
            Self::Communications => "Communications",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EndpointPlan {
    id: String,
    roles: Vec<EndpointRole>,
}

/// Group role defaults by endpoint ID so one physical endpoint has one
/// loopback even when Windows assigns it to both roles.
fn plan_endpoints(role_devices: &[(EndpointRole, String)]) -> Vec<EndpointPlan> {
    let mut grouped = HashMap::<String, Vec<EndpointRole>>::new();
    for (role, id) in role_devices {
        let roles = grouped.entry(id.clone()).or_default();
        if !roles.contains(role) {
            roles.push(*role);
        }
    }
    let mut plans = grouped
        .into_iter()
        .map(|(id, mut roles)| {
            roles.sort();
            EndpointPlan { id, roles }
        })
        .collect::<Vec<_>>();
    plans.sort_by(|left, right| left.id.cmp(&right.id));
    plans
}

#[derive(Debug, Default, PartialEq, Eq)]
struct EndpointDelta {
    retained: Vec<String>,
    added: Vec<String>,
    removed: Vec<String>,
}

fn endpoint_delta(current: &HashSet<String>, desired: &HashSet<String>) -> EndpointDelta {
    let mut delta = EndpointDelta {
        retained: current.intersection(desired).cloned().collect(),
        added: desired.difference(current).cloned().collect(),
        removed: current.difference(desired).cloned().collect(),
    };
    delta.retained.sort();
    delta.added.sort();
    delta.removed.sort();
    delta
}

struct MixerLane {
    base_frame: Option<i64>,
    samples: VecDeque<f32>,
    started_at: std::time::Instant,
    last_packet_at: Option<std::time::Instant>,
}

impl MixerLane {
    fn new(started_at: std::time::Instant) -> Self {
        Self {
            base_frame: None,
            samples: VecDeque::new(),
            started_at,
            last_packet_at: None,
        }
    }

    fn is_idle(&self, now: std::time::Instant) -> bool {
        now.duration_since(self.last_packet_at.unwrap_or(self.started_at))
            >= ENDPOINT_SILENCE_TIMEOUT
    }

    fn end_frame(&self) -> Option<i64> {
        self.base_frame
            .map(|base| base.saturating_add(self.samples.len() as i64))
    }

    fn append(&mut self, start_frame: i64, samples: &[f32]) {
        let Some(base) = self.base_frame else {
            self.base_frame = Some(start_frame);
            self.samples.extend(samples.iter().copied());
            return;
        };
        let end = base.saturating_add(self.samples.len() as i64);
        if start_frame > end {
            let gap = start_frame.saturating_sub(end);
            if gap > MAX_BUFFERED_GAP_FRAMES {
                self.base_frame = Some(start_frame);
                self.samples.clear();
                self.samples.extend(samples.iter().copied());
                return;
            }
            self.samples.extend(std::iter::repeat_n(0.0, gap as usize));
            self.samples.extend(samples.iter().copied());
            return;
        }
        let overlap = usize::try_from(end.saturating_sub(start_frame)).unwrap_or(usize::MAX);
        if overlap < samples.len() {
            self.samples.extend(samples[overlap..].iter().copied());
        }
    }

    fn sample_at(&self, frame: i64) -> f32 {
        let Some(base) = self.base_frame else {
            return 0.0;
        };
        let offset = frame.saturating_sub(base);
        usize::try_from(offset)
            .ok()
            .and_then(|index| self.samples.get(index))
            .copied()
            .unwrap_or(0.0)
    }

    fn discard_before(&mut self, frame: i64) {
        let Some(base) = self.base_frame else {
            return;
        };
        let count = usize::try_from(frame.saturating_sub(base))
            .unwrap_or(usize::MAX)
            .min(self.samples.len());
        if count > 0 {
            self.samples.drain(..count);
            self.base_frame = Some(base.saturating_add(count as i64));
        }
    }
}

/// Aligns endpoint packets using their common WASAPI QPC timestamps and mixes
/// each sample once. WASAPI converts each endpoint to the pipeline's 16 kHz,
/// mono float format before packets reach this mixer.
#[derive(Default)]
struct TimedSystemMixer {
    anchor_hns: Option<u64>,
    cursor_frame: Option<i64>,
    active: HashSet<String>,
    first_packet: HashSet<String>,
    lanes: HashMap<String, MixerLane>,
}

impl TimedSystemMixer {
    fn set_endpoints(&mut self, endpoint_ids: impl IntoIterator<Item = String>) {
        self.set_endpoints_at(endpoint_ids, std::time::Instant::now());
    }

    fn set_endpoints_at(
        &mut self,
        endpoint_ids: impl IntoIterator<Item = String>,
        now: std::time::Instant,
    ) {
        let next = endpoint_ids.into_iter().collect::<HashSet<_>>();
        self.lanes.retain(|id, _| next.contains(id));
        self.first_packet.retain(|id| next.contains(id));
        for id in &next {
            self.lanes
                .entry(id.clone())
                .or_insert_with(|| MixerLane::new(now));
        }
        self.active = next;
    }

    #[cfg(test)]
    fn push(&mut self, endpoint_id: &str, timestamp_hns: u64, samples: &[f32]) {
        self.push_at(
            endpoint_id,
            timestamp_hns,
            samples,
            std::time::Instant::now(),
        );
    }

    #[cfg(any(target_os = "windows", test))]
    fn push_packet(&mut self, endpoint_id: &str, packet: &TimedPacket) {
        self.push_checked_at(
            endpoint_id,
            packet.timestamp_hns,
            packet.trusted,
            &packet.samples,
            std::time::Instant::now(),
        );
    }

    fn push_at(
        &mut self,
        endpoint_id: &str,
        timestamp_hns: u64,
        samples: &[f32],
        now: std::time::Instant,
    ) {
        self.push_checked_at(endpoint_id, timestamp_hns, true, samples, now);
    }

    /// A packet whose timestamp is not `trusted` (WASAPI reported a timestamp
    /// error or a data discontinuity) is placed at the lane's expected next
    /// frame instead of the reported position.
    fn push_checked_at(
        &mut self,
        endpoint_id: &str,
        timestamp_hns: u64,
        trusted: bool,
        samples: &[f32],
        now: std::time::Instant,
    ) {
        if !self.active.contains(endpoint_id) {
            return;
        }
        let anchor = *self.anchor_hns.get_or_insert(timestamp_hns);
        let reported_frame = timestamp_to_relative_frame(timestamp_hns, anchor);
        self.first_packet.insert(endpoint_id.to_owned());
        let lane = self
            .lanes
            .entry(endpoint_id.to_owned())
            .or_insert_with(|| MixerLane::new(now));
        let start_frame = if trusted {
            reported_frame
        } else {
            lane.end_frame().unwrap_or(reported_frame)
        };
        lane.last_packet_at = Some(now);
        lane.append(start_frame, samples);
    }

    fn pop_ready(&mut self) -> Option<Vec<f32>> {
        self.pop_ready_at(std::time::Instant::now())
    }

    fn pop_ready_at(&mut self, now: std::time::Instant) -> Option<Vec<f32>> {
        if self.active.is_empty()
            || !self.active.iter().all(|id| {
                self.first_packet.contains(id)
                    || self.lanes.get(id).is_some_and(|lane| lane.is_idle(now))
            })
        {
            return None;
        }
        let available_end = self
            .active
            .iter()
            .filter_map(|id| self.lanes.get(id).and_then(MixerLane::end_frame))
            .max()?;
        let mut cursor = match self.cursor_frame {
            Some(cursor) => cursor,
            None => {
                let cursor = self
                    .active
                    .iter()
                    .filter_map(|id| self.lanes.get(id).and_then(|lane| lane.base_frame))
                    .min()
                    .unwrap_or(0);
                self.cursor_frame = Some(cursor);
                cursor
            }
        };
        let fresh_start = self
            .active
            .iter()
            .filter_map(|id| self.lanes.get(id))
            .filter(|lane| !lane.is_idle(now))
            .filter_map(|lane| lane.base_frame)
            .min();
        if let Some(fresh_start) = fresh_start {
            if fresh_start.saturating_sub(cursor) > MAX_BUFFERED_GAP_FRAMES {
                cursor = fresh_start;
                self.cursor_frame = Some(cursor);
            }
        }
        let end = self
            .active
            .iter()
            .map(|id| {
                let lane = self.lanes.get(id)?;
                match lane.end_frame() {
                    Some(end) if end > cursor => Some(end),
                    _ if lane.is_idle(now) => Some(available_end),
                    _ => None,
                }
            })
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .min()?;
        if end <= cursor {
            return None;
        }

        let frames = usize::try_from(end.saturating_sub(cursor)).ok()?;
        let mut mixed = Vec::with_capacity(frames);
        for frame in cursor..end {
            mixed.push(
                self.active
                    .iter()
                    .filter_map(|id| self.lanes.get(id))
                    .map(|lane| lane.sample_at(frame))
                    .sum(),
            );
        }
        self.cursor_frame = Some(end);
        for lane in self.lanes.values_mut() {
            lane.discard_before(end);
        }
        Some(mixed)
    }
}

/// One captured WASAPI packet. `trusted` is false when the driver flagged the
/// timestamp as erroneous or the data as discontinuous.
#[cfg(any(target_os = "windows", test))]
struct TimedPacket {
    timestamp_hns: u64,
    trusted: bool,
    samples: Vec<f32>,
}

fn timestamp_to_relative_frame(timestamp_hns: u64, anchor_hns: u64) -> i64 {
    if timestamp_hns >= anchor_hns {
        let delta = u128::from(timestamp_hns - anchor_hns)
            .saturating_mul(u128::from(OUTPUT_SAMPLE_RATE))
            / u128::from(HNS_PER_SECOND);
        i64::try_from(delta).unwrap_or(i64::MAX)
    } else {
        let delta = u128::from(anchor_hns - timestamp_hns)
            .saturating_mul(u128::from(OUTPUT_SAMPLE_RATE))
            / u128::from(HNS_PER_SECOND);
        -i64::try_from(delta).unwrap_or(i64::MAX)
    }
}

#[cfg(target_os = "windows")]
pub(super) fn prepare(
    generation: u64,
    on_audio: InputCallback,
    on_error: InputErrorCallback,
) -> Result<PreparedInput, AppError> {
    Ok(PreparedInput {
        format: InputFormat {
            side: InputSide::System,
            source: "system".to_owned(),
            sample_rate: OUTPUT_SAMPLE_RATE,
            channels: 1,
        },
        stream: Box::new(WindowsLoopbackStream {
            generation,
            on_audio,
            on_error,
            stop: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            worker: None,
        }),
    })
}

#[cfg(target_os = "windows")]
pub(super) fn system_available() -> bool {
    use std::sync::mpsc;
    use std::time::Duration;

    let (tx, rx) = mpsc::sync_channel(1);
    let spawned = std::thread::Builder::new()
        .name("wasapi-device-check".to_owned())
        .spawn(move || {
            let available = initialize_com().ok().and_then(|_com| {
                let enumerator = wasapi::DeviceEnumerator::new().ok()?;
                let (role_devices, _) = resolve_default_roles(&enumerator);
                Some(!plan_endpoints(&role_devices).is_empty())
            });
            let _ = tx.send(available.unwrap_or(false));
        });
    spawned.is_ok() && rx.recv_timeout(Duration::from_secs(2)).unwrap_or(false)
}

#[cfg(target_os = "windows")]
struct WindowsLoopbackStream {
    generation: u64,
    on_audio: InputCallback,
    on_error: InputErrorCallback,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

#[cfg(target_os = "windows")]
impl PreparedStream for WindowsLoopbackStream {
    fn start(&mut self) -> Result<(), AppError> {
        use std::sync::atomic::Ordering;
        use std::sync::mpsc;
        if self.worker.is_some() {
            return Ok(());
        }
        self.stop.store(false, Ordering::Release);
        let generation = self.generation;
        let on_audio = self.on_audio.clone();
        let on_error = self.on_error.clone();
        let stop = self.stop.clone();
        let (startup_tx, startup_rx) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("wasapi-system-loopback".to_owned())
            .spawn(move || run_worker(generation, on_audio, on_error, stop, startup_tx));
        self.worker = match worker {
            Ok(worker) => Some(worker),
            Err(error) => {
                return Err(startup_error(format!(
                    "could not start the Windows loopback worker: {error}"
                )));
            }
        };

        if let Err(error) = wait_for_worker_startup(&startup_rx, STARTUP_TIMEOUT) {
            self.stop.store(true, Ordering::Release);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
            return Err(startup_error(error));
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn startup_error(detail: impl AsRef<str>) -> AppError {
    super::permission_error(format!(
        "Could not start Windows system audio capture: {}. {}",
        detail.as_ref(),
        super::system_capture_guidance()
    ))
}

#[cfg(target_os = "windows")]
impl Drop for WindowsLoopbackStream {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(target_os = "windows")]
fn initialize_com() -> Result<ComApartment, String> {
    wasapi::initialize_mta()
        .ok()
        .map_err(|error| format!("Could not initialize WASAPI COM apartment: {error}"))?;
    Ok(ComApartment)
}

#[cfg(target_os = "windows")]
struct ComApartment;

#[cfg(target_os = "windows")]
impl Drop for ComApartment {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        wasapi::deinitialize();
    }
}

#[cfg(target_os = "windows")]
fn resolve_default_roles(
    enumerator: &wasapi::DeviceEnumerator,
) -> (Vec<(EndpointRole, String)>, Vec<(EndpointRole, String)>) {
    use wasapi::{Direction, Role};

    let mut devices = Vec::new();
    let mut errors = Vec::new();
    for (role, wasapi_role) in [
        (EndpointRole::Console, Role::Console),
        (EndpointRole::Communications, Role::Communications),
    ] {
        let result = enumerator
            .get_default_device_for_role(&Direction::Render, &wasapi_role)
            .and_then(|device| device.get_id());
        match result {
            Ok(id) => devices.push((role, id)),
            Err(error) => errors.push((role, error.to_string())),
        }
    }
    (devices, errors)
}

#[cfg(target_os = "windows")]
struct LoopbackEndpoint {
    name: String,
    client: wasapi::AudioClient,
    capture: wasapi::AudioCaptureClient,
}

#[cfg(target_os = "windows")]
impl LoopbackEndpoint {
    fn open(enumerator: &wasapi::DeviceEnumerator, id: &str) -> Result<Self, String> {
        use wasapi::{Direction, SampleType, StreamMode, WaveFormat};

        let device = enumerator
            .get_device(id)
            .map_err(|error| format!("could not find endpoint {id}: {error}"))?;
        let name = device.get_friendlyname().unwrap_or_else(|_| id.to_owned());
        let mut client = device
            .get_iaudioclient()
            .map_err(|error| format!("could not create a client for {name}: {error}"))?;
        let format = WaveFormat::new(32, 32, &SampleType::Float, 16_000, 1, None);
        let mode = StreamMode::PollingShared {
            autoconvert: true,
            buffer_duration_hns: LOOPBACK_BUFFER_HNS,
        };
        client
            .initialize_client(&format, &Direction::Capture, &mode)
            .map_err(|error| format!("could not initialize loopback for {name}: {error}"))?;
        let capture = client
            .get_audiocaptureclient()
            .map_err(|error| format!("could not open the capture buffer for {name}: {error}"))?;
        client
            .start_stream()
            .map_err(|error| format!("could not start loopback for {name}: {error}"))?;
        Ok(Self {
            name,
            client,
            capture,
        })
    }

    fn read_packets(&self) -> Result<Vec<TimedPacket>, String> {
        let mut packets = Vec::new();
        loop {
            let frames = self
                .capture
                .get_next_packet_size()
                .map_err(|error| format!("{}: {error}", self.name))?
                .unwrap_or(0);
            if frames == 0 {
                break;
            }
            let mut bytes = vec![0_u8; frames as usize * std::mem::size_of::<f32>()];
            let (read_frames, info) = self
                .capture
                .read_from_device(&mut bytes)
                .map_err(|error| format!("{}: {error}", self.name))?;
            if read_frames == 0 {
                continue;
            }
            let samples = if info.flags.silent {
                vec![0.0; read_frames as usize]
            } else {
                bytes[..read_frames as usize * 4]
                    .chunks_exact(4)
                    .map(|sample| f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]))
                    .collect()
            };
            packets.push(TimedPacket {
                timestamp_hns: info.timestamp,
                trusted: !(info.flags.timestamp_error || info.flags.data_discontinuity),
                samples,
            });
        }
        Ok(packets)
    }
}

#[cfg(target_os = "windows")]
impl Drop for LoopbackEndpoint {
    fn drop(&mut self) {
        let _ = self.client.stop_stream();
    }
}

#[cfg(target_os = "windows")]
fn run_worker(
    generation: u64,
    on_audio: InputCallback,
    on_error: InputErrorCallback,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    startup: std::sync::mpsc::SyncSender<Result<(), String>>,
) {
    use std::sync::atomic::Ordering;
    use std::time::Instant;

    let _com = match initialize_com() {
        Ok(com) => com,
        Err(error) => {
            let _ = startup.send(Err(error));
            return;
        }
    };
    let enumerator = match wasapi::DeviceEnumerator::new() {
        Ok(enumerator) => enumerator,
        Err(error) => {
            let _ = startup.send(Err(format!(
                "Could not enumerate Windows playback devices: {error}"
            )));
            return;
        }
    };

    let mut endpoints = HashMap::<String, LoopbackEndpoint>::new();
    let mut mixer = TimedSystemMixer::default();
    let mut reported = HashSet::<String>::new();
    refresh_endpoints(
        &enumerator,
        &mut endpoints,
        &mut mixer,
        &mut reported,
        generation,
        &on_error,
    );
    if stop.load(Ordering::Acquire) {
        return;
    }
    if endpoints.is_empty() {
        let _ = startup.send(Err(
            "no default Console or Communications playback endpoint could be opened".to_owned(),
        ));
        return;
    }
    if startup.send(Ok(())).is_err() {
        return;
    }
    let mut refresh_at = Instant::now() + DEVICE_REFRESH_INTERVAL;

    while !stop.load(Ordering::Acquire) {
        let mut failed = Vec::new();
        for (id, endpoint) in &endpoints {
            match endpoint.read_packets() {
                Ok(packets) => {
                    for packet in packets {
                        mixer.push_packet(id, &packet);
                        while let Some(samples) = mixer.pop_ready() {
                            on_audio(InputBlock {
                                generation,
                                side: InputSide::System,
                                sample_rate: OUTPUT_SAMPLE_RATE,
                                channels: 1,
                                samples,
                            });
                        }
                    }
                }
                Err(error) => failed.push((id.clone(), error)),
            }
        }
        for (id, error) in failed {
            endpoints.remove(&id);
            mixer.set_endpoints(endpoints.keys().cloned());
            report_once(
                &on_error,
                generation,
                &mut reported,
                format!("device:{id}"),
                format!("Windows render endpoint {id} stopped: {error}"),
            );
        }

        if Instant::now() >= refresh_at {
            refresh_endpoints(
                &enumerator,
                &mut endpoints,
                &mut mixer,
                &mut reported,
                generation,
                &on_error,
            );
            refresh_at = Instant::now() + DEVICE_REFRESH_INTERVAL;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(target_os = "windows")]
fn refresh_endpoints(
    enumerator: &wasapi::DeviceEnumerator,
    endpoints: &mut HashMap<String, LoopbackEndpoint>,
    mixer: &mut TimedSystemMixer,
    reported: &mut HashSet<String>,
    generation: u64,
    on_error: &InputErrorCallback,
) {
    let (role_devices, role_errors) = resolve_default_roles(enumerator);
    for (role, error) in role_errors {
        report_once(
            on_error,
            generation,
            reported,
            format!("role:{role:?}"),
            format!("Could not resolve the default {role:?} Windows playback device: {error}"),
        );
    }
    for (role, _) in &role_devices {
        reported.remove(&format!("role:{role:?}"));
    }
    if role_devices.is_empty() {
        mixer.set_endpoints(endpoints.keys().cloned());
        return;
    }

    let plans = plan_endpoints(&role_devices);
    let desired = plans
        .iter()
        .map(|plan| plan.id.clone())
        .collect::<HashSet<_>>();
    let current = endpoints.keys().cloned().collect::<HashSet<_>>();
    let _delta = endpoint_delta(&current, &desired);
    let mut next = HashMap::new();
    for plan in plans {
        if let Some(endpoint) = endpoints.remove(&plan.id) {
            reported.remove(&format!("device:{}", plan.id));
            next.insert(plan.id, endpoint);
            continue;
        }
        match LoopbackEndpoint::open(enumerator, &plan.id) {
            Ok(endpoint) => {
                reported.remove(&format!("device:{}", plan.id));
                next.insert(plan.id, endpoint);
            }
            Err(error) => {
                let roles = plan
                    .roles
                    .iter()
                    .map(|role| role.name())
                    .collect::<Vec<_>>()
                    .join(" and ");
                report_once(
                    on_error,
                    generation,
                    reported,
                    format!("device:{}", plan.id),
                    format!(
                        "Could not capture the {roles} Windows playback endpoint {}: {error}",
                        plan.id
                    ),
                );
            }
        }
    }
    *endpoints = next;
    mixer.set_endpoints(endpoints.keys().cloned());
}

#[cfg(target_os = "windows")]
fn report_once(
    on_error: &InputErrorCallback,
    generation: u64,
    reported: &mut HashSet<String>,
    key: String,
    detail: String,
) {
    if reported.insert(key) {
        on_error(generation, InputSide::System, detail);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn endpoint_ids(plans: &[EndpointPlan]) -> Vec<String> {
        plans.iter().map(|plan| plan.id.clone()).collect()
    }

    #[test]
    fn startup_handshake_accepts_a_ready_worker() {
        let (sender, receiver) = mpsc::sync_channel(1);
        sender.send(Ok(())).unwrap();

        assert_eq!(
            wait_for_worker_startup(&receiver, std::time::Duration::from_secs(1)),
            Ok(())
        );
    }

    #[test]
    fn startup_handshake_returns_the_worker_initialization_failure() {
        let (sender, receiver) = mpsc::sync_channel(1);
        sender
            .send(Err("no playback endpoint opened".to_owned()))
            .unwrap();

        assert_eq!(
            wait_for_worker_startup(&receiver, std::time::Duration::from_secs(1)),
            Err("no playback endpoint opened".to_owned())
        );
    }

    #[test]
    fn startup_handshake_fails_if_worker_does_not_respond_in_time() {
        let (_sender, receiver) = mpsc::sync_channel(1);

        let error =
            wait_for_worker_startup(&receiver, std::time::Duration::from_millis(1)).unwrap_err();
        assert!(error.contains("did not initialize an endpoint"));
    }

    #[test]
    fn startup_handshake_fails_if_worker_exits_without_a_result() {
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(sender);

        let error =
            wait_for_worker_startup(&receiver, std::time::Duration::from_secs(1)).unwrap_err();
        assert!(error.contains("exited before reporting startup status"));
    }

    #[test]
    fn different_role_devices_are_mixed_once_on_one_aligned_system_stream() {
        let plans = plan_endpoints(&[
            (EndpointRole::Console, "speaker-a".to_owned()),
            (EndpointRole::Communications, "headset-b".to_owned()),
        ]);
        assert_eq!(plans.len(), 2);

        let mut mixer = TimedSystemMixer::default();
        mixer.set_endpoints(endpoint_ids(&plans));
        mixer.push("speaker-a", 10_000_000, &vec![0.2; 1_600]);
        assert!(mixer.pop_ready().is_none());
        mixer.push("headset-b", 10_000_000, &vec![0.3; 1_600]);
        let output = mixer.pop_ready().unwrap();

        assert_eq!(output.len(), 1_600);
        assert!(output.iter().all(|sample| (*sample - 0.5).abs() < 0.0001));
        assert!(mixer.pop_ready().is_none());
    }

    #[test]
    fn roles_with_the_same_endpoint_id_open_and_mix_it_only_once() {
        let plans = plan_endpoints(&[
            (EndpointRole::Console, "shared-device".to_owned()),
            (EndpointRole::Communications, "shared-device".to_owned()),
        ]);
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].roles.len(), 2);

        let mut mixer = TimedSystemMixer::default();
        mixer.set_endpoints(endpoint_ids(&plans));
        mixer.push("shared-device", 20_000_000, &vec![0.4; 1_600]);
        let output = mixer.pop_ready().unwrap();
        assert!(output.iter().all(|sample| (*sample - 0.4).abs() < 0.0001));
    }

    #[test]
    fn an_endpoint_failure_keeps_audio_from_the_remaining_endpoint() {
        let mut mixer = TimedSystemMixer::default();
        mixer.set_endpoints(["console".to_owned(), "communications".to_owned()]);
        mixer.push("console", 30_000_000, &vec![0.25; 1_600]);
        mixer.push("communications", 30_000_000, &vec![0.15; 1_600]);
        assert!(mixer.pop_ready().is_some());

        mixer.set_endpoints(["console".to_owned()]);
        mixer.push("console", 31_000_000, &vec![0.25; 1_600]);
        let output = mixer.pop_ready().unwrap();
        assert!(output.iter().all(|sample| (*sample - 0.25).abs() < 0.0001));
    }

    #[test]
    fn changed_default_endpoint_is_added_and_old_endpoint_is_removed() {
        let delta = endpoint_delta(
            &HashSet::from(["old".to_owned(), "unchanged".to_owned()]),
            &HashSet::from(["new".to_owned(), "unchanged".to_owned()]),
        );
        assert_eq!(delta.retained, ["unchanged"]);
        assert_eq!(delta.added, ["new"]);
        assert_eq!(delta.removed, ["old"]);
    }

    #[test]
    fn timed_packets_with_offset_mix_on_the_same_sample_timeline() {
        let mut mixer = TimedSystemMixer::default();
        mixer.set_endpoints(["early".to_owned(), "late".to_owned()]);
        mixer.push("early", 40_000_000, &vec![0.2; 1_600]);
        mixer.push("late", 40_050_000, &vec![0.3; 1_520]);
        let output = mixer.pop_ready().unwrap();

        assert_eq!(output.len(), 1_600);
        assert!(output[..80]
            .iter()
            .all(|sample| (*sample - 0.2).abs() < 0.0001));
        assert!(output[80..]
            .iter()
            .all(|sample| (*sample - 0.5).abs() < 0.0001));
    }

    #[test]
    fn packet_with_timestamp_error_or_discontinuity_uses_the_expected_next_frame() {
        let started_at = std::time::Instant::now();
        let mut mixer = TimedSystemMixer::default();
        mixer.set_endpoints_at(["only".to_owned()], started_at);
        let packet = |timestamp_hns, trusted, value: f32| TimedPacket {
            timestamp_hns,
            trusted,
            samples: vec![value; 1_600],
        };
        mixer.push_checked_at(
            "only",
            10_000_000,
            packet(10_000_000, true, 0.1).trusted,
            &packet(10_000_000, true, 0.1).samples,
            started_at,
        );
        // Reported timestamp is a wild jump; the untrusted packet must still
        // follow the previous one contiguously.
        let bad = packet(900_000_000, false, 0.2);
        mixer.push_packet("only", &bad);
        let lane = mixer.lanes.get("only").unwrap();
        assert_eq!(lane.samples.len(), 3_200);
        assert_eq!(lane.base_frame, Some(0));
    }

    #[test]
    fn an_idle_endpoint_is_silence_and_does_not_stall_the_other_role() {
        let started_at = std::time::Instant::now();
        let mut mixer = TimedSystemMixer::default();
        mixer.set_endpoints_at(
            ["console".to_owned(), "communications".to_owned()],
            started_at,
        );
        mixer.push_at("communications", 50_000_000, &vec![0.25; 1_600], started_at);

        assert!(mixer
            .pop_ready_at(started_at + std::time::Duration::from_millis(20))
            .is_none());
        let output = mixer
            .pop_ready_at(started_at + ENDPOINT_SILENCE_TIMEOUT)
            .unwrap();
        assert_eq!(output.len(), 1_600);
        assert!(output.iter().all(|sample| (*sample - 0.25).abs() < 0.0001));
    }

    #[test]
    fn long_idle_gap_does_not_allocate_silence_for_the_entire_gap() {
        let mut mixer = TimedSystemMixer::default();
        mixer.set_endpoints(["console".to_owned()]);
        mixer.push("console", 60_000_000, &vec![0.2; 1_600]);
        assert_eq!(mixer.pop_ready().unwrap().len(), 1_600);

        // Simulate playback resuming after several minutes. The stream should
        // advance to the fresh packet instead of buffering minutes of zeros.
        mixer.push("console", 3_660_000_000, &vec![0.3; 1_600]);
        let output = mixer.pop_ready().unwrap();
        assert_eq!(output.len(), 1_600);
        assert!(output.iter().all(|sample| (*sample - 0.3).abs() < 0.0001));
    }
}
