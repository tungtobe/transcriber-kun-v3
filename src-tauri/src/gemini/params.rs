//! Central operational limits for Gemini key allocation.

use std::time::Duration;

pub const QUOTA_COOLDOWN: Duration = Duration::from_secs(60);
pub const JOB_MAX_WAIT: Duration = Duration::from_secs(180);
pub const MEMO_DEADLINE: Duration = Duration::from_secs(90);
pub const MAX_ATTEMPTS: u8 = 4;
