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
pub use probe::{check_supported_extension, probe, MediaInfo, SUPPORTED_EXTENSIONS};
pub use proxy::{create_proxy, ProxyInfo};

pub(crate) const OUTPUT_SAMPLE_RATE: u32 = 16_000;
pub(crate) const SUGGESTED_FORMATS: &str =
    "Export the recording as mp4, m4a, or mp3 and try again.";

/// Sai đuôi file (không thuộc allow-list) — category/code `Format`. Dùng
/// riêng khỏi [`no_audio_error`] vì cùng category `Format` nhưng UI cần nói
/// hai câu khác nhau (spec Always).
pub(crate) fn format_error(reason: &'static str) -> crate::core::error::AppError {
    crate::core::error::AppError::new(
        crate::core::error::Code::Format,
        format!("{reason} {SUGGESTED_FORMATS}"),
    )
}

/// File đúng đuôi nhưng không có track audio phát được, không có frame, hoặc
/// rỗng — `code` `NoAudio` (category vẫn `Format`) để UI hiển thị câu riêng
/// "File không có audio phát được." thay vì câu gợi ý đổi định dạng (spec
/// Code Map: "`core/error.rs` `Code` → thêm `NoAudio`").
pub(crate) fn no_audio_error(reason: &'static str) -> crate::core::error::AppError {
    crate::core::error::AppError::new(crate::core::error::Code::NoAudio, reason)
}

pub(crate) fn storage_error() -> crate::core::error::AppError {
    crate::core::error::AppError::new(
        crate::core::error::Code::Storage,
        "The media file could not be read or written.",
    )
}
