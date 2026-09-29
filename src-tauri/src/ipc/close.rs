//! One close-request path shared by native window closes and application
//! exit requests such as Cmd+Q.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::future::Future;
use tauri::Manager;
use tauri_specta::Event;

use crate::ipc::boot::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct CloseRequested {
    pub live_busy: bool,
    pub job_busy: bool,
}

/// Coalesce duplicate requests while the frontend flushes notes or waits for
/// the user. State-query failures fail closed so an active Live or Job is not
/// silently abandoned.
pub async fn request_close<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let state = app.state::<AppState>();
    if !try_claim(&state.close_requested, now_ms(), CLOSE_CLAIM_TTL_MS) {
        return;
    }

    let live_busy = match &state.live {
        Ok(live) => live.is_running().await.unwrap_or(true),
        Err(_) => false,
    };
    let job_busy = match &state.jobs {
        Ok(jobs) => jobs
            .snapshot()
            .await
            .map(|jobs| !jobs.is_empty())
            .unwrap_or(true),
        Err(_) => false,
    };

    if let Err(error) = (CloseRequested {
        live_busy,
        job_busy,
    })
    .emit(app)
    {
        tracing::warn!(error = %error, "could not emit close request");
        release(&state);
    }
}

/// How long one close request may go unanswered before a new one re-arms.
const CLOSE_CLAIM_TTL_MS: u64 = 10_000;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(1)
        .max(1)
}

/// Claims the close request. Fails only while a previous claim is younger
/// than `ttl_ms`; an expired claim is re-armed.
fn try_claim(claim: &std::sync::atomic::AtomicU64, now: u64, ttl_ms: u64) -> bool {
    use std::sync::atomic::Ordering::SeqCst;
    let previous = claim.load(SeqCst);
    if previous != 0 && now.saturating_sub(previous) < ttl_ms {
        return false;
    }
    claim
        .compare_exchange(previous, now.max(1), SeqCst, SeqCst)
        .is_ok()
}

pub fn release(state: &AppState) {
    release_claim(&state.close_requested);
}

fn release_claim(claim: &std::sync::atomic::AtomicU64) {
    claim.store(0, std::sync::atomic::Ordering::SeqCst);
}

/// Stops the running Live session for a confirmed close. `is_running` is asked
/// first so an idle app is a no-op; a stop failure propagates (stays fatal).
pub async fn save_live_for_close<RunFut, StopFut>(
    is_running: impl FnOnce() -> RunFut,
    stop_for_close: impl FnOnce() -> StopFut,
) -> Result<(), crate::core::error::AppError>
where
    RunFut: Future<Output = Result<bool, crate::core::error::AppError>>,
    StopFut: Future<Output = Result<(), crate::core::error::AppError>>,
{
    if is_running().await? {
        stop_for_close().await?;
    }
    Ok(())
}

/// Cancels every pending job, then polls until none is left or `timeout`
/// passes. A job that ignores cancel yields a `Storage` error (which
/// [`confirm_then_exit`] only logs).
pub async fn cancel_jobs_until_empty<Id, SnapFut, CancelFut>(
    snapshot: impl Fn() -> SnapFut,
    cancel: impl Fn(Id) -> CancelFut,
    timeout: std::time::Duration,
    poll: std::time::Duration,
) -> Result<(), crate::core::error::AppError>
where
    SnapFut: Future<Output = Result<Vec<Id>, crate::core::error::AppError>>,
    CancelFut: Future<Output = ()>,
{
    for id in snapshot().await? {
        cancel(id).await;
    }
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match snapshot().await {
            Ok(remaining) if remaining.is_empty() => return Ok(()),
            Ok(_) if tokio::time::Instant::now() < deadline => {}
            Ok(_) => {
                return Err(crate::core::error::AppError::new(
                    crate::core::error::Code::Storage,
                    "Transcription jobs did not stop before the close timeout",
                ));
            }
            Err(error) => return Err(error),
        }
        tokio::time::sleep(poll).await;
    }
}

pub fn authorize_exit(state: &AppState) {
    state
        .close_confirmed
        .store(true, std::sync::atomic::Ordering::SeqCst);
}

pub fn is_exit_authorized(state: &AppState) -> bool {
    state
        .close_confirmed
        .load(std::sync::atomic::Ordering::SeqCst)
}

/// `RunEvent::ExitRequested` is also how Cmd+Q reaches the close coordinator.
/// Until the frontend has flushed notes and confirmed, Tauri must keep the
/// process alive and let the shared `CloseRequested` event drive the dialog.
pub fn should_prevent_exit(is_authorized: bool) -> bool {
    !is_authorized
}

