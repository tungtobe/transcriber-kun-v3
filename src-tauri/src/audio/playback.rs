//! In-process TTS playback (Story 5.3).
//!
//! The only voice source is the Live Translate model's output audio
//! (PCM16 24 kHz mono little-endian). It is decoded by the gateway, pushed
//! here as bytes and played by a cpal output stream inside this process: no
//! IPC, no logging, no local/OS/cloud TTS.
//!
//! Layout: `PlaybackBuffer` is the only state shared with the device
//! callback. `Supervisor` is a pure, clock-injected state machine that owns
//! the stream and follows the default output device (poll 1 s, stall 2.5 s).
//! `PlaybackHandle` runs the supervisor on a dedicated thread (cpal streams
//! are not `Send`) and is what the LiveSession actor holds. Tests drive the
//! supervisor with a fake backend and a fake clock.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Sample rate of the model's output audio.
pub const TTS_SAMPLE_RATE: u32 = 24_000;
/// The default output device is polled at this period.
pub const DEVICE_POLL: Duration = Duration::from_secs(1);
/// Queued audio with no device callback for this long means a dead device.
pub const STALL_TIMEOUT: Duration = Duration::from_millis(2_500);
/// The supervisor thread wakes at least this often (speaking pill latency).
const THREAD_TICK: Duration = Duration::from_millis(100);
/// Upper bound of queued audio; a runaway producer cannot grow memory.
const MAX_QUEUED_SAMPLES: usize = TTS_SAMPLE_RATE as usize * 60;

struct BufferInner {
    samples: VecDeque<i16>,
    enabled: bool,
    /// Fractional read position between `samples[0]` and `samples[1]`.
    frac: f64,
}

/// Audio waiting to be played, shared with the device callback. While
/// disabled it accepts nothing, so nothing is ever buffered.
pub struct PlaybackBuffer {
    inner: Mutex<BufferInner>,
    callbacks: AtomicU64,
    failed: AtomicBool,
}

