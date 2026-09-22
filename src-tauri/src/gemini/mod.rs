//! Single Gemini gateway seam. Story 1.5 keeps the transport fake, but the
//! gate is real: no transport closure is invoked until consent is current.

use crate::consent;
use crate::core::error::{AppError, Code};

pub fn guarded_request<T>(
    accepted_version: u32,
    declined: bool,
    transport: impl FnOnce() -> Result<T, AppError>,
) -> Result<T, AppError> {
    if !consent::is_current(accepted_version, declined) {
        return Err(AppError::new(
            Code::Blocked,
            "Gemini transport requires current consent",
        ));
    }
    transport()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_declined_and_stale_never_open_transport() {
        for (version, declined) in [(0, false), (1, true), (2, false)] {
            let mut calls = 0;
            let result = guarded_request(version, declined, || {
                calls += 1;
                Ok::<_, AppError>(())
            });
            assert!(result.is_err());
            assert_eq!(calls, 0);
        }
    }

    #[test]
    fn current_consent_opens_transport_once() {
        let mut calls = 0;
        let result = guarded_request(1, false, || {
            calls += 1;
            Ok::<_, AppError>("fake response")
        });
        assert_eq!(result.unwrap(), "fake response");
        assert_eq!(calls, 1);
    }
}
