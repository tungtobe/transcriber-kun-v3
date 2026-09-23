//! Versioned onboarding consent owned by Rust.
//!
//! The binary is the source of truth for the current consent version and
//! privacy-policy URL.  Frontend code receives this metadata through IPC and
//! may only derive display state from the durable settings snapshot.

use serde::{Deserialize, Serialize};
use specta::Type;

/// The consent text version shipped by this binary.
pub const CURRENT_VERSION: u32 = 1;

/// The first public-policy URL. Keep this HTTPS-only until public beta
/// hosting is finalized (OQ6).
pub const PRIVACY_URL: &str = "https://transkun.app/privacy";

/// Support contact URL shown in Settings → Giới thiệu (story 1.10). Same
/// domain/scope as [`PRIVACY_URL`] — already covered by the existing
/// `opener:allow-open-url` capability wildcard (`https://transkun.app/*`),
/// so no capability change is needed to open it (spec Boundaries).
pub const SUPPORT_URL: &str = "https://transkun.app/support";

/// The state used by both routing and the Gemini transport gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum ConsentStatus {
    Pending,
    Declined,
    Stale,
    Current,
}

/// Rust-owned metadata needed to render the consent step. The accepted
/// version and declined bit are included so the frontend never needs a
/// second source of truth or a duplicated version constant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConsentPolicy {
    pub current_version: u32,
    pub privacy_url: String,
    /// Support contact URL for Settings → Giới thiệu (spec Always). Carried
    /// here (rather than a separate command) so it is already loaded
    /// whenever `settingsStore` fetches the consent policy — including when
    /// Consent was declined, when About must still show it (spec Always:
    /// "Hoạt động cả khi Consent bị từ chối").
    pub support_url: String,
    pub accepted_version: u32,
    pub declined: bool,
    pub status: ConsentStatus,
}

/// Derive consent status using the current-version equality policy.
/// Declined is intentionally checked first so a declined decision remains
/// visible even if a caller happens to retain an older accepted version.
pub const fn status_for(
    current_version: u32,
    accepted_version: u32,
    declined: bool,
) -> ConsentStatus {
    if declined {
        ConsentStatus::Declined
    } else if accepted_version == current_version {
        ConsentStatus::Current
    } else if accepted_version == 0 {
        ConsentStatus::Pending
    } else {
        ConsentStatus::Stale
    }
}

/// Whether a transport may be opened for the supplied consent snapshot.
pub const fn is_current(accepted_version: u32, declined: bool) -> bool {
    !declined && accepted_version == CURRENT_VERSION
}

pub fn policy(accepted_version: u32, declined: bool) -> ConsentPolicy {
    ConsentPolicy {
        current_version: CURRENT_VERSION,
        privacy_url: PRIVACY_URL.to_string(),
        support_url: SUPPORT_URL.to_string(),
        accepted_version,
        declined,
        status: status_for(CURRENT_VERSION, accepted_version, declined),
    }
}

/// Values used when a consent mutation is persisted. Keeping these helpers
/// here prevents frontend code from manufacturing the current version.
pub const fn accepted_version_after_accept() -> u32 {
    CURRENT_VERSION
}

pub const fn accepted_version_after_decline() -> u32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_pending_declined_stale_and_current_without_ambiguity() {
        assert_eq!(status_for(1, 0, false), ConsentStatus::Pending);
        assert_eq!(status_for(1, 0, true), ConsentStatus::Declined);
        assert_eq!(status_for(1, 0, false), ConsentStatus::Pending);
        assert_eq!(status_for(1, 1, false), ConsentStatus::Current);
        assert_eq!(status_for(1, 2, false), ConsentStatus::Stale);
        assert_eq!(status_for(2, 1, false), ConsentStatus::Stale);
    }

    #[test]
    fn policy_exposes_one_current_version_and_https_url() {
        let value = policy(0, false);
        assert_eq!(value.current_version, CURRENT_VERSION);
        assert_eq!(value.privacy_url, PRIVACY_URL);
        assert!(value.privacy_url.starts_with("https://"));
        assert_eq!(value.support_url, SUPPORT_URL);
        assert!(value.support_url.starts_with("https://"));
        assert_eq!(value.status, ConsentStatus::Pending);
    }

    #[test]
    fn current_gate_requires_exact_version_and_not_declined() {
        assert!(is_current(CURRENT_VERSION, false));
        assert!(!is_current(0, false));
        assert!(!is_current(CURRENT_VERSION - 1, false));
        assert!(!is_current(CURRENT_VERSION, true));
    }
}
