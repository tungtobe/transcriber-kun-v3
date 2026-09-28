//! Shared live-audio capture primitives.
//!
//! Capture adapters submit interleaved samples to this module. The pipeline
//! downmixes and resamples each input onto one 16 kHz sample timeline, applies
//! a soft gate, and publishes one 1,600-sample PCM16 chunk every 100 ms. The
//! output clock advances by emitted samples, including silence while an
//! already-open source is gated or temporarily has no samples.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::sync::broadcast;

use crate::core::error::{AppError, Code};

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod cpal_backend;
#[cfg(target_os = "macos")]
mod macos_tap;
#[cfg(any(target_os = "windows", test))]
mod windows_loopback;

pub const OUTPUT_SAMPLE_RATE: u32 = 16_000;
pub const OUTPUT_CHANNELS: u16 = 1;
pub const OUTPUT_CHUNK_SAMPLES: usize = 1_600;
const OUTPUT_CHUNK_PERIOD: Duration = Duration::from_millis(100);
const OUTPUT_BROADCAST_CAPACITY: usize = 32;
const GATE_FLOOR: f32 = 0.004;
const GATE_CEILING: f32 = 0.016;
const GATE_ATTACK_ALPHA: f32 = 0.008;
const GATE_RELEASE_ALPHA: f32 = 0.001;

/// A microphone visible to the frontend. `source` is the opaque value passed
/// to `live_set_source`; `name` is for display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LiveMicrophone {
    pub source: String,
    pub name: String,
    pub is_default: bool,
}

/// Live-capture availability. Device enumeration is refreshed on every call,
/// so callers can request a refresh after connecting a microphone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LiveSources {
    pub microphones: Vec<LiveMicrophone>,
    pub default_microphone: Option<String>,
    pub system_available: bool,
}

/// A published mono PCM16 chunk. `start_sample` is the first sample's offset
/// on the process-wide live-capture output timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct PcmChunk {
    pub start_sample: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<i16>,
    pub source_errors: Vec<AudioSourceError>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioSourceError {
    pub source: String,
    pub error: AppError,
}

/// The source selector accepts exactly the public forms in the story.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureSource {
    System,
    Microphone { name: String },
    Mixed { microphone: String },
}

impl CaptureSource {
    pub fn parse(value: &str) -> Result<Self, AppError> {
        if value == "system" {
            return Ok(Self::System);
        }
        if let Some(name) = value.strip_prefix("mic:") {
            if !name.trim().is_empty() {
                return Ok(Self::Microphone {
                    name: name.to_owned(),
                });
            }
        }
        if let Some(name) = value.strip_prefix("mixed:") {
            if !name.trim().is_empty() {
                return Ok(Self::Mixed {
                    microphone: name.to_owned(),
                });
            }
        }
        Err(AppError::new(
            Code::Request,
            "Choose system, mic:<name>, or mixed:<mic> as the live audio source.",
        ))
    }

    pub fn key(&self) -> String {
        match self {
            Self::System => "system".to_owned(),
            Self::Microphone { name } => format!("mic:{name}"),
            Self::Mixed { microphone } => format!("mixed:{microphone}"),
        }
    }

