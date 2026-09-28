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
    if state
        .close_requested
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
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
        state
            .close_requested
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

pub fn release(state: &AppState) {
    state
        .close_requested
        .store(false, std::sync::atomic::Ordering::SeqCst);
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

/// Runs the confirmed-close commit in safety order. The exit action is only
/// called after Live persistence and Job cancellation have both succeeded.
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
    save_live.await?;
    cancel_jobs.await?;
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

    #[tokio::test]
    async fn live_plus_job_close_saves_live_then_cancels_jobs_then_authorizes_exit() {
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
            ["live-saved", "jobs-cancelled", "exit-authorized"]
        );
    }

    #[tokio::test]
    async fn live_storage_failure_keeps_jobs_and_exit_untouched() {
        let actions = Arc::new(Mutex::new(Vec::new()));
        let live_actions = actions.clone();
        let jobs_actions = actions.clone();
        let exit_actions = actions.clone();

        let error = confirm_then_exit(
            async move {
                live_actions.lock().unwrap().push("live-save-failed");
                Err(AppError::new(
                    Code::Storage,
                    "injected Live metadata failure",
                ))
            },
            async move {
                jobs_actions.lock().unwrap().push("jobs-cancelled");
                Ok(())
            },
            move || exit_actions.lock().unwrap().push("exit-authorized"),
        )
        .await
        .unwrap_err();

        assert_eq!(error.code, Code::Storage);
        assert_eq!(*actions.lock().unwrap(), ["live-save-failed"]);
    }
}
