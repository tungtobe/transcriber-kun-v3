//! Central operational limits for Gemini key allocation.

use std::time::Duration;

/// Gemini REST endpoint.  Features only receive the normalized path below;
/// URL construction remains owned by the gateway transport.
pub const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com";
/// Gemini Live WebSocket endpoint. Never include this URL in logs because the
/// production connector appends the leased API key as a query parameter.
pub const GEMINI_LIVE_WS_ENDPOINT: &str = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";
pub const MODELS_PATH: &str = "/v1beta/models";
/// The list operation asks for ten records at a time and follows every page
/// under one operation deadline.
pub const DEFAULT_MODELS_PAGE_SIZE: u32 = 10;
pub const MODELS_LIST_TIMEOUT: Duration = Duration::from_secs(15);
pub const KEY_TEST_TIMEOUT: Duration = Duration::from_secs(10);
/// A transcribe chunk's single-operation deadline, including key-pool wait.
pub const TRANSCRIBE_CHUNK_TIMEOUT: Duration = Duration::from_secs(120);
/// Maximum serialized JSON body sent for one inline transcribe request.
pub const MAX_TRANSCRIBE_REQUEST_BYTES: usize = 20 * 1024 * 1024;
// Owned by `core::model_defaults` so `settings` can fall back to the same
// strings without a feature-to-feature import (Story 1.9 Code Map).
pub use crate::core::model_defaults::{
    DEFAULT_LIVE_MODEL, DEFAULT_MEMO_MODEL, DEFAULT_TRANSCRIBE_MODEL,
};

pub const QUOTA_COOLDOWN: Duration = Duration::from_secs(60);
pub const JOB_MAX_WAIT: Duration = Duration::from_secs(180);
pub const MEMO_DEADLINE: Duration = Duration::from_secs(90);
pub const MAX_ATTEMPTS: u8 = 4;
/// Backoff applied to a key after a `RequestOutcome::Server` (5xx / transport
/// network) failure, before that key is eligible for the retry: `1s ×
/// attempts so far`, capped at [`SERVER_BACKOFF_MAX`]. Reuses the same
/// `cooldown_until` mechanism as [`QUOTA_COOLDOWN`].
pub const SERVER_BACKOFF: Duration = Duration::from_secs(1);
pub const SERVER_BACKOFF_MAX: Duration = Duration::from_secs(5);