/// Runs the confirmed-close commit in safety order: Jobs are cancelled first
/// (a job that ignores cancel is only logged, it never blocks exit), then Live
/// is stopped and saved. The exit action is only called after Live
/// persistence succeeded; a Live stop failure stays fatal.
/// Keeping the operations injectable lets tests exercise the same production
/// sequence with realistic async save/cancel boundaries.
pub async fn confirm_then_exit<LiveFuture, JobsFuture>(
    save_live: LiveFuture,
    cancel_jobs: JobsFuture,
    authorize_exit: impl FnOnce(),
) -> Result<(), crate::core::error::AppError>
where
    LiveFuture: Future<Output = Result<(), crate::core::error::AppError>>,
    JobsFuture: Future<Output = Result<(), crate::core::error::AppError>>,
{
    if let Err(error) = cancel_jobs.await {
        tracing::warn!(code = ?error.code, "jobs did not stop before close; exiting anyway");
    }
    save_live.await?;
    authorize_exit();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::error::{AppError, Code};
    use std::sync::{Arc, Mutex};

    #[test]
    fn cmd_q_exit_is_intercepted_until_the_shared_close_flow_authorizes_it() {
        assert!(should_prevent_exit(false));
        assert!(!should_prevent_exit(true));
    }

    #[test]
    fn release_clears_the_claim_so_a_new_close_can_start() {
        let claim = std::sync::atomic::AtomicU64::new(0);
        assert!(try_claim(&claim, 1_000, 10_000));
        assert!(!try_claim(&claim, 2_000, 10_000));
        release_claim(&claim);
        assert!(try_claim(&claim, 2_000, 10_000));
    }

    #[tokio::test]
    async fn save_live_for_close_only_stops_a_running_session_and_keeps_failures_fatal() {
        let stopped = Arc::new(Mutex::new(0_u32));
        let idle = stopped.clone();
        save_live_for_close(
            || async { Ok(false) },
            move || async move {
                *idle.lock().unwrap() += 1;
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(*stopped.lock().unwrap(), 0);

        let running = stopped.clone();
        save_live_for_close(
            || async { Ok(true) },
            move || async move {
                *running.lock().unwrap() += 1;
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(*stopped.lock().unwrap(), 1);

        let error = save_live_for_close(
            || async { Ok(true) },
            || async { Err(AppError::new(Code::Storage, "stop failed")) },
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, Code::Storage);
        let error = save_live_for_close(
            || async { Err(AppError::new(Code::Storage, "actor gone")) },
            || async { Ok(()) },
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, Code::Storage);
    }

    #[tokio::test]
    async fn cancel_jobs_waits_for_the_queue_to_drain_and_times_out_on_a_stuck_job() {
        let pending = Arc::new(Mutex::new(vec![1_u32, 2]));
        let cancelled = Arc::new(Mutex::new(Vec::new()));
        let snapshot_pending = pending.clone();
        let cancel_pending = pending.clone();
        let cancel_log = cancelled.clone();
        cancel_jobs_until_empty(
            move || {
                let jobs = snapshot_pending.lock().unwrap().clone();
                async move { Ok(jobs) }
            },
            move |id| {
                cancel_pending.lock().unwrap().retain(|job| *job != id);
                cancel_log.lock().unwrap().push(id);
                async {}
            },
            std::time::Duration::from_millis(200),
            std::time::Duration::from_millis(5),
        )
        .await
        .unwrap();
        assert_eq!(*cancelled.lock().unwrap(), [1, 2]);

        // A job that ignores cancel exhausts the deadline.
        let error = cancel_jobs_until_empty(
            || async { Ok(vec![7_u32]) },
            |_| async {},
            std::time::Duration::from_millis(30),
            std::time::Duration::from_millis(5),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, Code::Storage);
    }

    #[test]
    fn close_claim_expires_and_rearms() {
        let claim = std::sync::atomic::AtomicU64::new(0);
        assert!(try_claim(&claim, 1_000, 10_000));
        assert!(!try_claim(&claim, 5_000, 10_000));
        assert!(try_claim(&claim, 11_500, 10_000));
        assert!(!try_claim(&claim, 12_000, 10_000));
    }

    #[tokio::test]
    async fn live_plus_job_close_cancels_jobs_then_saves_live_then_authorizes_exit() {
        let actions = Arc::new(Mutex::new(Vec::new()));
        let live_actions = actions.clone();
        let jobs_actions = actions.clone();
        let exit_actions = actions.clone();

        confirm_then_exit(
            async move {
                live_actions.lock().unwrap().push("live-saved");
                Ok(())
            },
            async move {
                jobs_actions.lock().unwrap().push("jobs-cancelled");
                Ok(())
            },
            move || exit_actions.lock().unwrap().push("exit-authorized"),
        )
        .await
        .unwrap();

        assert_eq!(
            *actions.lock().unwrap(),
            ["jobs-cancelled", "live-saved", "exit-authorized"]
        );
    }

    #[tokio::test]
    async fn job_cancel_timeout_does_not_block_exit() {
        let exited = Arc::new(Mutex::new(false));
        let flag = exited.clone();
        confirm_then_exit(
            async { Ok(()) },
            async { Err(AppError::new(Code::Storage, "jobs did not stop")) },
            move || *flag.lock().unwrap() = true,
        )
        .await
        .unwrap();
        assert!(*exited.lock().unwrap());
    }

    #[tokio::test]
    async fn live_storage_failure_stays_fatal_and_does_not_exit() {
        let exited = Arc::new(Mutex::new(false));
        let flag = exited.clone();
        let error = confirm_then_exit(
            async {
                Err(AppError::new(
                    Code::Storage,
                    "injected Live metadata failure",
                ))
            },
            async { Ok(()) },
            move || *flag.lock().unwrap() = true,
        )
        .await
        .unwrap_err();

        assert_eq!(error.code, Code::Storage);
        assert!(!*exited.lock().unwrap());
    }
}
