use std::{fs::File, path::Path};

use symphonia::core::{
    codecs::audio::{AudioDecoder, AudioDecoderOptions},
    errors::Error as SymphoniaError,
    formats::probe::Hint,
    formats::{FormatOptions, FormatReader, Track},
    io::MediaSourceStream,
    meta::MetadataOptions,
};

use super::{format_error, storage_error};

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "mp3", "m4a", "mp4", "mov", "mkv", "webm", "wav", "flac", "ogg", "aiff", "aif", "caf",
];

#[derive(Debug, Clone, PartialEq)]
pub struct MediaInfo {
    /// Lowercase input extension. It is descriptive only and contains no path.
    pub container: String,
    pub sample_rate: u32,
    pub channels: usize,
    pub frame_count: u64,
    pub duration_seconds: f64,
    /// `false` means the duration was obtained by counting decoded audio frames.
    pub duration_from_metadata: bool,
}

/// Reads only container and codec metadata unless the container omits frame
/// count, in which case it decodes and counts frames without retaining PCM.
pub fn probe(path: &Path) -> Result<MediaInfo, crate::core::error::AppError> {
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
    let metadata_frames = track.num_frames;
    let mut sample_rate = codec_params.sample_rate.unwrap_or_default();
    let mut channels = codec_params
        .channels
        .as_ref()
        .map_or(0, |value| value.count());

    if let (Some(frame_count), true) = (metadata_frames, sample_rate > 0 && channels > 0) {
        if frame_count == 0 {
            return Err(format_error("No playable audio frames were found."));
        }
        return Ok(media_info(
            extension,
            sample_rate,
            channels,
            frame_count,
            true,
        ));
    }

    let mut frame_count = 0_u64;
    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(SymphoniaError::ResetRequired) => {
                return Err(format_error("The audio stream changes format mid-file."));
            }
            Err(_) => return Err(format_error("The audio stream could not be read.")),
        };
        if packet.track_id != track.id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(audio) => {
                if sample_rate == 0 {
                    sample_rate = audio.spec().rate();
                }
                if channels == 0 {
                    channels = audio.spec().channels().count();
                }
                frame_count = frame_count.saturating_add(audio.frames() as u64);
            }
            Err(SymphoniaError::DecodeError(_)) => {
                return Err(format_error(
                    "The audio stream contains an undecodable frame.",
                ));
            }
            Err(_) => return Err(format_error("The audio stream could not be decoded.")),
        }
    }

    if sample_rate == 0 || frame_count == 0 {
        return Err(format_error("No playable audio frames were found."));
    }

    Ok(media_info(
        extension,
        sample_rate,
        channels,
        metadata_frames.unwrap_or(frame_count),
        metadata_frames.is_some(),
    ))
}

fn media_info(
    container: String,
    sample_rate: u32,
    channels: usize,
    frame_count: u64,
    duration_from_metadata: bool,
) -> MediaInfo {
    MediaInfo {
        container,
        sample_rate,
        channels,
        frame_count,
        duration_seconds: frame_count as f64 / f64::from(sample_rate),
        duration_from_metadata,
    }
}

pub(super) fn open_media(
    path: &Path,
    extension: &str,
) -> Result<(Box<dyn FormatReader>, Track), crate::core::error::AppError> {
    let file = File::open(path).map_err(|_| storage_error())?;
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(extension);

    let probed = symphonia::default::get_probe()
        .probe(
            &hint,
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|_| format_error("The media container or audio codec is unsupported."))?;
    let format = probed;
    let track = format
        .tracks()
        .iter()
        .find(|track| {
            track
                .codec_params
                .as_ref()
                .and_then(|params| params.audio())
                .is_some()
        })
        .cloned()
        .ok_or_else(|| format_error("No playable audio track was found."))?;

    Ok((format, track))
}

pub(super) fn make_decoder(
    track: &Track,
) -> Result<Box<dyn AudioDecoder>, crate::core::error::AppError> {
    let params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .ok_or_else(|| format_error("No playable audio track was found."))?;
    symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|_| format_error("The audio codec is unsupported by the pure-Rust decoder."))
}

pub(super) fn supported_extension(path: &Path) -> Result<String, crate::core::error::AppError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if SUPPORTED_EXTENSIONS.contains(&extension.as_str()) {
        Ok(extension)
    } else {
        Err(format_error("This media format is not supported."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_container_extensions_are_rejected_before_open() {
        for extension in ["avi", "wmv", "flv", "ts"] {
            let error =
                supported_extension(Path::new(&format!("does-not-exist.{extension}"))).unwrap_err();
            assert_eq!(error.category, crate::core::error::Category::Format);
            assert!(error.detail_redacted.contains("mp4, m4a, or mp3"));
        }
    }

    #[test]
    fn common_audio_and_video_containers_are_allowlisted_case_insensitively() {
        for extension in [
            "mp3", "m4a", "mp4", "mov", "mkv", "webm", "wav", "flac", "ogg", "aiff", "caf",
        ] {
            let filename = format!("recording.{}", extension.to_ascii_uppercase());
            let path = Path::new(&filename);
            assert_eq!(supported_extension(path).unwrap(), extension);
        }
    }

    #[test]
    fn container_without_audio_is_a_format_error() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("empty.wav");
        let writer = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 1,
                sample_rate: 16_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        writer.finalize().unwrap();

        let error = probe(&path).unwrap_err();
        assert_eq!(error.category, crate::core::error::Category::Format);
        assert!(error.detail_redacted.contains("mp4, m4a, or mp3"));
    }
}