    fn inputs(&self) -> Vec<SourceInput> {
        match self {
            Self::System => vec![SourceInput {
                side: InputSide::System,
                source: "system".to_owned(),
            }],
            Self::Microphone { name } => vec![SourceInput {
                side: InputSide::Microphone,
                source: format!("mic:{name}"),
            }],
            Self::Mixed { microphone } => vec![
                SourceInput {
                    side: InputSide::System,
                    source: "system".to_owned(),
                },
                SourceInput {
                    side: InputSide::Microphone,
                    source: format!("mic:{microphone}"),
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputSide {
    System,
    Microphone,
}

impl InputSide {
    fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Microphone => "microphone",
        }
    }
}

/// Source format selected while preparing an adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputFormat {
    pub side: InputSide,
    pub source: String,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInput {
    pub side: InputSide,
    pub source: String,
}

/// Samples delivered by a live capture adapter.
#[derive(Debug, Clone)]
pub struct InputBlock {
    pub generation: u64,
    pub side: InputSide,
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

pub type InputCallback = Arc<dyn Fn(InputBlock) + Send + Sync + 'static>;
pub type InputErrorCallback = Arc<dyn Fn(u64, InputSide, String) + Send + Sync + 'static>;

/// A prepared stream is built but not necessarily playing yet. The controller
/// starts every replacement before swapping it into the active slot.
pub trait PreparedStream: Send {
    fn start(&mut self) -> Result<(), AppError>;
}

pub struct PreparedInput {
    pub format: InputFormat,
    pub stream: Box<dyn PreparedStream>,
}

pub struct PreparedSourceSet {
    pub inputs: Vec<PreparedInput>,
}

/// Platform seam shared by CPAL, the future system-capture adapters, and
/// deterministic tests.
pub trait CaptureBackend: Send + Sync + 'static {
    fn live_sources(&self) -> Result<LiveSources, AppError>;

    fn prepare(
        &self,
        inputs: &[SourceInput],
        generation: u64,
        on_audio: InputCallback,
        on_error: InputErrorCallback,
    ) -> Result<PreparedSourceSet, AppError>;
}

struct OpenedInput {
    generation: u64,
    format: InputFormat,
    _stream: Box<dyn PreparedStream>,
}

struct ControllerInner {
    backend: Arc<dyn CaptureBackend>,
    pipeline: Mutex<AudioPipeline>,
    active_source: Mutex<Option<String>>,
    opened: Mutex<HashMap<String, OpenedInput>>,
    failed_inputs: Mutex<HashSet<(u64, InputSide)>>,
    source_switch: Mutex<()>,
    next_generation: AtomicU64,
    output: broadcast::Sender<PcmChunk>,
}

#[derive(Default)]
struct StagedInputs {
    active: bool,
    blocks: VecDeque<InputBlock>,
    errors: VecDeque<(u64, InputSide, String)>,
}

/// Process-wide capture coordinator. Subscribers receive the shared output
/// chunks used later by Live transport and durable recording.
pub struct CaptureController {
    inner: Arc<ControllerInner>,
}

impl CaptureController {
    pub fn new(backend: Arc<dyn CaptureBackend>) -> Self {
        let (output, _) = broadcast::channel(OUTPUT_BROADCAST_CAPACITY);
        let inner = Arc::new(ControllerInner {
            backend,
            pipeline: Mutex::new(AudioPipeline::default()),
            active_source: Mutex::new(None),
            opened: Mutex::new(HashMap::new()),
            failed_inputs: Mutex::new(HashSet::new()),
            source_switch: Mutex::new(()),
            next_generation: AtomicU64::new(1),
            output,
        });
        spawn_output_clock(Arc::downgrade(&inner));
        Self { inner }
    }

    pub fn production() -> Self {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let backend: Arc<dyn CaptureBackend> = Arc::new(cpal_backend::CpalBackend::default());

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let backend: Arc<dyn CaptureBackend> = Arc::new(UnsupportedBackend);

        Self::new(backend)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<PcmChunk> {
        self.inner.output.subscribe()
    }

    pub fn live_sources(&self, _refresh: bool) -> Result<LiveSources, AppError> {
        // This backend enumerates devices on each call. `refresh` remains
        // explicit in IPC so the caller can signal device-list refresh intent.
        self.inner.backend.live_sources()
    }

    pub fn active_source(&self) -> Option<String> {
        self.inner
            .active_source
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Current offset of the process-wide capture sample clock. It advances
    /// only when capture publishes a chunk, so Live timestamps never depend on
    /// when an event is received or replayed by the WebSocket task.
    pub fn sample_clock(&self) -> u64 {
        self.inner
            .pipeline
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clock_samples
    }

    /// Stops capture and releases every opened device stream. Callbacks from
    /// the released generation are ignored because the pipeline has no active
    /// inputs when this method returns.
    pub fn stop_capture(&self) {
        let _switch = self
            .inner
            .source_switch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let opened = {
            let mut opened = self
                .inner
                .opened
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            std::mem::take(&mut *opened)
        };
        self.inner
            .failed_inputs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        self.inner
            .pipeline
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .deactivate();
        *self
            .inner
            .active_source
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;

        // Drop native streams after clearing their generation from the
        // pipeline so callbacks racing stream teardown cannot publish audio.
        drop(opened);
    }

    /// Prepare and start the full replacement first. If opening or starting
    /// any part fails, dropping the candidate leaves the current streams and
    /// pipeline generation intact.
    pub fn set_source(&self, source: &str) -> Result<(), AppError> {
        let source = CaptureSource::parse(source)?;
        let source_key = source.key();
        let requested = source.inputs();
        let _switch = self
            .inner
            .source_switch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let retry_failed_input = {
            let opened = self
                .inner
                .opened
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let failed_inputs = self
                .inner
                .failed_inputs
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            requested.iter().any(|input| {
                opened.get(&input.source).is_none_or(|opened_input| {
                    failed_inputs.contains(&(opened_input.generation, opened_input.format.side))
                })
            })
        };
        if self.active_source().as_deref() == Some(source_key.as_str()) && !retry_failed_input {
            return Ok(());
        }

        let generation = self.inner.next_generation.fetch_add(1, Ordering::Relaxed);
        let missing = {
            let mut opened = self
                .inner
                .opened
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let mut failed_inputs = self
                .inner
                .failed_inputs
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let failed_sources = requested
                .iter()
                .filter_map(|input| {
                    let opened_input = opened.get(&input.source)?;
                    failed_inputs
                        .remove(&(opened_input.generation, opened_input.format.side))
                        .then(|| input.source.clone())
                })
                .collect::<Vec<_>>();
            for failed_source in failed_sources {
                opened.remove(&failed_source);
            }
            requested
                .iter()
                .filter(|input| !opened.contains_key(&input.source))
                .cloned()
                .collect::<Vec<_>>()
        };
        let pipeline = Arc::downgrade(&self.inner);
        let staged = Arc::new(Mutex::new(StagedInputs::default()));
        let audio_staged = staged.clone();

        let on_audio: InputCallback = Arc::new(move |block| {
            let mut staged = audio_staged
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !staged.active {
                staged.blocks.push_back(block);
                return;
            }
            drop(staged);
            if let Some(inner) = pipeline.upgrade() {
                inner
                    .pipeline
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .ingest(block);
            }
        });
        let error_pipeline = Arc::downgrade(&self.inner);
        let error_staged = staged.clone();
        let on_error: InputErrorCallback = Arc::new(move |generation, side, detail| {
            let mut staged = error_staged
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !staged.active {
                staged.errors.push_back((generation, side, detail));
                return;
            }
            drop(staged);
            if let Some(inner) = error_pipeline.upgrade() {
                let mut pipeline = inner
                    .pipeline
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if pipeline.active_inputs.contains(&(generation, side)) {
                    pipeline.note_source_error(generation, side, detail);
                    drop(pipeline);
                    inner
                        .failed_inputs
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .insert((generation, side));
                }
            }
        });

        let mut prepared = if missing.is_empty() {
            PreparedSourceSet { inputs: Vec::new() }
        } else {
            self.inner
                .backend
                .prepare(&missing, generation, on_audio, on_error)?
        };
        validate_prepared(&missing, &prepared)?;
        for input in &mut prepared.inputs {
            input.stream.start()?;
        }

        let mut opened = self
            .inner
            .opened
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let prepared_by_source = prepared
            .inputs
            .iter()
            .map(|input| {
                (
                    input.format.source.clone(),
                    (generation, input.format.clone()),
                )
            })
            .collect::<HashMap<_, _>>();
        let active_inputs = requested
            .iter()
            .map(|requested_input| {
                if let Some(opened_input) = opened.get(&requested_input.source) {
                    (opened_input.generation, opened_input.format.clone())
                } else {
                    prepared_by_source
                        .get(&requested_input.source)
                        .cloned()
                        .expect("validated prepared input must cover each missing source")
                }
            })
            .collect::<Vec<_>>();

        let mut pipeline = self
            .inner
            .pipeline
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        pipeline.activate_inputs(source.clone(), active_inputs);
        let mut staged = staged
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        staged.active = true;
        while let Some(block) = staged.blocks.pop_front() {
            pipeline.ingest(block);
        }
        while let Some((generation, side, detail)) = staged.errors.pop_front() {
            self.inner
                .failed_inputs
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert((generation, side));
            pipeline.note_source_error(generation, side, detail);
        }
        drop(staged);
        drop(pipeline);

        for input in prepared.inputs.drain(..) {
            opened.insert(
                input.format.source.clone(),
                OpenedInput {
                    generation,
                    format: input.format,
                    _stream: input.stream,
                },
            );
        }
        *self
            .inner
            .active_source
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(source_key);
        Ok(())
    }
}

fn validate_prepared(
    requested: &[SourceInput],
    prepared: &PreparedSourceSet,
) -> Result<(), AppError> {
    let mut actual = prepared
        .inputs
        .iter()
        .map(|input| SourceInput {
            side: input.format.side,
            source: input.format.source.clone(),
        })
        .collect::<Vec<_>>();
    actual.sort_by_key(|input| match input.side {
        InputSide::System => 0,
        InputSide::Microphone => 1,
    });
    let mut expected = requested.to_vec();
    expected.sort_by_key(|input| match input.side {
        InputSide::System => 0,
        InputSide::Microphone => 1,
    });
    if actual != expected {
        let guidance = if requested
            .iter()
            .any(|input| input.side == InputSide::System)
        {
            system_capture_guidance()
        } else {
            microphone_capture_guidance()
        };
        return Err(AppError::new(
            Code::Permission,
            format!("The selected live audio source is not available. {guidance}"),
        ));
    }
    if prepared.inputs.iter().any(|input| {
        input.format.sample_rate == 0
            || input.format.channels == 0
            || input.format.source.is_empty()
    }) {
        let guidance = if requested
            .iter()
            .any(|input| input.side == InputSide::System)
        {
            system_capture_guidance()
        } else {
            microphone_capture_guidance()
        };
        return Err(AppError::new(
            Code::Permission,
            format!("The audio device returned an invalid stream format. {guidance}"),
        ));
    }
    Ok(())
}

fn spawn_output_clock(inner: Weak<ControllerInner>) {
    let spawn = thread::Builder::new()
        .name("live-audio-output-clock".to_owned())
        .spawn(move || {
            let mut deadline = Instant::now() + OUTPUT_CHUNK_PERIOD;
            loop {
                thread::sleep(deadline.saturating_duration_since(Instant::now()));
                let Some(inner) = inner.upgrade() else {
                    break;
                };
                let chunk = inner
                    .pipeline
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .tick();
                if let Some(chunk) = chunk {
                    let _ = inner.output.send(chunk);
                }
                drop(inner);
                deadline += OUTPUT_CHUNK_PERIOD;
                let now = Instant::now();
                if deadline < now {
                    deadline = now + OUTPUT_CHUNK_PERIOD;
                }
            }
        });
    if let Err(err) = spawn {
        tracing::error!(error = %err, "could not start live audio output clock");
    }
}

#[derive(Default)]
struct AudioPipeline {
    source: Option<CaptureSource>,
    inputs: Vec<InputState>,
    active_inputs: HashSet<(u64, InputSide)>,
    pending_output: VecDeque<f32>,
    clock_samples: u64,
    pending_errors: Vec<AudioSourceError>,
}

impl AudioPipeline {
    #[cfg(test)]
    fn activate(&mut self, generation: u64, source: CaptureSource, formats: Vec<InputFormat>) {
        let inputs = formats
            .into_iter()
            .map(|format| (generation, format))
            .collect();
        self.activate_inputs(source, inputs);
    }

    fn activate_inputs(&mut self, source: CaptureSource, inputs: Vec<(u64, InputFormat)>) {
        self.preserve_active_samples();

        let mut active_inputs = HashSet::new();
        for (generation, format) in inputs {
            let key = (generation, format.side);
            active_inputs.insert(key);
            if !self
                .inputs
                .iter()
                .any(|input| input.generation == generation && input.format.side == format.side)
            {
                self.inputs.push(InputState::new(generation, format));
            }
        }
        self.active_inputs = active_inputs;
        self.source = Some(source);
    }

    fn deactivate(&mut self) {
        self.source = None;
        self.active_inputs.clear();
        self.pending_output.clear();
        self.pending_errors.clear();
        for input in &mut self.inputs {
            input.samples.clear();
        }
    }

    fn preserve_active_samples(&mut self) {
        let sample_count = self
            .inputs
            .iter()
            .filter(|input| {
                self.active_inputs
                    .contains(&(input.generation, input.format.side))
            })
            .map(|input| input.samples.len())
            .max()
            .unwrap_or(0);

        for _ in 0..sample_count {
            let mixed = self
                .inputs
                .iter_mut()
                .filter(|input| {
                    self.active_inputs
                        .contains(&(input.generation, input.format.side))
                })
                .map(|input| input.samples.pop_front().unwrap_or(0.0))
                .sum::<f32>();
            self.pending_output.push_back(mixed);
        }
    }

    fn ingest(&mut self, block: InputBlock) {
        let key = (block.generation, block.side);
        let active = self.active_inputs.contains(&key);
        let Some(input_index) = self.inputs.iter().position(|input| {
            input.generation == block.generation && input.format.side == block.side
        }) else {
            return;
        };
        let input = &mut self.inputs[input_index];
        if input.format.sample_rate != block.sample_rate
            || input.format.channels != block.channels
            || block.channels == 0
        {
            if active {
                let guidance = match input.format.side {
                    InputSide::System => system_capture_guidance(),
                    InputSide::Microphone => microphone_capture_guidance(),
                };
                self.pending_errors.push(AudioSourceError {
                    source: input.format.source.clone(),
                    error: permission_error(format!(
                        "The audio device changed its stream format while capturing. {guidance}"
                    )),
                });
            }
            return;
        }

        let channel_count = usize::from(block.channels);
        let mono = block
            .samples
            .chunks_exact(channel_count)
            .map(|frame| frame.iter().copied().sum::<f32>() / channel_count as f32);
        for sample in input.resampler.push(mono) {
            let gated = input.gate.apply(sample);
            if active {
                input.samples.push_back(gated);
            }
        }
    }

    fn note_source_error(&mut self, generation: u64, side: InputSide, detail: String) {
        if !self.active_inputs.contains(&(generation, side)) {
            return;
        }
        if let Some(input) = self.inputs.iter().find(|input| input.format.side == side) {
            let guidance = match side {
                InputSide::System => system_source_error_guidance(),
                InputSide::Microphone => microphone_capture_guidance(),
            };
            self.pending_errors.push(AudioSourceError {
                source: input.format.source.clone(),
                error: permission_error(format!(
                    "The {} audio source stopped: {detail}. {guidance}",
                    side.name(),
                )),
            });
        }
    }

    fn tick(&mut self) -> Option<PcmChunk> {
        self.source.as_ref()?;
        let start_sample = self.clock_samples;
        let mut output = vec![0_i16; OUTPUT_CHUNK_SAMPLES];
        for output_index in 0..OUTPUT_CHUNK_SAMPLES {
            let mixed = if let Some(pending) = self.pending_output.pop_front() {
                pending
            } else {
                self.inputs
                    .iter_mut()
                    .filter(|input| {
                        self.active_inputs
                            .contains(&(input.generation, input.format.side))
                    })
                    .map(|input| input.samples.pop_front().unwrap_or(0.0))
                    .sum::<f32>()
            };
            output[output_index] = float_to_pcm16(mixed);
        }
        self.clock_samples = self
            .clock_samples
            .saturating_add(OUTPUT_CHUNK_SAMPLES as u64);
        Some(PcmChunk {
            start_sample,
            sample_rate: OUTPUT_SAMPLE_RATE,
            channels: OUTPUT_CHANNELS,
            samples: output,
            source_errors: std::mem::take(&mut self.pending_errors),
        })
    }
}

struct InputState {
    generation: u64,
    format: InputFormat,
    resampler: LinearResampler,
    gate: SoftGate,
    samples: VecDeque<f32>,
}

impl InputState {
    fn new(generation: u64, format: InputFormat) -> Self {
        let resampler = LinearResampler::new(format.sample_rate);
        Self {
            generation,
            format,
            resampler,
            gate: SoftGate::default(),
            samples: VecDeque::new(),
        }
    }
}

#[derive(Default)]
struct SoftGate {
    envelope: f32,
}

impl SoftGate {
    fn apply(&mut self, sample: f32) -> f32 {
        if !sample.is_finite() {
            return 0.0;
        }
        let amplitude = sample.abs();
        let alpha = if amplitude > self.envelope {
            GATE_ATTACK_ALPHA
        } else {
            GATE_RELEASE_ALPHA
        };
        self.envelope += (amplitude - self.envelope) * alpha;

        let gain = if self.envelope <= GATE_FLOOR {
            0.0
        } else if self.envelope >= GATE_CEILING {
            1.0
        } else {
            let position = (self.envelope - GATE_FLOOR) / (GATE_CEILING - GATE_FLOOR);
            position * position * (3.0 - 2.0 * position)
        };
        sample * gain
    }
}

/// A streaming linear resampler. It tracks input sample positions, so callback
/// block boundaries do not reset phase or duplicate output samples.
struct LinearResampler {
    input_rate: u32,
    input_frames: u64,
    next_output_position: f64,
    previous_sample: Option<f32>,
}

impl LinearResampler {
    fn new(input_rate: u32) -> Self {
        Self {
            input_rate,
            input_frames: 0,
            next_output_position: 0.0,
            previous_sample: None,
        }
    }

    fn push(&mut self, input: impl IntoIterator<Item = f32>) -> Vec<f32> {
        let step = f64::from(self.input_rate) / f64::from(OUTPUT_SAMPLE_RATE);
        let mut output = Vec::new();
        for sample in input {
            let current_position = self.input_frames as f64;
            if let Some(previous) = self.previous_sample {
                while self.next_output_position <= current_position {
                    let fraction = (self.next_output_position - (current_position - 1.0))
                        .clamp(0.0, 1.0) as f32;
                    output.push(previous + (sample - previous) * fraction);
                    self.next_output_position += step;
                }
            } else {
                output.push(sample);
                self.next_output_position = step;
            }
            self.previous_sample = Some(sample);
            self.input_frames = self.input_frames.saturating_add(1);
        }
        output
    }
}

fn float_to_pcm16(sample: f32) -> i16 {
    if !sample.is_finite() {
        return 0;
    }
    let limited = sample.clamp(-1.0, 1.0);
    if limited <= -1.0 {
        i16::MIN
    } else {
        (limited * f32::from(i16::MAX)).round() as i16
    }
}

fn permission_error(detail: impl AsRef<str>) -> AppError {
    AppError::new(Code::Permission, detail)
}

fn system_capture_guidance() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        return "Check that a default Windows playback device is connected and available, then retry.";
    }
    #[cfg(target_os = "macos")]
    {
        return "If macOS denied system-audio access, allow it in System Settings > Privacy & Security > Screen & System Audio Recording, then retry.";
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        "Check device availability and select a supported audio source."
    }
}

fn system_source_error_guidance() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        return "If macOS denied system-audio access, allow it in System Settings > Privacy & Security > Screen & System Audio Recording, then retry. Silence alone does not indicate a permission failure.";
    }
    #[cfg(not(target_os = "macos"))]
    {
        system_capture_guidance()
    }
}

fn microphone_capture_guidance() -> &'static str {
    microphone_capture_guidance_for(cfg!(target_os = "windows"))
}

