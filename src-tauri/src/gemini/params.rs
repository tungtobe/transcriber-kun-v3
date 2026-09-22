//! Central operational limits for Gemini key allocation.

use std::time::Duration;

/// Gemini REST endpoint.  Features only receive the normalized path below;
/// URL construction remains owned by the gateway transport.
pub const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com";
pub const MODELS_PATH: &str = "/v1beta/models";
/// The list operation asks for ten records at a time and follows every page
/// under one operation deadline.
pub const DEFAULT_MODELS_PAGE_SIZE: u32 = 10;
pub const MODELS_LIST_TIMEOUT: Duration = Duration::from_secs(15);
pub const KEY_TEST_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_TRANSCRIBE_MODEL: &str = "gemini-flash-lite-latest";
pub const DEFAULT_MEMO_MODEL: &str = "gemini-flash-lite-latest";
pub const DEFAULT_LIVE_MODEL: &str = "gemini-3.5-live-translate-preview";

pub const QUOTA_COOLDOWN: Duration = Duration::from_secs(60);
pub const JOB_MAX_WAIT: Duration = Duration::from_secs(180);
pub const MEMO_DEADLINE: Duration = Duration::from_secs(90);
pub const MAX_ATTEMPTS: u8 = 4;