impl PlaybackBuffer {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(BufferInner {
                samples: VecDeque::new(),
                enabled: false,
                frac: 0.0,
            }),
            callbacks: AtomicU64::new(0),
            failed: AtomicBool::new(false),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BufferInner> {
        self.inner.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    pub fn set_enabled(&self, enabled: bool) {
        let mut inner = self.lock();
        inner.enabled = enabled;
        if !enabled {
            inner.samples.clear();
            inner.frac = 0.0;
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.lock().enabled
    }

    /// Queues PCM16 little-endian bytes. Returns false (and queues nothing)
    /// when disabled or when the bytes are not whole samples.
    pub fn push_pcm_le(&self, bytes: &[u8]) -> bool {
        if bytes.is_empty() || bytes.len() % 2 != 0 {
            return false;
        }
        let mut inner = self.lock();
        if !inner.enabled || inner.samples.len() >= MAX_QUEUED_SAMPLES {
            return false;
        }
        inner.samples.extend(
            bytes
                .chunks_exact(2)
                .map(|pair| i16::from_le_bytes([pair[0], pair[1]])),
        );
        true
    }

    pub fn clear(&self) {
        let mut inner = self.lock();
        inner.samples.clear();
        inner.frac = 0.0;
    }

    pub fn len(&self) -> usize {
        self.lock().samples.len()
    }

    pub fn callbacks(&self) -> u64 {
        self.callbacks.load(Ordering::Acquire)
    }

    /// Set by a stream error callback; the supervisor reopens on it.
    pub fn mark_failed(&self) {
        self.failed.store(true, Ordering::Release);
    }

    fn take_failed(&self) -> bool {
        self.failed.swap(false, Ordering::AcqRel)
    }

    /// Fills `out` (interleaved, `channels` per frame) at `device_rate`,
    /// resampling the 24 kHz mono queue linearly. Missing audio is silence.
    /// Called from the device callback; counts as a callback for stall
    /// detection.
    pub fn pull_frames(&self, out: &mut [f32], channels: usize, device_rate: u32) {
        self.callbacks.fetch_add(1, Ordering::AcqRel);
        let channels = channels.max(1);
        let step = f64::from(TTS_SAMPLE_RATE) / f64::from(device_rate.max(1));
        let mut inner = self.lock();
        for frame in out.chunks_mut(channels) {
            let value = match inner.samples.front().copied() {
                Some(current) if inner.enabled => {
                    let next = inner.samples.get(1).copied().unwrap_or(current);
                    let current = f64::from(current);
                    let value = current + (f64::from(next) - current) * inner.frac;
                    inner.frac += step;
                    while inner.frac >= 1.0 && !inner.samples.is_empty() {
                        inner.samples.pop_front();
                        inner.frac -= 1.0;
                    }
                    if inner.samples.is_empty() {
                        inner.frac = 0.0;
                    }
                    (value / 32_768.0) as f32
                }
                _ => 0.0,
            };
            frame.fill(value);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackError(pub String);

/// An open output stream. Dropping it closes the device.
pub trait PlaybackStream {}

/// Output-device access. Implemented over cpal on macOS/Windows and by a
/// fake in tests, in the manner of `CaptureBackend`.
pub trait PlaybackBackend: 'static {
    /// Identifier of the current default output device, `None` without one.
    fn default_output_id(&mut self) -> Option<String>;
    /// Opens a stream on `device_id` that pulls audio from `buffer`.
    fn open(
        &mut self,
        device_id: &str,
        buffer: Arc<PlaybackBuffer>,
    ) -> Result<Box<dyn PlaybackStream>, PlaybackError>;
}

/// Backend for platforms without playback support: never has a device.
pub struct NullBackend;

impl PlaybackBackend for NullBackend {
    fn default_output_id(&mut self) -> Option<String> {
        None
    }
    fn open(
        &mut self,
        _device_id: &str,
        _buffer: Arc<PlaybackBuffer>,
    ) -> Result<Box<dyn PlaybackStream>, PlaybackError> {
        Err(PlaybackError("no output device".to_owned()))
    }
}

/// Follows the default output device and restarts the stream when it
/// changes, errors or stalls. Time is injected (`now` = monotonic offset).
pub struct Supervisor<B: PlaybackBackend> {
    backend: B,
    buffer: Arc<PlaybackBuffer>,
    stream: Option<Box<dyn PlaybackStream>>,
    device: Option<String>,
    last_poll: Option<Duration>,
    last_callbacks: u64,
    last_progress: Duration,
    speaking: bool,
}

impl<B: PlaybackBackend> Supervisor<B> {
    pub fn new(backend: B, buffer: Arc<PlaybackBuffer>) -> Self {
        Self {
            backend,
            buffer,
            stream: None,
            device: None,
            last_poll: None,
            last_callbacks: 0,
            last_progress: Duration::ZERO,
            speaking: false,
        }
    }

    pub fn speaking(&self) -> bool {
        self.speaking
    }

    pub fn has_stream(&self) -> bool {
        self.stream.is_some()
    }

    fn reopen(&mut self, target: Option<String>, now: Duration) {
        self.stream = None;
        self.device = None;
        self.last_progress = now;
        if let Some(id) = target {
            match self.backend.open(&id, self.buffer.clone()) {
                Ok(stream) => {
                    self.stream = Some(stream);
                    self.device = Some(id);
                }
                Err(error) => {
                    tracing::warn!(error = %error.0, "TTS output stream could not be opened");
                }
            }
        }
        if self.stream.is_none() {
            // Nowhere to play: stale audio must not surface later.
            self.buffer.clear();
        }
        self.last_callbacks = self.buffer.callbacks();
    }

    /// Advances the state machine; returns the new `speaking` value when it
    /// changed.
    pub fn tick(&mut self, now: Duration) -> Option<bool> {
        if !self.buffer.is_enabled() {
            self.stream = None;
            self.device = None;
            self.last_poll = None;
        } else {
            let poll_due = self
                .last_poll
                .map_or(true, |last| now.saturating_sub(last) >= DEVICE_POLL);
            let failed = self.buffer.take_failed();
            let stalled = {
                let callbacks = self.buffer.callbacks();
                if callbacks != self.last_callbacks || self.buffer.len() == 0 {
                    self.last_callbacks = callbacks;
                    self.last_progress = now;
                    false
                } else {
                    self.stream.is_some()
                        && now.saturating_sub(self.last_progress) >= STALL_TIMEOUT
                }
            };
            if poll_due || failed || stalled {
                let current = self.backend.default_output_id();
                if poll_due {
                    self.last_poll = Some(now);
                }
                if failed || stalled || current != self.device || self.stream.is_none() {
                    self.reopen(current, now);
                }
            }
        }
        let speaking = self.buffer.is_enabled() && self.stream.is_some() && self.buffer.len() > 0;
        if speaking == self.speaking {
            return None;
        }
        self.speaking = speaking;
        Some(speaking)
    }
}

pub type SpeakingCallback = Arc<dyn Fn(bool) + Send + Sync>;

enum Wake {
    Nudge,
}

/// Cheap handle held by the LiveSession actor. The supervisor thread exits
/// when the last handle is dropped.
#[derive(Clone)]
pub struct PlaybackHandle {
    buffer: Arc<PlaybackBuffer>,
    wake: mpsc::Sender<Wake>,
}

impl PlaybackHandle {
    /// `factory` runs on the supervisor thread so non-`Send` backends work.
    pub fn spawn<B, F>(factory: F, on_speaking: SpeakingCallback) -> Self
    where
        B: PlaybackBackend,
        F: FnOnce() -> B + Send + 'static,
    {
        let buffer = PlaybackBuffer::new();
        let (wake, wake_rx) = mpsc::channel::<Wake>();
        let thread_buffer = buffer.clone();
        let spawned = thread::Builder::new()
            .name("tts-playback".to_owned())
            .spawn(move || {
                let mut supervisor = Supervisor::new(factory(), thread_buffer);
                let start = Instant::now();
                loop {
                    match wake_rx.recv_timeout(THREAD_TICK) {
                        Ok(Wake::Nudge) | Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                    if let Some(speaking) = supervisor.tick(start.elapsed()) {
                        on_speaking(speaking);
                    }
                }
            });
        if let Err(error) = spawned {
            tracing::warn!(error = %error, "TTS playback thread could not start");
        }
        Self { buffer, wake }
    }

    /// Enables or disables playback. Disabling clears queued audio.
    pub fn set_enabled(&self, enabled: bool) {
        self.buffer.set_enabled(enabled);
        let _ = self.wake.send(Wake::Nudge);
    }

    pub fn is_enabled(&self) -> bool {
        self.buffer.is_enabled()
    }

    /// Queues model audio (PCM16 LE bytes); dropped while disabled.
    pub fn push(&self, pcm_le: &[u8]) {
        if self.buffer.push_pcm_le(pcm_le) {
            let _ = self.wake.send(Wake::Nudge);
        }
    }

    /// Stops playback at once and drops queued audio.
    pub fn clear(&self) {
        self.buffer.clear();
        let _ = self.wake.send(Wake::Nudge);
    }

    pub fn queued_samples(&self) -> usize {
        self.buffer.len()
    }
}

/// Backend for the real machine: cpal on macOS/Windows, none elsewhere.
pub fn platform_backend() -> impl PlaybackBackend {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        cpal_out::CpalPlayback
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        NullBackend
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod cpal_out {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{FromSample, SampleFormat, SizedSample, Stream};

    use super::{PlaybackBackend, PlaybackBuffer, PlaybackError, PlaybackStream};
    use std::sync::Arc;

    pub(super) struct CpalPlayback;

    struct CpalStream {
        _stream: Stream,
    }
    impl PlaybackStream for CpalStream {}

    impl PlaybackBackend for CpalPlayback {
        fn default_output_id(&mut self) -> Option<String> {
            cpal::default_host()
                .default_output_device()
                .and_then(|device| device.id().ok())
                .map(|id| id.to_string())
        }

        fn open(
            &mut self,
            device_id: &str,
            buffer: Arc<PlaybackBuffer>,
        ) -> Result<Box<dyn PlaybackStream>, PlaybackError> {
            let host = cpal::default_host();
            let device = host
                .output_devices()
                .map_err(|error| PlaybackError(error.to_string()))?
                .find(|device| {
                    device
                        .id()
                        .is_ok_and(|id| id.to_string() == device_id)
                })
                .ok_or_else(|| PlaybackError("output device is gone".to_owned()))?;
            let supported = device
                .default_output_config()
                .map_err(|error| PlaybackError(error.to_string()))?;
            let rate = supported.sample_rate();
            let channels = usize::from(supported.channels());
            let format = supported.sample_format();
            let config: cpal::StreamConfig = supported.into();
            let stream = match format {
                SampleFormat::F32 => build::<f32>(&device, config, buffer, channels, rate),
                SampleFormat::I16 => build::<i16>(&device, config, buffer, channels, rate),
                SampleFormat::I32 => build::<i32>(&device, config, buffer, channels, rate),
                SampleFormat::U16 => build::<u16>(&device, config, buffer, channels, rate),
                _ => Err(PlaybackError(
                    "output sample format is not supported".to_owned(),
                )),
            }?;
            stream
                .play()
                .map_err(|error| PlaybackError(error.to_string()))?;
            Ok(Box::new(CpalStream { _stream: stream }))
        }
    }

    fn build<T>(
        device: &cpal::Device,
        config: cpal::StreamConfig,
        buffer: Arc<PlaybackBuffer>,
        channels: usize,
        rate: u32,
    ) -> Result<Stream, PlaybackError>
    where
        T: SizedSample + FromSample<f32> + Send + 'static,
    {
        let error_buffer = buffer.clone();
        let mut scratch: Vec<f32> = Vec::new();
        device
            .build_output_stream::<T, _, _>(
                config,
                move |data, _info| {
                    scratch.resize(data.len(), 0.0);
                    buffer.pull_frames(&mut scratch, channels, rate);
                    for (out, value) in data.iter_mut().zip(scratch.iter()) {
                        *out = T::from_sample(*value);
                    }
                },
                move |error| {
                    tracing::warn!(error = %error, "TTS output stream error");
                    error_buffer.mark_failed();
                },
                None,
            )
            .map_err(|error| PlaybackError(error.to_string()))
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;

    #[derive(Default)]
    pub struct FakeState {
        pub default_id: Option<String>,
        pub open_fails: bool,
        pub opened: Vec<String>,
        /// Buffer of the currently open stream, so tests can pump callbacks.
        pub active: Option<Arc<PlaybackBuffer>>,
        pub active_device: Option<String>,
    }

    #[derive(Clone, Default)]
    pub struct FakeBackend(pub Arc<Mutex<FakeState>>);

    struct FakeStream(Arc<Mutex<FakeState>>);
    impl PlaybackStream for FakeStream {}
    impl Drop for FakeStream {
        fn drop(&mut self) {
            let mut state = self.0.lock().unwrap();
            state.active = None;
            state.active_device = None;
        }
    }

    impl FakeBackend {
        pub fn with_device(id: &str) -> Self {
            let backend = Self::default();
            backend.0.lock().unwrap().default_id = Some(id.to_owned());
            backend
        }
        pub fn set_default(&self, id: Option<&str>) {
            self.0.lock().unwrap().default_id = id.map(str::to_owned);
        }
        /// Simulates the device pulling `frames` mono 24 kHz frames.
        pub fn pump(&self, frames: usize) {
            let buffer = self.0.lock().unwrap().active.clone();
            if let Some(buffer) = buffer {
                let mut out = vec![0.0f32; frames];
                buffer.pull_frames(&mut out, 1, TTS_SAMPLE_RATE);
            }
        }
        pub fn opened(&self) -> Vec<String> {
            self.0.lock().unwrap().opened.clone()
        }
        pub fn active_device(&self) -> Option<String> {
            self.0.lock().unwrap().active_device.clone()
        }
    }

    impl PlaybackBackend for FakeBackend {
        fn default_output_id(&mut self) -> Option<String> {
            self.0.lock().unwrap().default_id.clone()
        }
        fn open(
            &mut self,
            device_id: &str,
            buffer: Arc<PlaybackBuffer>,
        ) -> Result<Box<dyn PlaybackStream>, PlaybackError> {
            let mut state = self.0.lock().unwrap();
            if state.open_fails {
                return Err(PlaybackError("open failed".to_owned()));
            }
            state.opened.push(device_id.to_owned());
            state.active = Some(buffer);
            state.active_device = Some(device_id.to_owned());
            drop(state);
            Ok(Box::new(FakeStream(self.0.clone())))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeBackend;
    use super::*;

    fn pcm(samples: usize) -> Vec<u8> {
        (0..samples)
            .flat_map(|i| ((i % 1000) as i16 + 1).to_le_bytes())
            .collect()
    }

    fn setup() -> (FakeBackend, Arc<PlaybackBuffer>, Supervisor<FakeBackend>) {
        let backend = FakeBackend::with_device("speakers");
        let buffer = PlaybackBuffer::new();
        buffer.set_enabled(true);
        let supervisor = Supervisor::new(backend.clone(), buffer.clone());
        (backend, buffer, supervisor)
    }

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    #[test]
    fn disabled_buffer_accepts_nothing_and_odd_or_empty_chunks_are_ignored() {
        let buffer = PlaybackBuffer::new();
        assert!(!buffer.push_pcm_le(&pcm(4)));
        assert_eq!(buffer.len(), 0);
        buffer.set_enabled(true);
        assert!(!buffer.push_pcm_le(&[1, 2, 3]));
        assert!(!buffer.push_pcm_le(&[]));
        assert!(buffer.push_pcm_le(&pcm(4)));
        assert_eq!(buffer.len(), 4);
        buffer.set_enabled(false);
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn pull_frames_decodes_le_and_resamples_to_device_rate_and_channels() {
        let buffer = PlaybackBuffer::new();
        buffer.set_enabled(true);
        assert!(buffer.push_pcm_le(&[0x00, 0x40, 0x00, 0x40])); // 16384 twice
        let mut out = [1.0f32; 4];
        buffer.pull_frames(&mut out, 1, TTS_SAMPLE_RATE);
        assert!((out[0] - 0.5).abs() < 1e-6 && (out[1] - 0.5).abs() < 1e-6);
        assert_eq!(out[2], 0.0);
        assert_eq!(buffer.len(), 0);

        // 48 kHz stereo: every source sample yields two frames, both
        // channels equal.
        assert!(buffer.push_pcm_le(&[0x00, 0x40, 0x00, 0x40]));
        let mut out = [9.0f32; 8];
        buffer.pull_frames(&mut out, 2, 48_000);
        assert!((out[0] - 0.5).abs() < 1e-6 && out[0] == out[1]);
        assert_eq!(buffer.len(), 0);
        assert!(out.iter().all(|value| *value <= 0.5 + 1e-6));
    }

    #[test]
    fn speaking_turns_on_with_audio_and_off_when_drained() {
        let (backend, buffer, mut supervisor) = setup();
        assert_eq!(supervisor.tick(ms(0)), None);
        assert!(supervisor.has_stream());
        buffer.push_pcm_le(&pcm(2_400));
        assert_eq!(supervisor.tick(ms(100)), Some(true));
        backend.pump(1_000);
        assert_eq!(supervisor.tick(ms(200)), None);
        backend.pump(5_000);
        assert_eq!(supervisor.tick(ms(300)), Some(false));
    }

    #[test]
    fn disabling_clears_audio_closes_the_stream_and_stops_speaking() {
        let (backend, buffer, mut supervisor) = setup();
        supervisor.tick(ms(0));
        buffer.push_pcm_le(&pcm(2_400));
        assert_eq!(supervisor.tick(ms(100)), Some(true));
        buffer.set_enabled(false);
        assert_eq!(supervisor.tick(ms(200)), Some(false));
        assert_eq!(buffer.len(), 0);
        assert!(backend.active_device().is_none());
        assert!(!buffer.push_pcm_le(&pcm(10)));
    }

    #[test]
    fn interrupt_clear_stops_speaking_at_once() {
        let (_backend, buffer, mut supervisor) = setup();
        supervisor.tick(ms(0));
        buffer.push_pcm_le(&pcm(2_400));
        assert_eq!(supervisor.tick(ms(100)), Some(true));
        buffer.clear();
        assert_eq!(supervisor.tick(ms(110)), Some(false));
    }

    #[test]
    fn default_device_change_resumes_on_the_new_device_within_two_seconds() {
        let (backend, buffer, mut supervisor) = setup();
        supervisor.tick(ms(0));
        buffer.push_pcm_le(&pcm(48_000));
        supervisor.tick(ms(100));
        backend.set_default(Some("headphones"));
        let changed_at = 1_050;
        let mut now = changed_at;
        loop {
            supervisor.tick(ms(now));
            if backend.active_device().as_deref() == Some("headphones") {
                break;
            }
            now += 100;
            assert!(now - changed_at <= 2_000, "did not switch within 2 s");
        }
        assert_eq!(backend.opened(), vec!["speakers", "headphones"]);
        // Queued audio survives the switch and still plays.
        backend.pump(100);
        assert!(buffer.len() > 0);
    }

    #[test]
    fn stalled_device_is_replaced_within_three_seconds() {
        let (backend, buffer, mut supervisor) = setup();
        supervisor.tick(ms(0));
        buffer.push_pcm_le(&pcm(48_000));
        assert_eq!(supervisor.tick(ms(100)), Some(true));
        backend.pump(100); // last sign of life at t=100
        supervisor.tick(ms(150));
        // Device stops calling back. Same default id, so only the stall
        // detector can notice.
        let mut now = 150;
        while backend.opened().len() < 2 {
            now += 100;
            supervisor.tick(ms(now));
            assert!(now - 150 <= 3_000, "stall not handled within 3 s");
        }
        assert!(now - 150 >= 2_400, "stall declared too early");
        assert_eq!(supervisor.speaking(), true);
    }

    #[test]
    fn no_device_means_not_speaking_and_recovers_when_one_appears() {
        let (backend, buffer, mut supervisor) = setup();
        supervisor.tick(ms(0));
        buffer.push_pcm_le(&pcm(2_400));
        assert_eq!(supervisor.tick(ms(100)), Some(true));
        backend.set_default(None);
        assert_eq!(supervisor.tick(ms(1_200)), Some(false));
        assert!(!supervisor.has_stream());
        assert_eq!(buffer.len(), 0);
        backend.set_default(Some("usb"));
        supervisor.tick(ms(2_300));
        assert!(supervisor.has_stream());
        buffer.push_pcm_le(&pcm(2_400));
        assert_eq!(supervisor.tick(ms(2_400)), Some(true));
    }

    #[test]
    fn open_failure_is_retried_on_the_next_poll_and_drops_stale_audio() {
        let (backend, buffer, mut supervisor) = setup();
        backend.0.lock().unwrap().open_fails = true;
        buffer.push_pcm_le(&pcm(2_400));
        assert_eq!(supervisor.tick(ms(0)), None);
        assert!(!supervisor.has_stream());
        assert_eq!(buffer.len(), 0);
        backend.0.lock().unwrap().open_fails = false;
        supervisor.tick(ms(1_000));
        assert!(supervisor.has_stream());
    }

    #[test]
    fn stream_error_flag_reopens_immediately() {
        let (backend, buffer, mut supervisor) = setup();
        supervisor.tick(ms(0));
        buffer.mark_failed();
        supervisor.tick(ms(100));
        assert_eq!(backend.opened().len(), 2);
    }

    #[test]
    fn handle_emits_speaking_on_a_real_thread_and_ignores_audio_while_disabled() {
        let backend = FakeBackend::with_device("speakers");
        let events = Arc::new(Mutex::new(Vec::<bool>::new()));
        let sink = events.clone();
        let thread_backend = backend.clone();
        let handle = PlaybackHandle::spawn(
            move || thread_backend,
            Arc::new(move |speaking| sink.lock().unwrap().push(speaking)),
        );
        handle.push(&pcm(2_400));
        assert_eq!(handle.queued_samples(), 0);
        handle.set_enabled(true);
        handle.push(&pcm(2_400));
        wait_for(|| events.lock().unwrap().as_slice() == [true]);
        handle.clear();
        wait_for(|| events.lock().unwrap().as_slice() == [true, false]);
        assert_eq!(handle.queued_samples(), 0);
    }

    fn wait_for(condition: impl Fn() -> bool) {
        let start = Instant::now();
        while !condition() {
            assert!(start.elapsed() < Duration::from_secs(3), "timed out");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
