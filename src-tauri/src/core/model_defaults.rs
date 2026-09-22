//! Default Gemini model names shared by `settings` (fallback values for the
//! three persisted model preferences) and `gemini::params` (defaults used by
//! direct Gemini calls). Living in `core` keeps the dependency one-way: two
//! features (`settings`, `gemini`) both depend on `core`, neither depends on
//! the other (Architecture Spine AD-1/AD-13: feature không import lẫn nhau).

pub const DEFAULT_TRANSCRIBE_MODEL: &str = "gemini-flash-lite-latest";
pub const DEFAULT_MEMO_MODEL: &str = "gemini-flash-lite-latest";
pub const DEFAULT_LIVE_MODEL: &str = "gemini-3.5-live-translate-preview";
