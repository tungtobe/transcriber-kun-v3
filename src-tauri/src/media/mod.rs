//! Streaming media ingestion primitives.
//!
//! This module deliberately has no IPC, picker, session, or Gemini knowledge.
//! Inputs are read packet-by-packet, normalized to mono 16 kHz, and emitted as
//! bounded FLAC chunks or a standalone FLAC proxy under the app's media root.

mod chunk;
mod decode;
mod flac;
mod hash;
mod probe;
mod proxy;
mod resample;

pub use chunk::{serialize_transcribe_request, Chunk, ChunkBudget, ChunkOptions, Chunker};
pub use decode::{decode_mono_16khz, DecodeStats};
pub use hash::sha256_file;
pub use probe::{probe, MediaInfo};
pub use proxy::{create_proxy, ProxyInfo};

pub(crate) const OUTPUT_SAMPLE_RATE: u32 = 16_000;
pub(crate) const SUGGESTED_FORMATS: &str =
    "Export the recording as mp4, m4a, or mp3 and try again.";

pub(crate) fn format_error(reason: &'static str) -> crate::core::error::AppError {
    crate::core::error::AppError::new(
        crate::core::error::Code::Format,
        format!("{reason} {SUGGESTED_FORMATS}"),
    )
}

pub(crate) fn storage_error() -> crate::core::error::AppError {
    crate::core::error::AppError::new(
        crate::core::error::Code::Storage,
        "The media file could not be read or written.",
    )
}
