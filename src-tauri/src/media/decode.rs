use std::path::Path;

use symphonia::core::errors::Error as SymphoniaError;

use super::{
    format_error,
    probe::{make_decoder, open_media, supported_extension},
    resample::MonoResampler,
    storage_error, OUTPUT_SAMPLE_RATE,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecodeStats {
    pub input_sample_rate: u32,
    pub source_frame_count: u64,
    pub output_frame_count: u64,
    pub duration_seconds: f64,
}

/// Decodes the first audio track as mono 16 kHz samples and sends bounded
/// blocks to `emit`. Video tracks are ignored; the source is never collected
/// into a file-sized PCM buffer.
pub fn decode_mono_16khz<F>(
    path: &Path,
    mut emit: F,
) -> Result<DecodeStats, crate::core::error::AppError>
where
    F: FnMut(&[f32]) -> Result<(), crate::core::error::AppError>,
{
    let extension = supported_extension(path)?;
    let (mut format, track) = open_media(path, &extension)?;
    let codec_params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .ok_or_else(|| format_error("No playable audio track was found."))?;
    if codec_params.codec == symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS {
        return Err(format_error(
            "Opus audio is not supported by the pure-Rust decoder.",
        ));
    }

    let mut decoder = make_decoder(&track)?;
    let mut input_rate = codec_params.sample_rate.unwrap_or_default();
    let metadata_frame_limit = track.num_frames;
    let mut resampler = (input_rate > 0)
        .then(|| MonoResampler::new(input_rate))
        .transpose()?;
    let mut source_frame_count = 0_u64;

    loop {
        if metadata_frame_limit.is_some_and(|limit| source_frame_count >= limit) {
            break;
        }
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(SymphoniaError::ResetRequired) => {
                return Err(format_error("The audio stream changes format mid-file."));
            }
            Err(SymphoniaError::IoError(_)) => return Err(storage_error()),
            Err(_) => return Err(format_error("The audio stream could not be read.")),
        };
        if packet.track_id != track.id {
            continue;
        }

        let audio = match decoder.decode(&packet) {
            Ok(audio) => audio,
            Err(SymphoniaError::IoError(_)) => return Err(storage_error()),
            Err(_) => return Err(format_error("The audio stream could not be decoded.")),
        };
        let spec = audio.spec();
        let block_rate = spec.rate();
        if block_rate == 0 {
            return Err(format_error("The audio sample rate is missing."));
        }
        if input_rate == 0 {
            input_rate = block_rate;
        }
        if block_rate != input_rate {
            return Err(format_error("The audio sample rate changes mid-file."));
        }
        if resampler.is_none() {
            resampler = Some(MonoResampler::new(input_rate)?);
        }

        let frames = audio.frames();
        let channel_count = spec.channels().count();
        if channel_count == 0 {
            return Err(format_error("The audio channel layout is unsupported."));
        }
        let frames_to_process = metadata_frame_limit.map_or(frames, |limit| {
            usize::try_from(limit.saturating_sub(source_frame_count))
                .unwrap_or(usize::MAX)
                .min(frames)
        });
        if frames_to_process == 0 {
            continue;
        }
        source_frame_count = source_frame_count.saturating_add(frames_to_process as u64);

        let mut interleaved = vec![0.0_f32; frames * channel_count];
        audio.copy_to_slice_interleaved(&mut interleaved);
        let mut mono = Vec::with_capacity(frames_to_process);
        for input_frame in interleaved
            .chunks_exact(channel_count)
            .take(frames_to_process)
        {
            let sum = input_frame.iter().copied().sum::<f32>();
            mono.push(sum / channel_count as f32);
        }
        resampler
            .as_mut()
            .expect("resampler initialized above")
            .push(&mono, &mut emit)?;
    }

    if source_frame_count == 0 || input_rate == 0 {
        return Err(format_error("No playable audio frames were found."));
    }
    let output_frame_count = resampler
        .as_mut()
        .ok_or_else(|| format_error("No playable audio frames were found."))?
        .finish(&mut emit)?;

    Ok(DecodeStats {
        input_sample_rate: input_rate,
        source_frame_count,
        output_frame_count,
        duration_seconds: output_frame_count as f64 / f64::from(OUTPUT_SAMPLE_RATE),
    })
}
