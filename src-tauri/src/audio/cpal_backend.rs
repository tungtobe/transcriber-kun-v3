//! CPAL microphone discovery and capture for macOS and Windows.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};
use std::time::Duration;

use super::{
    permission_error, CaptureBackend, InputBlock, InputCallback, InputErrorCallback, InputFormat,
    InputSide, LiveMicrophone, LiveSources, PreparedInput, PreparedSourceSet, PreparedStream,
};
use crate::core::error::AppError;

#[cfg(target_os = "macos")]
type TapResource = Option<super::macos_tap::TapAggregate>;
#[cfg(not(target_os = "macos"))]
type TapResource = Option<()>;

#[derive(Default)]
pub(super) struct CpalBackend;

impl CaptureBackend for CpalBackend {
    fn live_sources(&self) -> Result<LiveSources, AppError> {
        let host = cpal::default_host();
        let default_id = host
            .default_input_device()
            .and_then(|device| device.id().ok())
            .map(|id| id.to_string());
        let devices = host
            .input_devices()
            .map_err(|err| permission_error(format!(
                "Could not enumerate microphone devices: {err}. Check microphone permission and device availability."
            )))?;

        let mut microphones = Vec::new();
        for device in devices {
            let id = device.id().map_err(|err| permission_error(format!(
                "Could not identify a microphone device: {err}. Check microphone permission and device availability."
            )))?.to_string();
            let description = device.description().map_err(|err| permission_error(format!(
                "Could not read a microphone device name: {err}. Check microphone permission and device availability."
            )))?;
            microphones.push(LiveMicrophone {
                source: format!("mic:{id}"),
                name: description.name().to_owned(),
                is_default: default_id.as_deref() == Some(id.as_str()),
            });
        }
        microphones.sort_by(|left, right| {
            right
                .is_default
                .cmp(&left.is_default)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
                .then_with(|| left.source.cmp(&right.source))
        });
        let default_microphone = microphones
            .iter()
            .find(|microphone| microphone.is_default)
            .map(|microphone| microphone.source.clone());

        #[cfg(target_os = "macos")]
        let system_available = super::macos_tap::supported_by_os();
        #[cfg(not(target_os = "macos"))]
        let system_available = false;

        Ok(LiveSources {
            microphones,
            default_microphone,
            system_available,
        })
    }

    fn prepare(
        &self,
        inputs: &[super::SourceInput],
        generation: u64,
        on_audio: InputCallback,
        on_error: InputErrorCallback,
    ) -> Result<PreparedSourceSet, AppError> {
        let mut prepared = Vec::with_capacity(inputs.len());
        for input in inputs {
            match input.side {
                InputSide::System => {
                    #[cfg(target_os = "macos")]
                    prepared.push(prepare_system(
                        generation,
                        on_audio.clone(),
                        on_error.clone(),
                    )?);
                    #[cfg(not(target_os = "macos"))]
                    return Err(system_unavailable_error());
                }
                InputSide::Microphone => prepared.push(prepare_microphone(
                    &input.source,
                    generation,
                    on_audio.clone(),
                    on_error.clone(),
                )?),
            }
        }
        Ok(PreparedSourceSet { inputs: prepared })
    }
}

fn system_unavailable_error() -> AppError {
    permission_error(
        "System audio capture is not available on this platform yet. Check device availability and select a microphone source.",
    )
}

fn prepare_microphone(
    device_id: &str,
    generation: u64,
    on_audio: InputCallback,
    on_error: InputErrorCallback,
) -> Result<PreparedInput, AppError> {
    let host = cpal::default_host();
    let requested_id = device_id.strip_prefix("mic:").unwrap_or(device_id);
    let device = host
        .input_devices()
        .map_err(|err| permission_error(format!(
            "Could not enumerate microphone devices: {err}. Check microphone permission and device availability."
        )))?
        .find(|device| {
            device
                .id()
                .map(|id| id.to_string() == requested_id)
                .unwrap_or(false)
        })
        .ok_or_else(|| permission_error(format!(
            "Microphone device “{requested_id}” is unavailable. Check that it is connected and that microphone access is allowed."
        )))?;
    let description = device.description().map_err(|err| permission_error(format!(
        "Could not read the microphone device name: {err}. Check microphone permission and device availability."
    )))?;
    let display_name = description.name().to_owned();
    let supported = device.default_input_config().map_err(|err| permission_error(format!(
        "Could not open microphone “{display_name}”: {err}. Allow microphone access in system settings and check that the device is available."
    )))?;
    let sample_rate = supported.sample_rate();
    let channels = supported.channels();
    let sample_format = supported.sample_format();
    let config: StreamConfig = supported.into();

    let error_source = format!("mic:{requested_id}");
    let stream = build_stream(
        &device,
        config,
        sample_format,
        generation,
        InputSide::Microphone,
        channels,
        sample_rate,
        error_source,
        on_audio,
        on_error,
    )
    .map_err(|err| permission_error(format!(
        "Could not prepare microphone “{display_name}”: {err}. Allow microphone access in system settings and check that the device is available."
    )))?;

    Ok(PreparedInput {
        format: InputFormat {
            side: InputSide::Microphone,
            source: format!("mic:{requested_id}"),
            sample_rate,
            channels,
        },
        stream: Box::new(CpalPreparedStream {
            stream,
            side: InputSide::Microphone,
            started: false,
            _tap: None,
        }),
    })
}