fn microphone_capture_guidance_for(windows: bool) -> &'static str {
    if windows {
        "Allow microphone access in Windows Settings > Privacy & security > Microphone (ms-settings:privacy-microphone), then retry."
    } else {
        "Allow microphone access in System Settings and check that the device is available."
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[derive(Default)]
struct UnsupportedBackend;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl CaptureBackend for UnsupportedBackend {
    fn live_sources(&self) -> Result<LiveSources, AppError> {
        Ok(LiveSources {
            microphones: Vec::new(),
            default_microphone: None,
            system_available: false,
        })
    }

    fn prepare(
        &self,
        _inputs: &[SourceInput],
        _generation: u64,
        _on_audio: InputCallback,
        _on_error: InputErrorCallback,
    ) -> Result<PreparedSourceSet, AppError> {
        Err(permission_error(
            "Live microphone capture is unavailable on this platform. Check device availability and microphone permission.",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    fn input_format(side: InputSide, source: &str, sample_rate: u32, channels: u16) -> InputFormat {
        InputFormat {
            side,
            source: source.to_owned(),
            sample_rate,
            channels,
        }
    }

    fn audio_block(
        generation: u64,
        side: InputSide,
        rate: u32,
        channels: u16,
        value: f32,
        frames: usize,
    ) -> InputBlock {
        InputBlock {
            generation,
            side,
            sample_rate: rate,
            channels,
            samples: vec![value; frames * usize::from(channels)],
        }
    }

    #[test]
    fn every_source_kind_emits_the_shared_pcm_chunk_format() {
        let cases = [
            (
                CaptureSource::System,
                vec![input_format(InputSide::System, "system", 16_000, 1)],
                vec![(InputSide::System, 16_000, 1, 0.25)],
            ),
            (
                CaptureSource::Microphone {
                    name: "default".to_owned(),
                },
                vec![input_format(
                    InputSide::Microphone,
                    "mic:default",
                    16_000,
                    1,
                )],
                vec![(InputSide::Microphone, 16_000, 1, 0.25)],
            ),
            (
                CaptureSource::Mixed {
                    microphone: "default".to_owned(),
                },
                vec![
                    input_format(InputSide::System, "system", 16_000, 1),
                    input_format(InputSide::Microphone, "mic:default", 16_000, 1),
                ],
                vec![
                    (InputSide::System, 16_000, 1, 0.15),
                    (InputSide::Microphone, 16_000, 1, 0.10),
                ],
            ),
        ];

        for (source, formats, inputs) in cases {
            let mut pipeline = AudioPipeline::default();
            pipeline.activate(1, source, formats);
            for (side, rate, channels, value) in inputs {
                pipeline.ingest(audio_block(1, side, rate, channels, value, 1_600));
            }
            let chunk = pipeline.tick().unwrap();
            assert_eq!(chunk.sample_rate, 16_000);
            assert_eq!(chunk.channels, 1);
            assert_eq!(chunk.samples.len(), 1_600);
            assert_eq!(chunk.start_sample, 0);
        }
    }

    #[test]
    fn system_only_source_does_not_request_a_microphone_input() {
        let inputs = CaptureSource::System.inputs();
        assert_eq!(
            inputs,
            [SourceInput {
                side: InputSide::System,
                source: "system".to_owned(),
            }]
        );
    }

    #[test]
    fn windows_microphone_permission_guidance_opens_privacy_settings() {
        let guidance = microphone_capture_guidance_for(true);
        assert!(guidance.contains("Windows Settings > Privacy & security > Microphone"));
        assert!(guidance.contains("ms-settings:privacy-microphone"));
    }

    #[test]
    fn repeated_source_swaps_keep_the_output_sample_clock_continuous() {
        let mut pipeline = AudioPipeline::default();
        let routes = [
            (
                CaptureSource::System,
                vec![input_format(InputSide::System, "system", 16_000, 1)],
                (InputSide::System, 0.2),
            ),
            (
                CaptureSource::Microphone {
                    name: "mic-a".to_owned(),
                },
                vec![input_format(InputSide::Microphone, "mic:mic-a", 16_000, 1)],
                (InputSide::Microphone, 0.3),
            ),
            (
                CaptureSource::Mixed {
                    microphone: "mic-a".to_owned(),
                },
                vec![
                    input_format(InputSide::System, "system", 16_000, 1),
                    input_format(InputSide::Microphone, "mic:mic-a", 16_000, 1),
                ],
                (InputSide::System, 0.2),
            ),
        ];
        let mut starts = Vec::new();
        for (generation, (source, formats, (side, value))) in routes.into_iter().enumerate() {
            let generation = generation as u64 + 1;
            pipeline.activate(generation, source, formats.clone());
            pipeline.ingest(audio_block(generation, side, 16_000, 1, value, 1_600));
            if formats.len() == 2 {
                pipeline.ingest(audio_block(
                    generation,
                    InputSide::Microphone,
                    16_000,
                    1,
                    0.1,
                    1_600,
                ));
            }
            let chunk = pipeline.tick().unwrap();
            starts.push(chunk.start_sample);
        }
        assert_eq!(starts, [0, 1_600, 3_200]);
        assert_eq!(pipeline.clock_samples, 4_800);
    }

    #[test]
    fn swapping_sources_preserves_buffered_sample_content_before_new_audio() {
        let mut expected_pipeline = AudioPipeline::default();
        expected_pipeline.activate(
            1,
            CaptureSource::System,
            vec![input_format(InputSide::System, "system", 16_000, 1)],
        );
        expected_pipeline.ingest(audio_block(1, InputSide::System, 16_000, 1, 0.25, 1_600));
        let expected = expected_pipeline.tick().unwrap();

        let mut pipeline = AudioPipeline::default();
        pipeline.activate(
            1,
            CaptureSource::System,
            vec![input_format(InputSide::System, "system", 16_000, 1)],
        );
        pipeline.ingest(audio_block(1, InputSide::System, 16_000, 1, 0.25, 1_600));
        pipeline.activate(
            2,
            CaptureSource::Microphone {
                name: "mic-a".to_owned(),
            },
            vec![input_format(InputSide::Microphone, "mic:mic-a", 16_000, 1)],
        );
        pipeline.ingest(audio_block(2, InputSide::Microphone, 16_000, 1, 0.0, 1_600));

        let after_swap = pipeline.tick().unwrap();
        let next = pipeline.tick().unwrap();
        assert_eq!(after_swap.samples, expected.samples);
        assert_eq!(after_swap.start_sample, 0);
        assert_eq!(next.start_sample, 1_600);
        assert!(next.samples.iter().all(|sample| *sample == 0));
    }

    #[test]
    fn inactive_open_input_is_gated_to_silence() {
        let mut pipeline = AudioPipeline::default();
        pipeline.activate(
            1,
            CaptureSource::System,
            vec![input_format(InputSide::System, "system", 16_000, 1)],
        );
        pipeline.ingest(audio_block(1, InputSide::System, 16_000, 1, 0.25, 1_600));
        let initial = pipeline.tick().unwrap();
        assert!(initial.samples.iter().any(|sample| *sample != 0));

        pipeline.activate(
            2,
            CaptureSource::Microphone {
                name: "mic-a".to_owned(),
            },
            vec![input_format(InputSide::Microphone, "mic:mic-a", 16_000, 1)],
        );
        pipeline.ingest(audio_block(1, InputSide::System, 16_000, 1, 0.5, 1_600));
        let gated = pipeline.tick().unwrap();
        assert!(gated.samples.iter().all(|sample| *sample == 0));
        assert_eq!(gated.start_sample, 1_600);
    }

    #[test]
    fn mixed_sources_resample_align_by_output_sample_and_clamp() {
        let mut pipeline = AudioPipeline::default();
        pipeline.activate(
            9,
            CaptureSource::Mixed {
                microphone: "mic-a".to_owned(),
            },
            vec![
                input_format(InputSide::System, "system", 48_000, 2),
                input_format(InputSide::Microphone, "mic:mic-a", 44_100, 1),
            ],
        );
        pipeline.ingest(audio_block(9, InputSide::System, 48_000, 2, 0.75, 4_800));
        pipeline.ingest(audio_block(
            9,
            InputSide::Microphone,
            44_100,
            1,
            0.75,
            4_410,
        ));
        let chunk = pipeline.tick().unwrap();
        assert_eq!(chunk.samples.len(), 1_600);
        assert_eq!(chunk.samples[1_000], i16::MAX);
        assert_eq!(chunk.start_sample, 0);
    }

    #[test]
    fn gate_emits_silence_without_turning_silence_into_a_permission_error() {
        let mut pipeline = AudioPipeline::default();
        pipeline.activate(
            3,
            CaptureSource::Microphone {
                name: "quiet".to_owned(),
            },
            vec![input_format(InputSide::Microphone, "mic:quiet", 16_000, 1)],
        );
        pipeline.ingest(audio_block(
            3,
            InputSide::Microphone,
            16_000,
            1,
            0.001,
            1_600,
        ));
        let chunk = pipeline.tick().unwrap();
        assert!(chunk.samples.iter().all(|sample| *sample == 0));
        assert!(chunk.source_errors.is_empty());
        assert_eq!(chunk.start_sample, 0);
    }

    #[test]
    fn a_lost_mixed_source_reports_an_error_and_the_clock_keeps_advancing() {
        let mut pipeline = AudioPipeline::default();
        pipeline.activate(
            4,
            CaptureSource::Mixed {
                microphone: "mic-a".to_owned(),
            },
            vec![
                input_format(InputSide::System, "system", 16_000, 1),
                input_format(InputSide::Microphone, "mic:mic-a", 16_000, 1),
            ],
        );
        pipeline.ingest(audio_block(4, InputSide::Microphone, 16_000, 1, 0.2, 1_600));
        pipeline.note_source_error(4, InputSide::System, "device disconnected".to_owned());
        let first = pipeline.tick().unwrap();
        let second = pipeline.tick().unwrap();
        assert_eq!(first.source_errors.len(), 1);
        assert_eq!(
            first.source_errors[0].error.category,
            crate::core::error::Category::Permission
        );
        assert!(first.samples.iter().any(|sample| *sample != 0));
        assert_eq!(first.start_sample, 0);
        assert_eq!(second.start_sample, 1_600);
    }

    #[test]
    fn failed_prepare_keeps_the_current_source_selected() {
        let backend = Arc::new(FakeBackend::default());
        let controller = CaptureController::new(backend.clone());
        controller.set_source("system").unwrap();
        backend.fail_prepare.store(true, Ordering::SeqCst);
        let error = controller.set_source("mic:bad-device").unwrap_err();
        assert_eq!(error.category, crate::core::error::Category::Permission);
        assert!(error.detail_redacted.to_lowercase().contains("permission"));
        assert_eq!(controller.active_source().as_deref(), Some("system"));
    }

    #[test]
    fn failed_stream_start_keeps_the_current_source_selected() {
        let backend = Arc::new(FakeBackend::default());
        let controller = CaptureController::new(backend.clone());
        controller.set_source("system").unwrap();
        backend.fail_start.store(true, Ordering::SeqCst);

        let error = controller.set_source("mic:Built-in").unwrap_err();

        assert_eq!(error.category, crate::core::error::Category::Permission);
        assert_eq!(controller.active_source().as_deref(), Some("system"));
    }

    #[test]
    fn already_open_sources_are_reused_without_closing_their_streams() {
        let backend = Arc::new(FakeBackend::default());
        let controller = CaptureController::new(backend.clone());
        controller.set_source("system").unwrap();
        controller.set_source("mic:Built-in").unwrap();
        controller.set_source("mixed:Built-in").unwrap();
        controller.set_source("system").unwrap();
        controller.set_source("mic:Built-in").unwrap();

        assert_eq!(backend.prepare_count.load(Ordering::SeqCst), 2);
        assert_eq!(backend.start_count.load(Ordering::SeqCst), 2);
        assert_eq!(backend.drop_count.load(Ordering::SeqCst), 0);
        drop(controller);
        assert_eq!(backend.drop_count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn stop_capture_releases_every_stream_and_ignores_old_generation_audio() {
        let backend = Arc::new(FakeBackend::default());
        let controller = CaptureController::new(backend.clone());
        controller.set_source("system").unwrap();
        assert_eq!(controller.active_source().as_deref(), Some("system"));

        controller.stop_capture();

        assert!(controller.active_source().is_none());
        assert_eq!(backend.drop_count.load(Ordering::SeqCst), 1);
        let mut pipeline = controller
            .inner
            .pipeline
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert!(pipeline.source.is_none());
        assert!(pipeline.active_inputs.is_empty());
        pipeline.ingest(audio_block(1, InputSide::System, 16_000, 1, 0.5, 1_600));
        assert!(pipeline.inputs[0].samples.is_empty());
        assert!(pipeline.tick().is_none());
        drop(pipeline);

        controller.set_source("system").unwrap();
        assert_eq!(controller.active_source().as_deref(), Some("system"));
        assert_eq!(backend.start_count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_failed_active_source_can_be_selected_again_to_retry_capture() {
        let backend = Arc::new(FakeBackend::default());
        let controller = CaptureController::new(backend.clone());
        controller.set_source("system").unwrap();
        backend.report_system_error();
        backend.fail_prepare.store(true, Ordering::SeqCst);
        assert!(controller.set_source("system").is_err());
        backend.fail_prepare.store(false, Ordering::SeqCst);

        controller.set_source("system").unwrap();

        assert_eq!(backend.prepare_count.load(Ordering::SeqCst), 3);
        assert_eq!(backend.start_count.load(Ordering::SeqCst), 2);
        assert_eq!(backend.drop_count.load(Ordering::SeqCst), 1);
        assert_eq!(controller.active_source().as_deref(), Some("system"));
    }

    #[test]
    fn samples_received_before_the_swap_are_kept_for_the_new_source() {
        let controller = CaptureController::new(Arc::new(FakeBackend::default()));
        controller.set_source("system").unwrap();
        let pipeline = controller
            .inner
            .pipeline
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert_eq!(pipeline.inputs[0].samples.len(), 1_600);
        assert_eq!(pipeline.clock_samples, 0);
    }

    #[test]
    fn live_sources_expose_default_mic_and_current_system_availability() {
        let controller = CaptureController::new(Arc::new(FakeBackend::default()));
        let sources = controller.live_sources(true).unwrap();
        assert_eq!(sources.default_microphone.as_deref(), Some("mic:Built-in"));
        assert_eq!(sources.microphones.len(), 2);
        assert!(sources.system_available);
    }

    #[derive(Default)]
    struct FakeBackend {
        fail_prepare: AtomicBool,
        fail_start: Arc<AtomicBool>,
        prepare_count: AtomicUsize,
        start_count: Arc<AtomicUsize>,
        drop_count: Arc<AtomicUsize>,
        last_system_error: Arc<Mutex<Option<(InputErrorCallback, u64)>>>,
    }

    impl CaptureBackend for FakeBackend {
        fn live_sources(&self) -> Result<LiveSources, AppError> {
            Ok(LiveSources {
                microphones: vec![
                    LiveMicrophone {
                        source: "mic:Built-in".to_owned(),
                        name: "Built-in".to_owned(),
                        is_default: true,
                    },
                    LiveMicrophone {
                        source: "mic:USB".to_owned(),
                        name: "USB".to_owned(),
                        is_default: false,
                    },
                ],
                default_microphone: Some("mic:Built-in".to_owned()),
                system_available: true,
            })
        }

        fn prepare(
            &self,
            inputs: &[SourceInput],
            generation: u64,
            on_audio: InputCallback,
            on_error: InputErrorCallback,
        ) -> Result<PreparedSourceSet, AppError> {
            self.prepare_count.fetch_add(1, Ordering::SeqCst);
            if self.fail_prepare.load(Ordering::SeqCst) {
                return Err(permission_error(
                    "fake device open failure with permission guidance",
                ));
            }
            let formats = inputs
                .iter()
                .map(|input| input_format(input.side, &input.source, 16_000, 1))
                .collect::<Vec<_>>();
            if inputs.iter().any(|input| input.side == InputSide::System) {
                *self
                    .last_system_error
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                    Some((on_error.clone(), generation));
            }
            Ok(PreparedSourceSet {
                inputs: formats
                    .into_iter()
                    .map(|format| PreparedInput {
                        stream: Box::new(FakeStream {
                            on_audio: on_audio.clone(),
                            block: audio_block(
                                generation,
                                format.side,
                                format.sample_rate,
                                format.channels,
                                0.25,
                                1_600,
                            ),
                            start_count: self.start_count.clone(),
                            drop_count: self.drop_count.clone(),
                            fail_start: self.fail_start.clone(),
                        }),
                        format,
                    })
                    .collect(),
            })
        }
    }

    impl FakeBackend {
        fn report_system_error(&self) {
            let callback = self
                .last_system_error
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
                .expect("system stream has been prepared");
            (callback.0)(
                callback.1,
                InputSide::System,
                "fake Core Audio tap denial".to_owned(),
            );
        }
    }

    struct FakeStream {
        on_audio: InputCallback,
        block: InputBlock,
        start_count: Arc<AtomicUsize>,
        drop_count: Arc<AtomicUsize>,
        fail_start: Arc<AtomicBool>,
    }

    impl PreparedStream for FakeStream {
        fn start(&mut self) -> Result<(), AppError> {
            self.start_count.fetch_add(1, Ordering::SeqCst);
            if self.fail_start.load(Ordering::SeqCst) {
                return Err(permission_error("fake stream startup failure"));
            }
            (self.on_audio)(self.block.clone());
            Ok(())
        }
    }

    impl Drop for FakeStream {
        fn drop(&mut self) {
            self.drop_count.fetch_add(1, Ordering::SeqCst);
        }
    }
}