#[cfg(target_os = "macos")]
fn prepare_system(
    generation: u64,
    on_audio: InputCallback,
    on_error: InputErrorCallback,
) -> Result<PreparedInput, AppError> {
    if !super::macos_tap::supported_by_os() {
        return Err(system_unavailable_error());
    }
    let (device, tap) = super::macos_tap::TapAggregate::create()?;
    let supported = device
        .default_input_config()
        .map_err(|error| system_capture_error(format!("could not read the tap format: {error}")))?;
    let sample_rate = supported.sample_rate();
    let channels = supported.channels();
    let sample_format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let stream = build_stream(
        &device,
        config,
        sample_format,
        generation,
        InputSide::System,
        channels,
        sample_rate,
        "system".to_owned(),
        on_audio,
        on_error,
    )
    .map_err(system_capture_error)?;

    Ok(PreparedInput {
        format: InputFormat {
            side: InputSide::System,
            source: "system".to_owned(),
            sample_rate,
            channels,
        },
        stream: Box::new(CpalPreparedStream {
            stream,
            side: InputSide::System,
            started: false,
            _tap: Some(tap),
        }),
    })
}

fn system_capture_error(detail: impl AsRef<str>) -> AppError {
    permission_error(format!(
        "Could not prepare system audio capture: {}. If macOS denied access, allow it in System Settings > Privacy & Security > Screen & System Audio Recording, then retry.",
        detail.as_ref()
    ))
}

fn stream_error_guidance(side: InputSide) -> &'static str {
    match side {
        InputSide::System => {
            "If macOS denied access, allow it in System Settings > Privacy & Security > Screen & System Audio Recording, then retry."
        }
        InputSide::Microphone => {
            "Allow microphone access in System Settings and check that the device is available."
        }
    }
}

fn build_stream(
    device: &cpal::Device,
    config: StreamConfig,
    sample_format: SampleFormat,
    generation: u64,
    side: InputSide,
    channels: u16,
    sample_rate: u32,
    source: String,
    on_audio: InputCallback,
    on_error: InputErrorCallback,
) -> Result<Stream, String> {
    macro_rules! build {
        ($sample:ty) => {{
            build_typed_stream::<$sample>(
                device,
                config,
                generation,
                side,
                channels,
                sample_rate,
                source,
                on_audio,
                on_error,
            )
            .map_err(|err| err.to_string())
        }};
    }

    match sample_format {
        SampleFormat::I8 => build!(i8),
        SampleFormat::I16 => build!(i16),
        SampleFormat::I24 => build!(cpal::I24),
        SampleFormat::I32 => build!(i32),
        SampleFormat::I64 => build!(i64),
        SampleFormat::U8 => build!(u8),
        SampleFormat::U16 => build!(u16),
        SampleFormat::U24 => build!(cpal::U24),
        SampleFormat::U32 => build!(u32),
        SampleFormat::U64 => build!(u64),
        SampleFormat::F32 => build!(f32),
        SampleFormat::F64 => build!(f64),
        SampleFormat::DsdU8 | SampleFormat::DsdU16 | SampleFormat::DsdU32 => {
            Err("DSD audio samples are not supported".to_owned())
        }
        _ => Err("audio sample format is not supported".to_owned()),
    }
}

fn build_typed_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    generation: u64,
    side: InputSide,
    channels: u16,
    sample_rate: u32,
    source: String,
    on_audio: InputCallback,
    on_error: InputErrorCallback,
) -> Result<Stream, cpal::Error>
where
    T: SizedSample + Sample + Copy + Send + 'static,
    f32: FromSample<T>,
{
    let stream_error_source = source.clone();
    device.build_input_stream::<T, _, _>(
        config,
        move |data, _info| {
            let samples = data
                .iter()
                .map(|sample| f32::from_sample(*sample))
                .collect::<Vec<_>>();
            on_audio(InputBlock {
                generation,
                side,
                sample_rate,
                channels,
                samples,
            });
        },
        move |err| on_error(generation, side, format!("{stream_error_source}: {err}")),
        Some(Duration::from_millis(750)),
    )
}

struct CpalPreparedStream {
    stream: Stream,
    side: InputSide,
    started: bool,
    // Keep the tap and aggregate alive until after the CPAL stream has been
    // dropped. Rust drops fields in declaration order.
    _tap: TapResource,
}

impl PreparedStream for CpalPreparedStream {
    fn start(&mut self) -> Result<(), AppError> {
        if self.started {
            return Ok(());
        }
        self.stream.play().map_err(|err| {
            permission_error(format!(
                "Could not start {} audio capture: {err}. {}",
                self.side.name(),
                stream_error_guidance(self.side)
            ))
        })?;
        self.started = true;
        Ok(())
    }
}
