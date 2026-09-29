//! Adapter UI (AD-1): command/Channel tauri-specta; boot, đóng cửa sổ; nơi
//! duy nhất điều phối chéo feature. `specta_builder()` là nguồn duy nhất
//! liệt kê command/event production, dùng chung cho `lib.rs` (đăng ký thật)
//! và test `export_bindings` (sinh `src/lib/bindings.ts`).

pub mod boot;
pub mod close;

#[cfg(test)]
mod spike_channel;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::Manager;
use tauri_specta::{collect_commands, collect_events, Builder, Event};

use crate::audio::LiveSources;
use crate::consent::{self, ConsentPolicy};
use crate::core::error::{AppError, Code};
use crate::core::id::{JobId, MemoTemplateId, SessionId, TagId, TranscriptId};
use crate::core::paths;
use crate::db::{repo, Db};
use crate::diagnostics::{self, DiagnosticsSummary};
use crate::gemini::{CancellationToken, ConsentSnapshot, KeyTestResult, ModelInfo, ModelKind};
use crate::library;
use crate::live::{LiveEvent, LiveStartParams};
use crate::memo;
use crate::secrets::{KeyId, KeyMetadata};
use crate::settings::{self, Settings, SettingsChanged, TranscribeLanguage};
use crate::transcribe::job::{CancelOutcome, JobEvent};
use crate::transcribe::registry::{self, RerunOutcome, RerunParams, RetranscribeParams};
use crate::transcribe::rerun::{self, RerunScope};
use boot::{ActiveRecordingExport, AppState};

/// Story 3.1: `true` khi `session_id` đang bị `library_session_delete` đánh
/// dấu "deleting" (spec Design Notes AD-1: chỉ `ipc/` đọc/ghi
/// `AppState::deleting`). Nhận thẳng `&Mutex<HashSet<SessionId>>` (không phải
/// `&AppState`) để test được trực tiếp không cần dựng một `AppState` thật.
/// Mutex bị poison đọc như "không đang xoá" thay vì panic -- một panic ở nơi
/// khác giữ lock không được phép chặn mọi command khác vĩnh viễn.
fn is_session_deleting(
    deleting: &std::sync::Mutex<std::collections::HashSet<SessionId>>,
    session_id: SessionId,
) -> bool {
    deleting
        .lock()
        .map(|set| set.contains(&session_id))
        .unwrap_or(false)
}

fn is_session_recovering(
    recovering: &std::sync::Mutex<std::collections::HashSet<SessionId>>,
    session_id: SessionId,
) -> bool {
    recovering
        .lock()
        .map(|set| set.contains(&session_id))
        .unwrap_or(false)
}

fn session_recovering_error() -> AppError {
    AppError::new(Code::Request, "Phiên đang được phục hồi")
}

fn recovering_delete_outcome(
    recovering: &std::sync::Mutex<std::collections::HashSet<SessionId>>,
    session_id: SessionId,
) -> Option<SessionDeleteOutcome> {
    is_session_recovering(recovering, session_id).then_some(SessionDeleteOutcome::Busy)
}

/// Lỗi từ chối dùng chung cho `transcribe_rerun`/`library_proxy_relink`/
/// `library_transcript_export` khi session đích đang bị xoá (spec I/O Matrix
/// "Race Chạy lại/relink": "Lệnh kia trả lỗi `Request` 'đang xoá'").
fn session_deleting_error() -> AppError {
    AppError::new(Code::Request, "Phiên đang được xoá")
}

/// Story 3.4: `true` khi `library_wipe_all` đang xoá toàn bộ dữ liệu (spec
/// Always: "một cờ toàn cục trong `AppState` ... làm mọi writer mới bị từ
/// chối"). `Ordering::SeqCst` cho cả đọc lẫn ghi (`mark`/`unmark` ở
/// `decide_wipe_all`) -- một cờ hiếm khi đổi, không cần thứ tự nới lỏng hơn.
fn is_wiping(wiping: &std::sync::atomic::AtomicBool) -> bool {
    wiping.load(std::sync::atomic::Ordering::SeqCst)
}

/// Lỗi từ chối dùng chung cho mọi writer bị `is_wiping` chặn (spec I/O Matrix
/// "Writer khi đang xoá": "lỗi `Request` 'đang xoá toàn bộ dữ liệu'").
fn wiping_error() -> AppError {
    AppError::new(Code::Request, "Đang xoá toàn bộ dữ liệu")
}

/// Story 3.7: huỷ mọi request `memo_generate` đang bay cho một Phiên --
/// gọi từ `library_session_delete`'s `mark_deleting` closure, ngay sau khi
/// đánh dấu "deleting" (spec Boundaries Always: "xoá Phiên ... huỷ toàn bộ
/// các request memo đang chạy của Phiên liên quan"). Không tự gỡ khoá khỏi
/// registry -- `memo_generate` (đang chờ ở `await`) tự gỡ khi tỉnh dậy, dù
/// nó thức dậy với `Cancelled` hay với `UpsertOutcome::SessionGone` (Phiên
/// đã biến mất khỏi `sessions` lúc commit).
fn cancel_memo_requests_for_session(
    memo_running: &std::sync::Mutex<
        std::collections::HashMap<(SessionId, MemoTemplateId), CancellationToken>,
    >,
    session_id: SessionId,
) {
    if let Ok(running) = memo_running.lock() {
        for (key, token) in running.iter() {
            if key.0 == session_id {
                token.cancel();
            }
        }
    }
}

/// Cùng vai trò [`cancel_memo_requests_for_session`] nhưng cho toàn bộ
/// registry -- gọi từ `library_wipe_all`'s `mark_wiping` closure (spec
/// Boundaries Always: cùng khuôn "huỷ toàn bộ ... liên quan" áp dụng cho mọi
/// Phiên khi xoá sạch dữ liệu).
fn cancel_all_memo_requests(
    memo_running: &std::sync::Mutex<
        std::collections::HashMap<(SessionId, MemoTemplateId), CancellationToken>,
    >,
) {
    if let Ok(running) = memo_running.lock() {
        for token in running.values() {
            token.cancel();
        }
    }
}

/// Chạy một closure blocking trên `spawn_blocking`, gộp lỗi join thành
/// `AppError` category `storage` — dùng cho những command story 2.4 có
/// nhiều bước chạm DB/OS tuần tự (Code Map: mọi I/O chặn chỉ qua
/// `spawn_blocking`, không có `rt-multi-thread`/`fs`).
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))
        .and_then(|inner| inner)
}

/// Trả về version của app. Nguồn duy nhất là `Cargo.toml`
/// (`CARGO_PKG_VERSION`, đọc lúc biên dịch) — không có nơi thứ hai giữ version.
#[tauri::command]
#[specta::specta]
fn app_version() -> Result<String, AppError> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}

/// List microphones, the default microphone, and system-audio availability.
/// Device enumeration is repeated on each call so a refresh observes newly
/// connected or removed devices.
#[tauri::command]
#[specta::specta]
async fn live_sources(
    refresh: bool,
    state: tauri::State<'_, AppState>,
) -> Result<LiveSources, AppError> {
    let capture = state.capture.clone();
    let result = tauri::async_runtime::spawn_blocking(move || capture.live_sources(refresh))
        .await
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))
        .and_then(|inner| inner);
    track_ipc_error(&state.db, result).await
}

/// Open the OS privacy pane for the selected Live input. The next explicit
/// source refresh rechecks permission state; frontend capabilities stay
/// restricted and no generic URL is accepted here.
#[tauri::command]
#[specta::specta]
async fn live_open_permission_settings(
    system: bool,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    let result = blocking(move || {
        #[cfg(target_os = "windows")]
        let url = "ms-settings:privacy-microphone";
        #[cfg(target_os = "macos")]
        let url = if system {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
        } else {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        };
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let url = {
            let _ = system;
            return Err(AppError::new(
                Code::Permission,
                "Audio settings are unavailable on this platform",
            ));
        };
        use tauri_plugin_opener::OpenerExt;
        app.opener()
            .open_url(url, None::<&str>)
            .map_err(|err| AppError::new(Code::Storage, err.to_string()))
    })
    .await;
    track_ipc_error(&state.db, result).await
}

/// Switch the capture source of the running Live session. `source` is one of
/// `system`, `mic:<name>`, or `mixed:<mic>` as returned by `live_sources`. The
/// swap runs inside the Live actor (serialized with start/stop) and is
/// rejected when no session is running, so no capture opens while idle.
#[tauri::command]
#[specta::specta]
async fn live_set_source(
    source: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    let result = async {
        let live = state.live.clone()?;
        live.set_source(source).await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Starts the one process-wide Live session. Consent and model preferences
/// are captured from the durable Rust settings snapshot before capture opens.
#[tauri::command]
#[specta::specta]
async fn live_start(
    source: String,
    language: TranscribeLanguage,
    locale: String,
    tag_ids: Vec<TagId>,
    state: tauri::State<'_, AppState>,
) -> Result<SessionId, AppError> {
    let result = async {
        if is_wiping(&state.wiping) {
            return Err(wiping_error());
        }
        let db = state.db.clone()?;
        let settings = settings::load(&db);
        let live = state.live.clone()?;
        live.start(LiveStartParams {
            source,
            language,
            tag_ids,
            locale: Some(locale),
            ui_language: settings.ui_language,
            model: settings.live_model,
            consent: ConsentSnapshot::new(
                settings.consent_accepted_version,
                settings.consent_declined,
            ),
        })
        .await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Stop capture, flush the final transcript and WAV, then commit the Live
/// session as complete before returning its ID.
#[tauri::command]
#[specta::specta]
async fn live_stop(state: tauri::State<'_, AppState>) -> Result<SessionId, AppError> {
    let result = match state.live.clone() {
        Ok(live) => live.stop().await,
        Err(error) => Err(error),
    };
    track_ipc_error(&state.db, result).await
}

/// Keep WAV capture running after five consecutive Gemini Live setup rejections.
/// This records a session-scoped choice in the LiveSession actor and never
/// starts another Gemini connection for the current session.
#[tauri::command]
#[specta::specta]
async fn live_continue_recording_only(state: tauri::State<'_, AppState>) -> Result<(), AppError> {
    let result = match state.live.clone() {
        Ok(live) => live.continue_recording_only().await,
        Err(error) => Err(error),
    };
    track_ipc_error(&state.db, result).await
}

/// Register a live event Channel and send its state snapshot atomically with
/// registration through the LiveSession actor.
#[tauri::command]
#[specta::specta]
async fn live_subscribe(
    on_event: tauri::ipc::Channel<LiveEvent>,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    let result = match state.live.clone() {
        Ok(live) => live.subscribe(on_event).await,
        Err(error) => Err(error),
    };
    track_ipc_error(&state.db, result).await
}

/// Drop the subscriber registered with `on_event` (the same Channel object the
/// frontend passed to `live_subscribe`). Idempotent.
#[tauri::command]
#[specta::specta]
async fn live_unsubscribe(
    on_event: tauri::ipc::Channel<LiveEvent>,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    let result = match state.live.clone() {
        Ok(live) => live.unsubscribe(on_event.id()).await,
        Err(error) => Err(error),
    };
    track_ipc_error(&state.db, result).await
}

/// Đọc settings qua `db` (kết quả boot đã lưu trong `AppState`). `db` là
/// `Err` khi DB không mở được lúc boot — trả thẳng lỗi đó (category
/// `storage`), không chạm DB thật; mọi trường hợp khác (bảng rỗng, value
/// hỏng) đi qua `settings::load`, không bao giờ lỗi (spec I/O Matrix). Hàm
/// thuần, tách khỏi command Tauri để test được không cần `tauri::State`.
fn get_settings(db: &Result<Arc<Db>, AppError>) -> Result<Settings, AppError> {
    let db = db.clone()?;
    Ok(settings::load(&db))
}

fn get_consent_policy(db: &Result<Arc<Db>, AppError>) -> Result<ConsentPolicy, AppError> {
    let settings = get_settings(db)?;
    Ok(consent::policy(
        settings.consent_accepted_version,
        settings.consent_declined,
    ))
}

fn get_gemini_consent(db: &Result<Arc<Db>, AppError>) -> Result<ConsentSnapshot, AppError> {
    let settings = get_settings(db)?;
    Ok(ConsentSnapshot::new(
        settings.consent_accepted_version,
        settings.consent_declined,
    ))
}

/// Ghi settings rồi gọi `on_saved` đúng một lần — chỉ khi ghi thành công
/// (spec I/O Matrix "Lưu settings": "Lỗi ghi → storage, không phát event";
/// và "DB không mở được": trả `storage`, không chạm DB thật). Hàm thuần,
/// tách khỏi command Tauri để test callback bằng closure đếm mà không cần
/// dựng `AppHandle`/event runtime thật.
fn save_settings_and_notify(
    db: &Result<Arc<Db>, AppError>,
    settings: Settings,
    on_saved: impl FnOnce(&Settings),
) -> Result<(), AppError> {
    let db = db.clone()?;
    settings::save(&db, &settings)?;
    on_saved(&settings);
    Ok(())
}

fn accept_consent_snapshot(mut settings: Settings) -> Settings {
    settings.consent_accepted_version = consent::accepted_version_after_accept();
    settings.consent_declined = false;
    settings
}

fn decline_consent_snapshot(mut settings: Settings) -> Settings {
    settings.consent_accepted_version = consent::accepted_version_after_decline();
    settings.consent_declined = true;
    settings
}

/// Logic đếm lỗi thuần (không async, không `tauri::State`) — tách riêng để
/// test trực tiếp bằng một `Db` tạm, không cần dựng runtime async (spec
/// Always: "Lỗi: tăng theo category khi một IPC command trả `Err`, qua một
/// helper duy nhất ở `ipc/`"). Lỗi ghi đếm chỉ log cảnh báo, không bao giờ
/// thay thế hay làm mất lỗi gốc của command (spec I/O Matrix "Lỗi IPC").
fn note_command_result<T>(db: &Db, result: &Result<T, AppError>) {
    if let Err(err) = result {
        if let Err(record_err) = diagnostics::record_error(db, err.category) {
            tracing::warn!(error = %record_err, "không ghi được bộ đếm lỗi IPC");
        }
    }
}

/// Helper duy nhất bọc quanh kết quả của mọi command trả `Result` (Code Map
/// `ipc/mod.rs`): khi `result` là `Err`, tăng bộ đếm lỗi theo category của nó
/// rồi trả lại đúng `result` ban đầu không đổi. `db` là `Err` (DB không mở
/// được lúc boot) thì bỏ qua việc đếm — không có nơi nào để ghi.
async fn track_ipc_error<T: Send + 'static>(
    db: &Result<Arc<Db>, AppError>,
    result: Result<T, AppError>,
) -> Result<T, AppError> {
    if result.is_err() {
        if let Ok(db) = db.clone() {
            return tauri::async_runtime::spawn_blocking(move || {
                note_command_result(&db, &result);
                result
            })
            .await
            .unwrap_or_else(|join_err| Err(AppError::new(Code::Storage, join_err.to_string())));
        }
    }
    result
}

/// Đọc toàn bộ settings hiện tại — xem [`get_settings`] cho logic thật.
#[tauri::command]
#[specta::specta]
async fn settings_get(state: tauri::State<'_, AppState>) -> Result<Settings, AppError> {
    let db = state.db.clone();
    let result = tauri::async_runtime::spawn_blocking(move || get_settings(&db))
        .await
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))
        .and_then(|inner| inner);
    track_ipc_error(&state.db, result).await
}

/// Return the Rust-owned consent policy and the durable decision in one
/// typed snapshot. A policy read failure is a storage error; no fallback
/// version is invented in the frontend.
#[tauri::command]
#[specta::specta]
async fn consent_policy(state: tauri::State<'_, AppState>) -> Result<ConsentPolicy, AppError> {
    let db = state.db.clone();
    let result = tauri::async_runtime::spawn_blocking(move || get_consent_policy(&db))
        .await
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))
        .and_then(|inner| inner);
    track_ipc_error(&state.db, result).await
}

/// Ghi toàn bộ settings và phát `settingsChanged` — xem
/// [`save_settings_and_notify`] cho logic ghi/thông báo thật; phát event
/// thất bại (ví dụ chưa có cửa sổ nào) không làm command trả lỗi vì dữ liệu
/// đã ghi bền xong, chỉ log cảnh báo.
#[tauri::command]
#[specta::specta]
async fn settings_save(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    settings: Settings,
) -> Result<(), AppError> {
    let db = state.db.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        save_settings_and_notify(&db, settings, |saved| {
            if let Err(err) = SettingsChanged(saved.clone()).emit(&app) {
                tracing::warn!(error = %err, "phát event settingsChanged thất bại");
            }
        })
    })
    .await
    .map_err(|err| AppError::new(Code::Storage, err.to_string()))
    .and_then(|inner| inner);
    track_ipc_error(&state.db, result).await
}

/// Persist acceptance at the current Rust-owned version and emit the full
/// settings snapshot only after the durable write succeeds.
#[tauri::command]
#[specta::specta]
async fn consent_accept(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, AppError> {
    let db = state.db.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let next = accept_consent_snapshot(settings);
        save_settings_and_notify(&db, next.clone(), |saved| {
            if let Err(err) = SettingsChanged(saved.clone()).emit(&app) {
                tracing::warn!(error = %err, "phát event consent accept thất bại");
            }
        })?;
        Ok(next)
    })
    .await
    .map_err(|err| AppError::new(Code::Storage, err.to_string()))
    .and_then(|inner| inner);
    track_ipc_error(&state.db, result).await
}

/// Persist a deliberate decline independently from the accepted-version
/// field, then emit the same full snapshot contract as settings_save.
#[tauri::command]
#[specta::specta]
async fn consent_decline(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, AppError> {
    let db = state.db.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let next = decline_consent_snapshot(settings);
        save_settings_and_notify(&db, next.clone(), |saved| {
            if let Err(err) = SettingsChanged(saved.clone()).emit(&app) {
                tracing::warn!(error = %err, "phát event consent decline thất bại");
            }
        })?;
        Ok(next)
    })
    .await
    .map_err(|err| AppError::new(Code::Storage, err.to_string()))
    .and_then(|inner| inner);
    track_ipc_error(&state.db, result).await
}

/// Return only opaque IDs and masked labels; key material never crosses IPC.
#[tauri::command]
#[specta::specta]
async fn keys_list(state: tauri::State<'_, AppState>) -> Result<Vec<KeyMetadata>, AppError> {
    let secrets = state.secrets.clone();
    let result = tauri::async_runtime::spawn_blocking(move || secrets.list())
        .await
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))
        .and_then(|inner| inner);
    track_ipc_error(&state.db, result).await
}

/// Replace the complete native-store key list, then refresh the actor before
/// replying so no removed key can be allocated after command completion.
#[tauri::command]
#[specta::specta]
async fn keys_set(
    state: tauri::State<'_, AppState>,
    keys: String,
) -> Result<Vec<KeyMetadata>, AppError> {
    let secrets = state.secrets.clone();
    let set_result = tauri::async_runtime::spawn_blocking(move || secrets.set(&keys))
        .await
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))
        .and_then(|inner| inner);
    let result = match set_result {
        Ok(metadata) => match state.key_pool.refresh().await {
            Ok(()) => Ok(metadata),
            Err(err) => Err(err),
        },
        Err(err) => Err(err),
    };
    track_ipc_error(&state.db, result).await
}

/// Delete one native credential entry by opaque ID and invalidate old leases.
#[tauri::command]
#[specta::specta]
async fn keys_delete(
    state: tauri::State<'_, AppState>,
    id: KeyId,
) -> Result<Vec<KeyMetadata>, AppError> {
    let secrets = state.secrets.clone();
    let delete_result = tauri::async_runtime::spawn_blocking(move || secrets.delete(&id))
        .await
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))
        .and_then(|inner| inner);
    let result = match delete_result {
        Ok(metadata) => match state.key_pool.refresh().await {
            Ok(()) => Ok(metadata),
            Err(err) => Err(err),
        },
        Err(err) => Err(err),
    };
    track_ipc_error(&state.db, result).await
}

/// List models through the one Gemini gateway. Consent is read server-side
/// for every call so a stale frontend snapshot can never open transport.
#[tauri::command]
#[specta::specta]
async fn models_list(
    state: tauri::State<'_, AppState>,
    kind: ModelKind,
) -> Result<Vec<ModelInfo>, AppError> {
    let db = state.db.clone();
    let consent_result = tauri::async_runtime::spawn_blocking(move || get_gemini_consent(&db))
        .await
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))
        .and_then(|inner| inner);
    let result = match consent_result {
        Ok(consent) => match state.gateway.clone() {
            Ok(gateway) => {
                gateway
                    .models_list(kind, consent, CancellationToken::new())
                    .await
            }
            Err(err) => Err(err),
        },
        Err(err) => Err(err),
    };
    track_ipc_error(&state.db, result).await
}

/// Validate one opaque key ID through a target-key lease. The actor path
/// deliberately cannot fall back to another key after a 401/403 or quota.
#[tauri::command]
#[specta::specta]
async fn keys_test(
    state: tauri::State<'_, AppState>,
    id: KeyId,
) -> Result<KeyTestResult, AppError> {
    let db = state.db.clone();
    let consent_result = tauri::async_runtime::spawn_blocking(move || get_gemini_consent(&db))
        .await
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))
        .and_then(|inner| inner);
    let result = match consent_result {
        Ok(consent) => match state.gateway.clone() {
            Ok(gateway) => {
                gateway
                    .keys_test(id, consent, CancellationToken::new())
                    .await
            }
            Err(err) => Err(err),
        },
        Err(err) => Err(err),
    };
    track_ipc_error(&state.db, result).await
}

/// Đọc bộ đếm cục bộ (phiên, lỗi theo category, crash) cho Settings → Chẩn
/// đoán — xem [`diagnostics::summary`] cho logic thật.
#[tauri::command]
#[specta::specta]
async fn diagnostics_summary(
    state: tauri::State<'_, AppState>,
) -> Result<DiagnosticsSummary, AppError> {
    let db = state.db.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let db = db?;
        diagnostics::summary(&db)
    })
    .await
    .map_err(|err| AppError::new(Code::Storage, err.to_string()))
    .and_then(|inner| inner);
    track_ipc_error(&state.db, result).await
}

/// Xoá nhật ký allow-list: file cũ bị xoá, file hôm nay bị truncate (spec
/// Boundaries "Xoá nhật ký") — xem [`diagnostics::clear_logs`].
#[tauri::command]
#[specta::specta]
async fn diagnostics_clear_logs(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|err| AppError::new(Code::Storage, err.to_string()));
    let result = tauri::async_runtime::spawn_blocking(move || {
        let log_dir = log_dir?;
        diagnostics::clear_logs(&log_dir).map_err(AppError::from)
    })
    .await
    .map_err(|err| AppError::new(Code::Storage, err.to_string()))
    .and_then(|inner| inner);
    track_ipc_error(&state.db, result).await
}

/// Mở dialog lưu file hệ thống bằng `rfd` (chỉ phía Rust — spec Boundaries:
/// "Frontend không nhận hay gửi đường dẫn") rồi ghi gói chẩn đoán đã build.
/// Huỷ dialog trả `Ok(false)`, không phải lỗi (spec Always).
///
/// macOS bắt buộc dialog file gốc hệ điều hành phải mở trên main thread —
/// gọi thẳng `rfd::FileDialog` từ một worker thread (`spawn_blocking`) không
/// an toàn. Hàm này lên lịch phần gọi dialog thật qua
/// [`tauri::AppHandle::run_on_main_thread`] rồi chặn (blocking `recv`) chờ
/// kết quả về qua một kênh `std::sync::mpsc` — bản thân việc chờ này vẫn
/// chạy trong `spawn_blocking` ở [`diagnostics_export`] nên không chặn
/// runtime async, chỉ dialog thật mới chạy đúng trên main thread.
fn save_diagnostics_bundle(app: &tauri::AppHandle, bundle: &str) -> Result<bool, AppError> {
    let file_name = diagnostics::default_export_file_name(std::time::SystemTime::now());
    let (tx, rx) = std::sync::mpsc::channel::<Option<std::path::PathBuf>>();

    app.run_on_main_thread(move || {
        let picked = rfd::FileDialog::new()
            .set_file_name(&file_name)
            .add_filter("Text", &["txt"])
            .save_file();
        // Người nhận (`rx.recv()` dưới đây) có thể đã bỏ cuộc nếu closure
        // này panic trước khi gửi được — `send` lỗi khi đó chỉ nghĩa là
        // không còn ai chờ, không phải lỗi cần xử lý ở đây.
        let _ = tx.send(picked);
    })
    .map_err(|err| AppError::new(Code::Storage, err.to_string()))?;

    let picked = rx
        .recv()
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))?;

    let Some(path) = picked else {
        return Ok(false);
    };
    std::fs::write(&path, bundle.as_bytes())?;
    Ok(true)
}

fn write_transcript_export(path: &std::path::Path, content: &str) -> Result<(), AppError> {
    std::fs::write(path, content.as_bytes()).map_err(AppError::from)
}

/// Open the native save dialog on Tauri's main thread and write the selected
/// transcript export on the blocking worker. The WebView never receives the
/// chosen path and cancellation is returned as `false` without an error.
fn save_transcript_export(
    app: &tauri::AppHandle,
    default_file_name: String,
    format: library::export::TranscriptExportFormat,
    content: String,
) -> Result<bool, AppError> {
    let filter_label = format.filter_label().to_string();
    let extension = format.extension().to_string();
    let (tx, rx) = std::sync::mpsc::channel::<Option<std::path::PathBuf>>();

    app.run_on_main_thread(move || {
        let picked = rfd::FileDialog::new()
            .set_file_name(&default_file_name)
            .add_filter(&filter_label, &[extension.as_str()])
            .save_file();
        let _ = tx.send(picked);
    })
    .map_err(|err| AppError::new(Code::Storage, err.to_string()))?;

    let picked = rx
        .recv()
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))?;
    let Some(path) = picked else {
        return Ok(false);
    };
    write_transcript_export(&path, &content)?;
    Ok(true)
}

/// Xuất gói chẩn đoán: build bundle (log allow-list + bộ đếm) rồi mở dialog
/// lưu hệ thống trong `spawn_blocking` — bản thân dialog chạy trên main
/// thread qua [`save_diagnostics_bundle`], `spawn_blocking` ở đây chỉ giữ
/// việc chờ kết quả tránh chặn runtime async.
#[tauri::command]
#[specta::specta]
async fn diagnostics_export(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<bool, AppError> {
    let db = state.db.clone();
    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|err| AppError::new(Code::Storage, err.to_string()));
    let bundle_result = tauri::async_runtime::spawn_blocking(move || {
        let log_dir = log_dir?;
        let db = db?;
        let summary = diagnostics::summary(&db)?;
        Ok(diagnostics::build_bundle(&log_dir, &summary))
    })
    .await
    .map_err(|err| AppError::new(Code::Storage, err.to_string()))
    .and_then(|inner| inner);

    let result = match bundle_result {
        Ok(bundle) => {
            let app_for_dialog = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                save_diagnostics_bundle(&app_for_dialog, &bundle)
            })
            .await
            .map_err(|err| AppError::new(Code::Storage, err.to_string()))
            .and_then(|inner| inner)
        }
        Err(err) => Err(err),
    };
    track_ipc_error(&state.db, result).await
}

/// Kết quả `transcribe_start` (spec Always: thứ tự gate "Consent → đuôi file
/// thuộc allow-list → hash + tra `source_hash` trong DB → tra reservation
/// JobRegistry → probe → có key dùng được → tạo Job").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TranscribeStartOutcome {
    Job {
        job_id: JobId,
        session_id: SessionId,
    },
    Existing {
        session_id: SessionId,
    },
    /// Cùng nội dung (`source_hash`) đã có một Job Transcribe đang chờ/chạy,
    /// chưa commit thành Phiên — mở view Job đó thay vì tạo Job thứ hai
    /// (spec Always: "tra reservation JobRegistry (trùng → `ExistingJob`)").
    ExistingJob {
        job_id: JobId,
        session_id: SessionId,
    },
}

/// Pure gate-order decision for `transcribe_start` (spec 2.8 Always: "Consent
/// → đuôi file thuộc allow-list (không I/O) → hash + tra `source_hash`
/// trong DB (trùng → `Existing`) → tra reservation JobRegistry (trùng →
/// `ExistingJob`) → probe (có track audio, có frame) → có key dùng được →
/// tạo Job"), factored out so it can be tested without a real
/// `AppState`/DB/gateway: every I/O step is injected as a closure, so a test
/// can assert both the *outcome* and that a later step's closure was never
/// invoked once an earlier gate rejects (spec I/O Matrix "Trùng Phiên"/
/// "Trùng Job": "không cần key, không Gemini"; "Sai định dạng"/"Không audio":
/// "không Phiên, không Job, không hash"/"không Job").
///
/// `check_extension` runs synchronously (no I/O — spec Always) right after
/// Consent; every other closure is called at most once, strictly in gate
/// order, and only when every prior gate passed.
#[allow(clippy::too_many_arguments)]
async fn decide_transcribe_start<HashFut, LookupFut, ReservationFut, ProbeFut, KeyFut, StartFut>(
    consent_current: bool,
    check_extension: impl FnOnce() -> Result<(), AppError>,
    compute_hash: impl FnOnce() -> HashFut,
    lookup_existing_session: impl FnOnce(String) -> LookupFut,
    lookup_existing_job: impl FnOnce(String) -> ReservationFut,
    probe_media: impl FnOnce() -> ProbeFut,
    has_usable_key: impl FnOnce() -> KeyFut,
    start_job: impl FnOnce(String) -> StartFut,
) -> Result<TranscribeStartOutcome, AppError>
where
    HashFut: std::future::Future<Output = Result<String, AppError>>,
    LookupFut: std::future::Future<Output = Result<Option<SessionId>, AppError>>,
    ReservationFut: std::future::Future<Output = Result<Option<(JobId, SessionId)>, AppError>>,
    ProbeFut: std::future::Future<Output = Result<(), AppError>>,
    KeyFut: std::future::Future<Output = Result<bool, AppError>>,
    StartFut: std::future::Future<Output = Result<registry::StartOutcome, AppError>>,
{
    if !consent_current {
        return Err(AppError::new(
            Code::Auth,
            "Current consent is required before transcription can start",
        ));
    }

    check_extension()?;

    let hash = compute_hash().await?;

    if let Some(session_id) = lookup_existing_session(hash.clone()).await? {
        return Ok(TranscribeStartOutcome::Existing { session_id });
    }

    if let Some((job_id, session_id)) = lookup_existing_job(hash.clone()).await? {
        return Ok(TranscribeStartOutcome::ExistingJob { job_id, session_id });
    }

    probe_media().await?;

    if !has_usable_key().await? {
        return Err(AppError::new(
            Code::Auth,
            "No usable Gemini API key is configured",
        ));
    }

    Ok(match start_job(hash).await? {
        registry::StartOutcome::Started { job_id, session_id } => {
            TranscribeStartOutcome::Job { job_id, session_id }
        }
        // Race giữa `lookup_existing_job` (snapshot đọc) và đây: một lời gọi
        // khác đã thắng và tạo Job trước — actor tự trả `Existing` nguyên tử
        // (spec Always: "Tạo Job trong actor là nguyên tử"), không tạo Job
        // thứ hai.
        registry::StartOutcome::Existing { job_id, session_id } => {
            TranscribeStartOutcome::ExistingJob { job_id, session_id }
        }
    })
}

/// Pure seam proving a new transcribe Job's `StartParams` actually carry
/// `Settings.transcribe_language`/`chunk_minutes` (spec Tasks: "settings
/// transcribe_language / chunk_minutes reach Start/Rerun params") — the
/// smallest testable unit around what was previously three separate
/// `let`s inlined at the `jobs.start(...)` call site.
fn build_start_params(
    settings: &settings::Settings,
    source_path: std::path::PathBuf,
    source_hash: String,
    source_name: Option<String>,
    consent: ConsentSnapshot,
) -> registry::StartParams {
    registry::StartParams {
        source_path,
        source_hash,
        source_name,
        model: settings.transcribe_model.clone(),
        language: settings.transcribe_language,
        chunk_minutes: settings.chunk_minutes,
        consent,
    }
}

async fn transcribe_start_inner(
    state: &AppState,
    path: String,
) -> Result<TranscribeStartOutcome, AppError> {
    if is_wiping(&state.wiping) {
        return Err(wiping_error());
    }
    let db = state.db.clone()?;
    let settings = blocking({
        let db = db.clone();
        move || Ok(settings::load(&db))
    })
    .await?;

    let consent =
        ConsentSnapshot::new(settings.consent_accepted_version, settings.consent_declined);
    let source_path = std::path::PathBuf::from(&path);
    let source_name = source_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    let extension_path = source_path.clone();
    let probe_path = source_path.clone();
    let hash_path = source_path.clone();
    let lookup_db = db.clone();
    let secrets = state.secrets.clone();
    let jobs = state.jobs.clone();
    let reservation_jobs = state.jobs.clone();

    decide_transcribe_start(
        consent.is_current(),
        move || crate::media::check_supported_extension(&extension_path).map(|_| ()),
        move || blocking(move || crate::media::sha256_file(&hash_path)),
        move |hash: String| {
            blocking(move || {
                lookup_db
                    .with_connection(|conn| Ok(repo::sessions::find_by_source_hash(conn, &hash)?))
            })
        },
        move |hash: String| async move {
            let jobs = reservation_jobs?;
            jobs.find_transcribe_by_hash(hash).await
        },
        move || blocking(move || crate::media::probe(&probe_path).map(|_| ())),
        move || async move {
            let keys = blocking(move || secrets.list()).await?;
            Ok(!keys.is_empty())
        },
        move |hash: String| async move {
            let jobs = jobs?;
            jobs.start(build_start_params(
                &settings,
                source_path,
                hash,
                source_name,
                consent,
            ))
            .await
        },
    )
    .await
}

/// Bắt đầu transcribe một file (spec Always: thứ tự gate đầy đủ nằm ở
/// [`transcribe_start_inner`]). `path` là đường dẫn tuyệt đối do caller cung
/// cấp — frontend tự dồn mọi đường vào (dialog `transcribe_pick_files`, kéo
/// thả) về một hàng xử lý tuần tự gọi lệnh này từng file (spec 2.8 Approach).
#[tauri::command]
#[specta::specta]
async fn transcribe_start(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<TranscribeStartOutcome, AppError> {
    let result = transcribe_start_inner(&state, path).await;
    track_ipc_error(&state.db, result).await
}

/// Mở dialog chọn **nhiều** file hệ thống bằng `rfd`, lọc đúng định dạng
/// `transcribe_start` chấp nhận (spec Code Map: "`transcribe_pick_files() ->
/// Vec<String>` dùng `rfd::FileDialog::add_filter(..).pick_files()`"). Huỷ
/// dialog trả `Ok(vec![])`, không phải lỗi — cùng quy ước với
/// [`pick_source_file`]/[`save_diagnostics_bundle`]. Chỉ trả đường dẫn thô;
/// gate thật (đuôi/hash/probe/key) vẫn luôn chạy trong `transcribe_start` cho
/// từng file — dialog này không tự ý bỏ qua bước nào (spec Boundaries:
/// "Frontend không nhận hay gửi đường dẫn" ngoài việc chuyển tiếp Vec này
/// sang `transcribe_start`).
///
/// Cùng kỹ thuật main-thread + kênh `std::sync::mpsc` như
/// [`save_diagnostics_bundle`] — xem doc của nó cho lý do (dialog file gốc
/// hệ điều hành trên macOS bắt buộc chạy trên main thread).
fn pick_source_files(app: &tauri::AppHandle) -> Result<Vec<std::path::PathBuf>, AppError> {
    let (tx, rx) = std::sync::mpsc::channel::<Vec<std::path::PathBuf>>();

    app.run_on_main_thread(move || {
        let picked = rfd::FileDialog::new()
            .add_filter("Media", crate::media::SUPPORTED_EXTENSIONS)
            .pick_files()
            .unwrap_or_default();
        let _ = tx.send(picked);
    })
    .map_err(|err| AppError::new(Code::Storage, err.to_string()))?;

    rx.recv()
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))
}

/// Command IPC cho [`pick_source_files`] — trả `Vec<String>` rỗng khi huỷ
/// dialog (spec I/O Matrix "Chọn file": "Huỷ dialog → không làm gì").
#[tauri::command]
#[specta::specta]
async fn transcribe_pick_files(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, AppError> {
    let result = tauri::async_runtime::spawn_blocking(move || pick_source_files(&app))
        .await
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))
        .and_then(|inner| inner)
        .map(|paths| {
            paths
                .into_iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect()
        });
    track_ipc_error(&state.db, result).await
}

/// Kết quả `transcribe_rerun` (spec I/O Matrix "Chạy lại `missing`/`gap(id)`/
/// `all`", "Không có gì để chạy", "Gọi trùng").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TranscribeRerunOutcome {
    Started {
        job_id: JobId,
    },
    Existing {
        job_id: JobId,
    },
    /// The session already has an in-flight rerun targeting a different
    /// `transcript_id`/`ranges` -- no second Job was enqueued (spec Always:
    /// "Never enqueue a second rerun for the same session"). The frontend
    /// treats this like an error notice, not a success.
    Busy {
        job_id: JobId,
    },
    NothingToRerun,
}

/// Pure gate-order decision for `transcribe_rerun` (spec Tasks: "gate tách
/// hàm test được: Consent → tra Phiên/transcript thuộc Phiên → từ chối
/// `primary` của Phiên live → giải scope thành vùng (`gap_id` phải là gap
/// `chunk_failed`) → `NothingToRerun` nếu rỗng → Proxy tồn tại → có key →
/// Job hiện có hoặc start"). Mirrors [`decide_transcribe_start`]: every
/// closure is called at most once, strictly in this order, only once every
/// earlier gate passed.
#[allow(clippy::too_many_arguments)]
async fn decide_transcribe_rerun<LoadFut, RangesFut, ProxyFut, KeyFut, StartFut>(
    consent_current: bool,
    load_session_and_transcript: impl FnOnce() -> LoadFut,
    resolve_ranges: impl FnOnce(
        &repo::sessions::SessionRow,
        &repo::transcripts::TranscriptRow,
    ) -> RangesFut,
    check_proxy: impl FnOnce(&repo::sessions::SessionRow) -> ProxyFut,
    has_usable_key: impl FnOnce() -> KeyFut,
    start_rerun: impl FnOnce(Vec<rerun::RerunRange>, std::path::PathBuf) -> StartFut,
) -> Result<TranscribeRerunOutcome, AppError>
where
    LoadFut: std::future::Future<
        Output = Result<(repo::sessions::SessionRow, repo::transcripts::TranscriptRow), AppError>,
    >,
    RangesFut: std::future::Future<Output = Result<Vec<rerun::RerunRange>, AppError>>,
    ProxyFut: std::future::Future<Output = Result<std::path::PathBuf, AppError>>,
    KeyFut: std::future::Future<Output = Result<bool, AppError>>,
    StartFut: std::future::Future<Output = Result<RerunOutcome, AppError>>,
{
    if !consent_current {
        return Err(AppError::new(
            Code::Auth,
            "Current consent is required before a rerun can start",
        ));
    }

    let (session, transcript) = load_session_and_transcript().await?;

    if session.kind == "live" && transcript.variant == repo::transcripts::Variant::Primary {
        return Err(AppError::new(
            Code::Request,
            "Không thể Chạy lại primary của một Phiên live",
        ));
    }

    let ranges = resolve_ranges(&session, &transcript).await?;
    if ranges.is_empty() {
        return Ok(TranscribeRerunOutcome::NothingToRerun);
    }

    let proxy_path = check_proxy(&session).await?;

    if !has_usable_key().await? {
        return Err(AppError::new(
            Code::Auth,
            "No usable Gemini API key is configured",
        ));
    }

    Ok(match start_rerun(ranges, proxy_path).await? {
        RerunOutcome::Started { job_id } => TranscribeRerunOutcome::Started { job_id },
        RerunOutcome::Existing { job_id } => TranscribeRerunOutcome::Existing { job_id },
        RerunOutcome::Busy { job_id } => TranscribeRerunOutcome::Busy { job_id },
    })
}

/// Same seam as [`build_start_params`], for a rerun Job's `RerunParams`.
#[allow(clippy::too_many_arguments)]
fn build_rerun_params(
    settings: &settings::Settings,
    session_id: SessionId,
    transcript_id: TranscriptId,
    ranges: Vec<rerun::RerunRange>,
    discard_old: bool,
    proxy_path: std::path::PathBuf,
    consent: ConsentSnapshot,
) -> RerunParams {
    RerunParams {
        session_id,
        transcript_id,
        ranges,
        discard_old,
        proxy_path,
        model: settings.transcribe_model.clone(),
        language: settings.transcribe_language,
        chunk_minutes: settings.chunk_minutes,
        consent,
    }
}

async fn transcribe_rerun_inner(
    state: &AppState,
    session_id: SessionId,
    transcript_id: TranscriptId,
    scope: RerunScope,
) -> Result<TranscribeRerunOutcome, AppError> {
    if is_session_deleting(&state.deleting, session_id) {
        return Err(session_deleting_error());
    }
    if is_session_recovering(&state.recovering, session_id) {
        return Err(session_recovering_error());
    }
    if is_wiping(&state.wiping) {
        return Err(wiping_error());
    }
    let db = state.db.clone()?;
    let data_dir = state.data_dir.clone()?;
    let settings = blocking({
        let db = db.clone();
        move || Ok(settings::load(&db))
    })
    .await?;
    let consent =
        ConsentSnapshot::new(settings.consent_accepted_version, settings.consent_declined);
    let secrets = state.secrets.clone();
    let jobs = state.jobs.clone();

    let load_db = db.clone();
    let resolve_db = db.clone();

    decide_transcribe_rerun(
        consent.is_current(),
        move || {
            blocking(move || {
                load_db.with_connection(|conn| {
                    let session = repo::sessions::get(conn, session_id)?
                        .ok_or_else(|| AppError::new(Code::Request, "Phiên không tồn tại"))?;
                    let transcript = repo::transcripts::get(conn, transcript_id)?
                        .ok_or_else(|| AppError::new(Code::Request, "Transcript không tồn tại"))?;
                    if transcript.session_id != session_id {
                        return Err(AppError::new(
                            Code::Request,
                            "Transcript không thuộc Phiên này",
                        ));
                    }
                    Ok((session, transcript))
                })
            })
        },
        move |session: &repo::sessions::SessionRow,
              _transcript: &repo::transcripts::TranscriptRow| {
            let total_duration_ms = (session.duration_sec * 1000.0).round().max(0.0) as u64;
            blocking(move || {
                resolve_db.with_connection(|conn| {
                    let segments = repo::segments::list_for_transcript(conn, transcript_id)?;
                    rerun::resolve_ranges(scope, &segments, total_duration_ms)
                })
            })
        },
        move |session: &repo::sessions::SessionRow| {
            let ext = session.proxy_ext.clone();
            let data_dir = data_dir.clone();
            async move {
                let ext = ext.ok_or_else(|| {
                    AppError::new(Code::Storage, "Phiên chưa có Proxy đã publish")
                })?;
                let path = paths::proxy_path(&data_dir, session_id, &ext);
                let exists_path = path.clone();
                let exists = blocking(move || Ok(exists_path.exists())).await?;
                if !exists {
                    return Err(AppError::new(Code::Storage, "File Proxy không còn tồn tại"));
                }
                Ok(path)
            }
        },
        move || async move {
            let keys = blocking(move || secrets.list()).await?;
            Ok(!keys.is_empty())
        },
        move |ranges, proxy_path| async move {
            let jobs = jobs?;
            jobs.start_rerun(build_rerun_params(
                &settings,
                session_id,
                transcript_id,
                ranges,
                matches!(scope, RerunScope::All),
                proxy_path,
                consent,
            ))
            .await
        },
    )
    .await
}

/// Chạy lại: vá vùng thiếu, một gap cụ thể, hoặc toàn bộ transcript
/// `primary` của một Phiên (spec Approach). `session_id`/`transcript_id` là
/// kiểu đã kiểm định dạng qua IPC (giống `jobs_cancel(job_id: JobId)`) — gate
/// order thật nằm ở [`decide_transcribe_rerun`].
#[tauri::command]
#[specta::specta]
async fn transcribe_rerun(
    state: tauri::State<'_, AppState>,
    session_id: SessionId,
    transcript_id: TranscriptId,
    scope: RerunScope,
) -> Result<TranscribeRerunOutcome, AppError> {
    let result = transcribe_rerun_inner(&state, session_id, transcript_id, scope).await;
    track_ipc_error(&state.db, result).await
}

/// Transcribe the persisted audio of a finalized live session and overwrite
/// its single transcript. The registry owns queueing and cancel.
#[tauri::command]
#[specta::specta]
async fn transcribe_recording(
    state: tauri::State<'_, AppState>,
    session_id: SessionId,
) -> Result<TranscribeRerunOutcome, AppError> {
    let result = async {
        if is_session_deleting(&state.deleting, session_id) {
            return Err(session_deleting_error());
        }
        if is_session_recovering(&state.recovering, session_id) {
            return Err(session_recovering_error());
        }
        if is_wiping(&state.wiping) {
            return Err(wiping_error());
        }
        let db = state.db.clone()?;
        let root = state.data_dir.clone()?;
        let settings = blocking({
            let db = db.clone();
            move || Ok(settings::load(&db))
        })
        .await?;
        let consent =
            ConsentSnapshot::new(settings.consent_accepted_version, settings.consent_declined);
        if !consent.is_current() {
            return Err(AppError::new(Code::Auth, "Current consent is required"));
        }
        let (session, expected_id) = blocking({
            let db = db.clone();
            move || {
                db.with_connection(|conn| {
                    let session = repo::sessions::get(conn, session_id)?
                        .ok_or_else(|| AppError::new(Code::Request, "Phiên không tồn tại"))?;
                    let expected_id = repo::transcripts::primary_for_session(conn, session_id)?;
                    Ok((session, expected_id))
                })
            }
        })
        .await?;
        if session.kind != "live" || session.status != "complete" {
            return Err(AppError::new(Code::Request, "Recording chưa sẵn sàng"));
        }
        let wav = paths::recording_path(&root, session_id);
        let source_path = if wav.is_file() {
            wav
        } else if let Some(ext) = session.proxy_ext {
            let proxy = paths::proxy_path(&root, session_id, &ext);
            if !proxy.is_file() {
                return Err(AppError::new(Code::Storage, "Recording không còn tồn tại"));
            }
            proxy
        } else {
            return Err(AppError::new(Code::Storage, "Recording không còn tồn tại"));
        };
        let secrets = state.secrets.clone();
        if blocking(move || secrets.list()).await?.is_empty() {
            return Err(AppError::new(
                Code::Auth,
                "No usable Gemini API key is configured",
            ));
        }
        let jobs = state.jobs.clone()?;
        Ok(
            match jobs
                .start_retranscribe(RetranscribeParams {
                    session_id,
                    expected_id,
                    source_path,
                    model: settings.transcribe_model,
                    language: settings.transcribe_language,
                    chunk_minutes: settings.chunk_minutes,
                    consent,
                })
                .await?
            {
                RerunOutcome::Started { job_id } => TranscribeRerunOutcome::Started { job_id },
                RerunOutcome::Existing { job_id } => TranscribeRerunOutcome::Existing { job_id },
                RerunOutcome::Busy { job_id } => TranscribeRerunOutcome::Busy { job_id },
            },
        )
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Đăng ký một Channel nhận snapshot rồi các `JobEvent` tiếp theo (spec
/// Always: "snapshot và đăng ký Channel trong cùng một lệnh actor").
#[tauri::command]
#[specta::specta]
async fn jobs_subscribe(
    state: tauri::State<'_, AppState>,
    on_event: tauri::ipc::Channel<JobEvent>,
) -> Result<(), AppError> {
    let result = async {
        let jobs = state.jobs.clone()?;
        jobs.subscribe(on_event).await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Gỡ đăng ký một Channel đã `jobs_subscribe` trước đó, theo `channel_id`
/// (`Channel::id()` phía frontend) — spec Always: "`jobs_unsubscribe`... gỡ
/// subscriber có `Channel::id()` khớp"; idempotent, `channel_id` không tồn
/// tại vẫn `Ok`.
#[tauri::command]
#[specta::specta]
async fn jobs_unsubscribe(
    state: tauri::State<'_, AppState>,
    channel_id: u32,
) -> Result<(), AppError> {
    let result = async {
        let jobs = state.jobs.clone()?;
        jobs.unsubscribe(channel_id).await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Huỷ một Job — xem [`crate::transcribe::job::CancelOutcome`] cho ý nghĩa
/// kết quả (spec I/O Matrix "Huỷ": "Cancel đến sau commit -> trả kết quả
/// hoàn tất").
#[tauri::command]
#[specta::specta]
async fn jobs_cancel(
    state: tauri::State<'_, AppState>,
    job_id: JobId,
) -> Result<CancelOutcome, AppError> {
    let result = async {
        let jobs = state.jobs.clone()?;
        jobs.cancel(job_id).await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Kết quả tra `id` của route `/session/:id` — có thể là Job đang chạy/chờ
/// (registry cấp `session_id` trước khi commit), một Phiên đã lưu, hay không
/// còn gì (spec I/O Matrix "`/session/:id`").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SessionLookup {
    Job {
        job_id: JobId,
        session_id: SessionId,
    },
    Session {
        session_id: SessionId,
        title: String,
        duration_sec: f64,
        status: String,
        /// `true` khi transcript `primary` hiện tại còn gap `chunk_failed`
        /// (story 2.5) — điều khiển cảnh báo + nút "Chạy lại phần thiếu" ở
        /// `/session/:id`.
        partial: bool,
        transcript_id: Option<TranscriptId>,
    },
    NotFound,
}

async fn library_session_get_inner(
    state: &AppState,
    id: String,
) -> Result<SessionLookup, AppError> {
    let Ok(session_id) = SessionId::try_from(id.as_str()) else {
        return Ok(SessionLookup::NotFound);
    };

    if let Ok(jobs) = state.jobs.clone() {
        let snapshot = jobs.snapshot().await?;
        if let Some(job) = snapshot
            .into_iter()
            .find(|job| job.session_id == session_id)
        {
            return Ok(SessionLookup::Job {
                job_id: job.job_id,
                session_id,
            });
        }
    }

    let db = state.db.clone()?;
    let summary = blocking(move || library::store::get(&db, session_id)).await?;
    Ok(match summary {
        Some(summary) => SessionLookup::Session {
            session_id: summary.session_id,
            title: summary.title,
            duration_sec: summary.duration_sec,
            status: summary.status,
            partial: summary.partial,
            transcript_id: summary.primary_transcript_id,
        },
        None => SessionLookup::NotFound,
    })
}

/// Tra `id` (Job hoặc Phiên) cho route `/session/:id`.
#[tauri::command]
#[specta::specta]
async fn library_session_get(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<SessionLookup, AppError> {
    let result = library_session_get_inner(&state, id).await;
    track_ipc_error(&state.db, result).await
}

/// Đọc chi tiết đầy đủ một Phiên cho `/session/:id` (story 2.7) — xem
/// [`library::store::get_detail`] cho logic thật. Caller (`Session.svelte`)
/// chỉ gọi lệnh này sau khi `library_session_get` đã xác định `id` là một
/// Phiên đã lưu, không phải Job đang chạy (spec Design Notes).
#[tauri::command]
#[specta::specta]
async fn library_session_detail(
    state: tauri::State<'_, AppState>,
    session_id: SessionId,
) -> Result<Option<library::store::SessionDetail>, AppError> {
    let db = state.db.clone();
    let root = state.data_dir.clone();
    let result = async {
        let db = db?;
        let root = root?;
        blocking(move || library::store::get_detail(&db, &root, session_id)).await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Export the transcript currently selected in Session detail. Rust reads the
/// transcript by its opaque ID (including private speaker/gap data), applies
/// the display offset, and owns both the system dialog and file write.
#[tauri::command]
#[specta::specta]
async fn library_transcript_export(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: SessionId,
    transcript_id: TranscriptId,
    format: library::export::TranscriptExportFormat,
    offset_sec: f64,
    gap_labels: library::export::GapLabels,
) -> Result<library::export::TranscriptExportOutcome, AppError> {
    if is_session_deleting(&state.deleting, session_id) {
        let result = Err(session_deleting_error());
        return track_ipc_error(&state.db, result).await;
    }
    if is_wiping(&state.wiping) {
        let result = Err(wiping_error());
        return track_ipc_error(&state.db, result).await;
    }
    let result = async {
        let db = state.db.clone()?;
        let rendered = blocking(move || {
            let data = library::store::get_export_data(&db, session_id, transcript_id)?
                .ok_or_else(|| {
                    AppError::new(Code::Storage, "selected transcript no longer exists")
                })?;
            library::export::render_transcript(&data, format, offset_sec, &gap_labels)
        })
        .await?;

        let has_gaps = rendered.has_gaps;
        let content = rendered.content;
        let default_file_name = format!("transcript-{}.{}", transcript_id, format.extension());
        let app_for_dialog = app.clone();
        let saved = tauri::async_runtime::spawn_blocking(move || {
            save_transcript_export(&app_for_dialog, default_file_name, format, content)
        })
        .await
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))
        .and_then(|inner| inner)?;

        Ok(library::export::TranscriptExportOutcome { saved, has_gaps })
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Một dòng phiên cho danh sách Home (story 2.9) — xem
/// [`repo::sessions::SessionListRow`] cho logic đọc thật. `created_at` giữ
/// dạng `f64` (mili-giây epoch, UTC) chứ không phải `i64`, cùng lý do
/// `SessionDetail::created_at`: specta-typescript cấm xuất kiểu BigInt.
/// `missing_gap_count` là `i32` (không phải `i64`), cùng lý do
/// `SegmentDetail::idx` — không Phiên nào tới gần `i32::MAX` gap. `tag_ids`
/// (story 3.2) là mọi tag đang gắn với Phiên — frontend tra tên qua
/// `tagsStore`/`tags_list` đã tải riêng, không kèm tên ở đây.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionListItem {
    pub session_id: SessionId,
    pub kind: String,
    pub status: String,
    pub recording_available: bool,
    pub title: String,
    pub created_at: f64,
    pub duration_sec: f64,
    pub recovered: bool,
    pub missing_gap_count: i32,
    pub tag_ids: Vec<TagId>,
}

fn session_list_row_to_item(
    root: &std::path::Path,
    row: repo::sessions::SessionListRow,
) -> SessionListItem {
    let recording_available = row.kind == "live" && paths::recording_path(root, row.id).is_file();
    SessionListItem {
        session_id: row.id,
        kind: row.kind,
        status: row.status,
        recording_available,
        title: row.title,
        created_at: row.created_at as f64,
        duration_sec: row.duration_sec,
        recovered: row.recovered,
        missing_gap_count: row.missing_gap_count as i32,
        tag_ids: row.tag_ids,
    }
}

/// Liệt kê mọi Phiên cho Home (story 2.9): một truy vấn, mới nhất trước, kèm
/// `missing_gap_count` (gap `chunk_failed` của transcript `primary`) — xem
/// [`repo::sessions::list_for_home`] cho logic đọc thật.
#[tauri::command]
#[specta::specta]
async fn library_sessions_list(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<SessionListItem>, AppError> {
    let db = state.db.clone();
    let root = state.data_dir.clone();
    let result = async {
        let db = db?;
        let root = root?;
        blocking(move || {
            db.with_connection(|conn| Ok(repo::sessions::list_for_home(conn)?))
                .map(|rows| {
                    rows.into_iter()
                        .map(|row| session_list_row_to_item(&root, row))
                        .collect()
                })
        })
        .await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecordingExportProgress {
    pub processed_seconds: f64,
    pub total_seconds: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RecordingExportFormat {
    Wav,
    Flac,
}

impl RecordingExportFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Flac => "flac",
        }
    }

    fn media_format(self) -> crate::media::RecordingExportFormat {
        match self {
            Self::Wav => crate::media::RecordingExportFormat::Wav,
            Self::Flac => crate::media::RecordingExportFormat::Flac,
        }
    }
}

/// Keep a picked path that already ends in the target extension; otherwise
/// append it. Never rewrite a dotted segment (`a.v2` becomes `a.v2.wav`), so
/// the file the user confirmed in the dialog is the file that gets written.
fn with_export_extension(path: std::path::PathBuf, extension: &str) -> std::path::PathBuf {
    let has_target = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case(extension));
    if has_target {
        return path;
    }
    let mut name = path.into_os_string();
    name.push(".");
    name.push(extension);
    std::path::PathBuf::from(name)
}

/// Open the system save dialog on Tauri's main thread. The picker returns a
/// Rust-only path; only progress crosses IPC while export work is running.
fn pick_recording_export_destination(
    app: &tauri::AppHandle,
    format: RecordingExportFormat,
) -> Result<Option<std::path::PathBuf>, AppError> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let (tx, rx) = std::sync::mpsc::channel::<Option<std::path::PathBuf>>();
        let extension = format.extension();
        let file_name = format!("recording.{extension}");
        app.run_on_main_thread(move || {
            let picked = rfd::FileDialog::new()
                .set_file_name(&file_name)
                .add_filter(
                    if extension == "wav" { "WAV" } else { "FLAC" },
                    &[extension],
                )
                .save_file();
            let _ = tx.send(picked);
        })
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))?;
        let picked = rx
            .recv()
            .map_err(|err| AppError::new(Code::Storage, err.to_string()))?;
        let Some(path) = picked else {
            return Ok(None);
        };
        Ok(Some(with_export_extension(path, format.extension())))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = app;
        Err(AppError::new(
            Code::Permission,
            "Recording export is available on macOS and Windows.",
        ))
    }
}

/// Export a persisted Live Recording. Session kind/status/source are read from
/// SQLite and the canonical path is checked again after the dialog returns.
#[tauri::command]
#[specta::specta]
async fn library_recording_export(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: SessionId,
    format: RecordingExportFormat,
    on_progress: tauri::ipc::Channel<RecordingExportProgress>,
) -> Result<bool, AppError> {
    let cancel_token = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reservation =
        reserve_recording_export(&state.recording_export, session_id, cancel_token.clone());
    if let Err(error) = reservation {
        return track_ipc_error(&state.db, Err(error)).await;
    }

    let result = async {
        if is_wiping(&state.wiping) || is_session_deleting(&state.deleting, session_id) {
            return Err(if is_wiping(&state.wiping) {
                wiping_error()
            } else {
                session_deleting_error()
            });
        }
        let db = state.db.clone();
        let root = state.data_dir.clone();
        blocking(move || {
            let db = db?;
            let root = root?;
            library::store::recording_export_source(&db, &root, session_id)
        })
        .await?;

        let app_for_dialog = app.clone();
        let picked = tauri::async_runtime::spawn_blocking(move || {
            pick_recording_export_destination(&app_for_dialog, format)
        })
        .await
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))??;
        let Some(destination) = picked else {
            return Ok(false);
        };
        if cancel_token.load(std::sync::atomic::Ordering::Acquire) {
            return Ok(false);
        }
        if is_wiping(&state.wiping) || is_session_deleting(&state.deleting, session_id) {
            return Err(if is_wiping(&state.wiping) {
                wiping_error()
            } else {
                session_deleting_error()
            });
        }

        let db = state.db.clone();
        let root = state.data_dir.clone();
        let source = blocking(move || {
            let db = db?;
            let root = root?;
            library::store::recording_export_source(&db, &root, session_id)
        })
        .await?;
        let total_seconds = source.duration_seconds;
        let channel = on_progress;
        let cancel = cancel_token.clone();
        let staging_dir = state
            .data_dir
            .clone()
            .ok()
            .map(|root| crate::core::paths::staging_dir(&root, crate::core::id::JobId::new()));
        let staging_cleanup = staging_dir.clone();
        let outcome = tauri::async_runtime::spawn_blocking(move || {
            let outcome = crate::media::export_recording(
                &source.path,
                &destination,
                staging_dir.as_deref(),
                format.media_format(),
                total_seconds,
                &cancel,
                |processed_seconds| {
                    let _ = channel.send(RecordingExportProgress {
                        processed_seconds,
                        total_seconds,
                    });
                },
            );
            // The per-export staging dir is empty by now; drop it.
            if let Some(dir) = staging_cleanup {
                let _ = std::fs::remove_dir(dir);
            }
            outcome
        })
        .await
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))??;
        Ok(outcome == crate::media::RecordingExportOutcome::Saved)
    }
    .await;

    release_recording_export(&state.recording_export, &cancel_token);
    track_ipc_error(&state.db, result).await
}

/// Takes the one export slot for `session_id`, or refuses when another export
/// already holds it.
fn reserve_recording_export(
    slot: &std::sync::Mutex<Option<ActiveRecordingExport>>,
    session_id: SessionId,
    cancel: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), AppError> {
    let mut current = slot
        .lock()
        .map_err(|_| AppError::new(Code::Storage, "Recording export state is unavailable."))?;
    if current.is_some() {
        return Err(AppError::new(
            Code::Request,
            "Another Recording export is already running.",
        ));
    }
    *current = Some(ActiveRecordingExport { session_id, cancel });
    Ok(())
}

/// Frees the slot, but only if `cancel` still owns it.
fn release_recording_export(
    slot: &std::sync::Mutex<Option<ActiveRecordingExport>>,
    cancel: &Arc<std::sync::atomic::AtomicBool>,
) {
    if let Ok(mut active) = slot.lock() {
        if active
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(&current.cancel, cancel))
        {
            *active = None;
        }
    }
}

/// `true` while an export is reading the WAV of `session_id`, or of any
/// session when `session_id` is `None` (wipe).
fn recording_export_active_for(
    slot: &std::sync::Mutex<Option<ActiveRecordingExport>>,
    session_id: Option<SessionId>,
) -> bool {
    slot.lock()
        .map(|active| {
            active
                .as_ref()
                .is_some_and(|export| session_id.is_none_or(|id| export.session_id == id))
        })
        .unwrap_or(false)
}

#[tauri::command]
#[specta::specta]
fn library_recording_export_cancel(state: tauri::State<'_, AppState>) -> Result<(), AppError> {
    let active = state
        .recording_export
        .lock()
        .map_err(|_| AppError::new(Code::Storage, "Recording export state is unavailable."))?;
    if let Some(export) = active.as_ref() {
        export
            .cancel
            .store(true, std::sync::atomic::Ordering::Release);
    }
    Ok(())
}

/// Kết quả `library_proxy_relink` (spec I/O Matrix "Chọn lại khớp/sai/huỷ",
/// "Phiên live thiếu Proxy"). `LiveUnsupported` gộp cả nhánh Phiên `live`
/// (chặn trước khi mở dialog) lẫn `library::store::RelinkOutcome::
/// NotFileSession` (một Phiên `file` bất thường không có `source_hash`) —
/// cả hai đều báo cùng một thông điệp "chưa có Recording để tái tạo" phía
/// UI (spec I/O Matrix "Phiên live thiếu Proxy").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ProxyRelinkOutcome {
    Relinked,
    HashMismatch,
    Cancelled,
    LiveUnsupported,
}

/// Pure gate-order decision for `library_proxy_relink` (spec Tasks: "gate
/// tách hàm test được"): tra Phiên → chặn Phiên không phải `file` (live —
/// không mở dialog, spec I/O Matrix "Phiên live thiếu Proxy") → mở dialog
/// chọn file (huỷ → `Cancelled`, không làm gì thêm) → so hash + publish qua
/// `library::store::relink_proxy`. Mirrors [`decide_transcribe_start`]/
/// [`decide_transcribe_rerun`]: mỗi closure gọi tối đa một lần, đúng thứ
/// tự, chỉ khi gate trước đã qua.
async fn decide_proxy_relink<LoadFut, PickFut, RelinkFut>(
    load_session: impl FnOnce() -> LoadFut,
    pick_file: impl FnOnce() -> PickFut,
    do_relink: impl FnOnce(std::path::PathBuf) -> RelinkFut,
) -> Result<ProxyRelinkOutcome, AppError>
where
    LoadFut: std::future::Future<Output = Result<repo::sessions::SessionRow, AppError>>,
    PickFut: std::future::Future<Output = Result<Option<std::path::PathBuf>, AppError>>,
    RelinkFut: std::future::Future<Output = Result<library::store::RelinkOutcome, AppError>>,
{
    let session = load_session().await?;
    if session.kind != "file" {
        return Ok(ProxyRelinkOutcome::LiveUnsupported);
    }

    let Some(picked) = pick_file().await? else {
        return Ok(ProxyRelinkOutcome::Cancelled);
    };

    Ok(match do_relink(picked).await? {
        library::store::RelinkOutcome::Relinked => ProxyRelinkOutcome::Relinked,
        library::store::RelinkOutcome::HashMismatch => ProxyRelinkOutcome::HashMismatch,
        library::store::RelinkOutcome::NotFileSession => ProxyRelinkOutcome::LiveUnsupported,
    })
}

/// Mở dialog chọn file hệ thống bằng `rfd` (chỉ phía Rust — spec Boundaries:
/// "không cấp capability dialog/fs cho frontend") cho "Chọn lại file
/// nguồn". Huỷ dialog trả `Ok(None)`, không phải lỗi (spec I/O Matrix "Chọn
/// lại sai": "huỷ dialog: im lặng"). Cùng kỹ thuật main-thread + kênh
/// `std::sync::mpsc` như [`save_diagnostics_bundle`] — xem doc của nó cho lý
/// do (dialog file gốc hệ điều hành trên macOS bắt buộc chạy trên main
/// thread).
fn pick_source_file(app: &tauri::AppHandle) -> Result<Option<std::path::PathBuf>, AppError> {
    let (tx, rx) = std::sync::mpsc::channel::<Option<std::path::PathBuf>>();

    app.run_on_main_thread(move || {
        let picked = rfd::FileDialog::new().pick_file();
        // Giống `save_diagnostics_bundle`: `send` lỗi chỉ nghĩa là không còn
        // ai chờ (closure panic trước đó), không phải lỗi cần xử lý ở đây.
        let _ = tx.send(picked);
    })
    .map_err(|err| AppError::new(Code::Storage, err.to_string()))?;

    rx.recv()
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))
}

async fn library_proxy_relink_inner(
    app: &tauri::AppHandle,
    state: &AppState,
    session_id: SessionId,
) -> Result<ProxyRelinkOutcome, AppError> {
    if is_session_deleting(&state.deleting, session_id) {
        return Err(session_deleting_error());
    }
    if is_wiping(&state.wiping) {
        return Err(wiping_error());
    }
    let db = state.db.clone()?;
    let root = state.data_dir.clone()?;
    let load_db = db.clone();
    let relink_db = db.clone();
    let relink_root = root.clone();
    let app_for_dialog = app.clone();

    decide_proxy_relink(
        move || {
            blocking(move || {
                load_db.with_connection(|conn| {
                    repo::sessions::get(conn, session_id)?
                        .ok_or_else(|| AppError::new(Code::Request, "Phiên không tồn tại"))
                })
            })
        },
        move || blocking(move || pick_source_file(&app_for_dialog)),
        move |picked: std::path::PathBuf| {
            blocking(move || {
                library::store::relink_proxy(&relink_db, &relink_root, session_id, &picked)
            })
        },
    )
    .await
}

/// Chọn lại file nguồn cho một Phiên `file` đã lưu (spec Approach, FR-15):
/// hash file được chọn phải khớp đúng `source_hash` của Phiên trước khi
/// publish Proxy mới — xem [`decide_proxy_relink`] cho thứ tự gate thật và
/// [`library::store::relink_proxy`] cho logic publish/DB.
#[tauri::command]
#[specta::specta]
async fn library_proxy_relink(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: SessionId,
) -> Result<ProxyRelinkOutcome, AppError> {
    let result = library_proxy_relink_inner(&app, &state, session_id).await;
    track_ipc_error(&state.db, result).await
}

/// Đổi tên một Phiên đã lưu (story 3.1, spec Approach "đổi tên inline") — xem
/// [`library::store::rename_session`] cho validate/ghi thật. `Ok(None)` khi
/// Phiên không còn tồn tại (đã bị xoá đồng thời) — frontend coi như không có
/// gì để cập nhật, không phải lỗi.
#[tauri::command]
#[specta::specta]
async fn library_session_rename(
    state: tauri::State<'_, AppState>,
    session_id: SessionId,
    title: String,
) -> Result<Option<String>, AppError> {
    if is_wiping(&state.wiping) {
        let result = Err(wiping_error());
        return track_ipc_error(&state.db, result).await;
    }
    let result = async {
        let db = state.db.clone()?;
        blocking(move || library::store::rename_session(&db, session_id, &title)).await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Kết quả `library_session_delete` (spec I/O Matrix "Xoá phiên rảnh", "Xoá
/// khi có Job").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SessionDeleteOutcome {
    Deleted,
    Busy,
}

/// Pure gate-order decision for `library_session_delete` (spec Design Notes:
/// "Đánh dấu **trước** khi hỏi `is_busy`, nên một Chạy lại bắt đầu sau lần
/// hỏi đó sẽ thấy guard và bị từ chối"; Tasks: "`decide_session_delete` +
/// test (busy, deleted, unmark khi lỗi)"). `unmark_deleting` runs on every
/// exit path — whether `is_busy` returns `Busy`, an error, or `do_delete`
/// itself errors — so a session marked "deleting" is never left stuck that
/// way (spec Design Notes: "gỡ ở mọi nhánh thoát, kể cả lỗi").
async fn decide_session_delete<BusyFut, DeleteFut>(
    mark_deleting: impl FnOnce(),
    is_busy: impl FnOnce() -> BusyFut,
    do_delete: impl FnOnce() -> DeleteFut,
    unmark_deleting: impl FnOnce(),
) -> Result<SessionDeleteOutcome, AppError>
where
    BusyFut: std::future::Future<Output = Result<bool, AppError>>,
    DeleteFut: std::future::Future<Output = Result<(), AppError>>,
{
    mark_deleting();

    let outcome = match is_busy().await {
        Ok(true) => Ok(SessionDeleteOutcome::Busy),
        Ok(false) => do_delete().await.map(|()| SessionDeleteOutcome::Deleted),
        Err(err) => Err(err),
    };

    unmark_deleting();
    outcome
}

/// `true` (Busy) when a Live session is running; otherwise defers to the
/// Job check. Live is asked first so a running session short-circuits.
async fn busy_when_live_running<LiveFut, OtherFut>(
    live_running: LiveFut,
    other_busy: OtherFut,
) -> Result<bool, AppError>
where
    LiveFut: std::future::Future<Output = Result<bool, AppError>>,
    OtherFut: std::future::Future<Output = Result<bool, AppError>>,
{
    if live_running.await? {
        return Ok(true);
    }
    other_busy.await
}

async fn library_session_delete_inner(
    state: &AppState,
    session_id: SessionId,
) -> Result<SessionDeleteOutcome, AppError> {
    if is_wiping(&state.wiping) {
        return Err(wiping_error());
    }
    if let Some(outcome) = recovering_delete_outcome(&state.recovering, session_id) {
        return Ok(outcome);
    }
    let db = state.db.clone()?;
    let root = state.data_dir.clone()?;
    let jobs = state.jobs.clone()?;
    let live = state.live.clone()?;
    let mark_set = state.deleting.clone();
    let unmark_set = state.deleting.clone();
    let export_slot = state.recording_export.clone();

    let memo_running_for_mark = state.memo_running.clone();

    decide_session_delete(
        move || {
            if let Ok(mut set) = mark_set.lock() {
                set.insert(session_id);
            }
            // Story 3.7 spec Boundaries Always: "xoá Phiên ... huỷ toàn bộ
            // các request memo đang chạy của Phiên liên quan" -- huỷ ngay
            // sau khi đánh dấu "deleting" (cùng thời điểm `is_session_deleting`
            // bắt đầu chặn `memo_generate` mới), trước khi kiểm `is_busy`.
            cancel_memo_requests_for_session(&memo_running_for_mark, session_id);
        },
        move || async move {
            // A running Live session owns its row and WAV; never delete under it.
            // An active Recording export is reading the same WAV.
            if recording_export_active_for(&export_slot, Some(session_id)) {
                return Ok(true);
            }
            busy_when_live_running(live.is_running(), jobs.is_busy(session_id)).await
        },
        move || blocking(move || library::store::delete_session(&db, &root, session_id)),
        move || {
            if let Ok(mut set) = unmark_set.lock() {
                set.remove(&session_id);
            }
        },
    )
    .await
}

/// Xoá hẳn một Phiên đã lưu (story 3.1, spec Approach): chặn khi
/// `JobRegistry.is_busy(session_id)` (outcome `Busy`, không xoá gì) — xem
/// [`decide_session_delete`] cho thứ tự gate thật và
/// [`library::store::delete_session`] cho logic xoá DB/media.
#[tauri::command]
#[specta::specta]
async fn library_session_delete(
    state: tauri::State<'_, AppState>,
    session_id: SessionId,
) -> Result<SessionDeleteOutcome, AppError> {
    let result = library_session_delete_inner(&state, session_id).await;
    track_ipc_error(&state.db, result).await
}

/// Mọi tag kèm số Phiên đang gắn (story 3.2, spec Boundaries: "sắp số phiên
/// giảm dần rồi tên") — xem [`library::tags::list_with_counts`] cho logic đọc
/// thật.
#[tauri::command]
#[specta::specta]
async fn tags_list(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<library::tags::TagWithCount>, AppError> {
    let db = state.db.clone();
    let result = async {
        let db = db?;
        blocking(move || library::tags::list_with_counts(&db)).await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Tạo tag mới hoặc trả về tag đã có cùng `name_key` (story 3.2) — xem
/// [`library::tags::create_or_get`] cho chuẩn hoá/validate/idempotent thật.
#[tauri::command]
#[specta::specta]
async fn tags_create(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<library::tags::TagSummary, AppError> {
    if is_wiping(&state.wiping) {
        let result = Err(wiping_error());
        return track_ipc_error(&state.db, result).await;
    }
    let db = state.db.clone();
    let result = async {
        let db = db?;
        blocking(move || library::tags::create_or_get(&db, &name)).await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Gắn một tag cho một Phiên (story 3.2) — chặn khi `session_id` đang bị
/// `library_session_delete` xoá (spec Code Map: "guard `is_session_deleting`
/// cho attach"), cùng lý do các lệnh khác chạm một Phiên đang xoá dở. Xem
/// [`library::tags::attach_tag`] cho validate giới hạn 20/no-op thật.
#[tauri::command]
#[specta::specta]
async fn session_tags_attach(
    state: tauri::State<'_, AppState>,
    session_id: SessionId,
    tag_id: TagId,
) -> Result<(), AppError> {
    if is_session_deleting(&state.deleting, session_id) {
        let result = Err(session_deleting_error());
        return track_ipc_error(&state.db, result).await;
    }
    if is_wiping(&state.wiping) {
        let result = Err(wiping_error());
        return track_ipc_error(&state.db, result).await;
    }
    let db = state.db.clone();
    let result = async {
        let db = db?;
        blocking(move || library::tags::attach_tag(&db, session_id, tag_id)).await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Gỡ một tag khỏi một Phiên (story 3.2) — xem [`library::tags::detach_tag`].
#[tauri::command]
#[specta::specta]
async fn session_tags_detach(
    state: tauri::State<'_, AppState>,
    session_id: SessionId,
    tag_id: TagId,
) -> Result<(), AppError> {
    let db = state.db.clone();
    let result = async {
        let db = db?;
        blocking(move || library::tags::detach_tag(&db, session_id, tag_id)).await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Xoá hẳn một tag toàn cục, gỡ khỏi mọi Phiên (story 3.2) — xem
/// [`library::tags::delete_tag`].
#[tauri::command]
#[specta::specta]
async fn tags_delete(state: tauri::State<'_, AppState>, tag_id: TagId) -> Result<(), AppError> {
    if is_wiping(&state.wiping) {
        let result = Err(wiping_error());
        return track_ipc_error(&state.db, result).await;
    }
    let db = state.db.clone();
    let result = async {
        let db = db?;
        blocking(move || library::tags::delete_tag(&db, tag_id)).await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Đọc ghi chú đã lưu của một Phiên (story 3.5) — `Ok(None)` khi Phiên chưa
/// từng có ghi chú (không phải lỗi). Không có guard `is_wiping`/
/// `is_session_deleting` -- đọc không xung đột với xoá/wipe (spec Boundaries
/// Always chỉ nói `notes_save` bị từ chối).
#[tauri::command]
#[specta::specta]
async fn notes_get(
    state: tauri::State<'_, AppState>,
    session_id: SessionId,
) -> Result<Option<library::notes::NoteSnapshot>, AppError> {
    let db = state.db.clone();
    let result = async {
        let db = db?;
        blocking(move || library::notes::get(&db, session_id)).await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Lưu ghi chú của một Phiên nếu `revision` mới lớn hơn revision đang lưu
/// (story 3.5, spec Boundaries Always) — từ chối khi Phiên đang bị
/// `library_session_delete` xoá hoặc đang `library_wipe_all` (cùng khuôn với
/// `session_tags_attach`/`tags_create`). Xem [`library::notes::save`] cho
/// validate độ dài/upsert có điều kiện thật.
#[tauri::command]
#[specta::specta]
async fn notes_save(
    state: tauri::State<'_, AppState>,
    session_id: SessionId,
    body: String,
    revision: i32,
) -> Result<library::notes::NotesSaveOutcome, AppError> {
    if is_session_deleting(&state.deleting, session_id) {
        let result = Err(session_deleting_error());
        return track_ipc_error(&state.db, result).await;
    }
    if is_wiping(&state.wiping) {
        let result = Err(wiping_error());
        return track_ipc_error(&state.db, result).await;
    }
    let db = state.db.clone();
    let result = async {
        let db = db?;
        blocking(move || library::notes::save(&db, session_id, body, revision)).await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Danh sách Template memo của một locale (story 3.6): hai mẫu mặc định của
/// `locale` (seed lười nếu chưa có) + mọi mẫu người dùng — xem
/// [`memo::templates::list`] cho logic seed/đọc thật. `locale` khác
/// `vi`/`en`/`ja` trả lỗi `Request`.
#[tauri::command]
#[specta::specta]
async fn memo_templates_list(
    state: tauri::State<'_, AppState>,
    locale: String,
) -> Result<Vec<memo::templates::MemoTemplate>, AppError> {
    let db = state.db.clone();
    let result = async {
        let db = db?;
        blocking(move || memo::templates::list(&db, &locale)).await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Tạo một Template memo mới của người dùng (story 3.6) — xem
/// [`memo::templates::create`] cho validate tên/prompt thật.
#[tauri::command]
#[specta::specta]
async fn memo_template_create(
    state: tauri::State<'_, AppState>,
    name: String,
    prompt: String,
) -> Result<memo::templates::MemoTemplate, AppError> {
    let db = state.db.clone();
    let result = async {
        let db = db?;
        blocking(move || memo::templates::create(&db, name, prompt)).await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Sửa tên/prompt của một Template memo, kể cả mẫu mặc định (story 3.6) —
/// xem [`memo::templates::update`].
#[tauri::command]
#[specta::specta]
async fn memo_template_update(
    state: tauri::State<'_, AppState>,
    id: MemoTemplateId,
    name: String,
    prompt: String,
) -> Result<memo::templates::MemoTemplate, AppError> {
    let db = state.db.clone();
    let result = async {
        let db = db?;
        blocking(move || memo::templates::update(&db, id, name, prompt)).await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Xoá một Template memo người dùng (story 3.6) — từ chối mẫu mặc định, xem
/// [`memo::templates::delete`].
#[tauri::command]
#[specta::specta]
async fn memo_template_delete(
    state: tauri::State<'_, AppState>,
    id: MemoTemplateId,
) -> Result<(), AppError> {
    let db = state.db.clone();
    let result = async {
        let db = db?;
        blocking(move || memo::templates::delete(&db, id)).await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Ghi lại tên/prompt gốc của hai mẫu mặc định của `locale` (story 3.6) —
/// xem [`memo::templates::restore_defaults`].
#[tauri::command]
#[specta::specta]
async fn memo_templates_restore_defaults(
    state: tauri::State<'_, AppState>,
    locale: String,
) -> Result<Vec<memo::templates::MemoTemplate>, AppError> {
    let db = state.db.clone();
    let result = async {
        let db = db?;
        blocking(move || memo::templates::restore_defaults(&db, &locale)).await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Kết quả `memo_generate` (story 3.7, spec Boundaries Always: "Kết quả
/// lệnh: `Generated(Memo) | Cancelled | AlreadyRunning`; lỗi trả `AppError`
/// đúng category"). `AlreadyRunning` chỉ quyết định được ở đây (registry của
/// `ipc::`), không ở `memo::generate::run` (spec: "registry ... chỉ `ipc/`
/// điều phối") -- đó là lý do nó không nằm trong `memo::generate::GenerateOutcome`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MemoGenerateOutcome {
    Generated { memo: memo::generate::MemoView },
    Cancelled,
    AlreadyRunning,
}

async fn memo_generate_inner(
    state: &AppState,
    session_id: SessionId,
    template_id: MemoTemplateId,
    transcript_id: TranscriptId,
    locale: String,
) -> Result<MemoGenerateOutcome, AppError> {
    if is_session_deleting(&state.deleting, session_id) {
        return Err(session_deleting_error());
    }
    if is_wiping(&state.wiping) {
        return Err(wiping_error());
    }
    let db = state.db.clone()?;
    let gateway = state.gateway.clone()?;
    let consent_db = db.clone();
    let consent = tauri::async_runtime::spawn_blocking(move || get_gemini_consent(&Ok(consent_db)))
        .await
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))
        .and_then(|inner| inner)?;

    // Story 3.7 spec Boundaries Always: "mỗi cặp (Phiên, Template) tối đa
    // một request đang chạy ... gọi trùng trả `AlreadyRunning` không tạo
    // request mới" -- khoá/mở registry quanh đúng một lần gọi
    // `memo::generate::run`, không giữ lock qua await đó (chỉ giữ lúc kiểm
    // tra + chèn/gỡ khoá).
    let key = (session_id, template_id);
    let cancellation = {
        let mut running = state
            .memo_running
            .lock()
            .map_err(|_| AppError::new(Code::Storage, "memo registry mutex poisoned"))?;
        if running.contains_key(&key) {
            return Ok(MemoGenerateOutcome::AlreadyRunning);
        }
        let token = CancellationToken::new();
        running.insert(key, token.clone());
        token
    };

    let outcome = memo::generate::run_for_source(
        &db,
        &gateway,
        session_id,
        template_id,
        Some(transcript_id),
        &locale,
        consent,
        cancellation,
    )
    .await;

    // Gỡ khoá ở mọi nhánh thoát (kể cả lỗi) trước khi trả kết quả -- cùng
    // khuôn `decide_session_delete`'s `unmark_deleting`.
    if let Ok(mut running) = state.memo_running.lock() {
        running.remove(&key);
    }

    Ok(match outcome? {
        memo::generate::GenerateOutcome::Generated(memo) => MemoGenerateOutcome::Generated { memo },
        memo::generate::GenerateOutcome::Cancelled => MemoGenerateOutcome::Cancelled,
    })
}

/// Sinh (hoặc sinh lại) memo của một cặp (Phiên, Template) -- xem
/// [`memo_generate_inner`] cho gate/registry thật và [`memo::generate::run`]
/// cho luồng gọi Gemini/commit thật. Await tới khi xong (≤ 90 s + chờ key,
/// spec Design Notes) -- frontend là một store singleton nên promise vẫn
/// được xử lý sau khi panel Memo unmount (toast thay vì cập nhật UI trực
/// tiếp).
#[tauri::command]
#[specta::specta]
async fn memo_generate(
    state: tauri::State<'_, AppState>,
    session_id: SessionId,
    template_id: MemoTemplateId,
    transcript_id: TranscriptId,
    locale: String,
) -> Result<MemoGenerateOutcome, AppError> {
    let result = memo_generate_inner(&state, session_id, template_id, transcript_id, locale).await;
    track_ipc_error(&state.db, result).await
}

/// Đọc memo đã cache của một cặp (Phiên, Template), không bao giờ gọi
/// Gemini (spec Boundaries Always: "Mở lại memo chỉ đọc DB") — xem
/// [`memo::generate::view`] cho logic đọc + suy hai cờ provenance thật.
#[tauri::command]
#[specta::specta]
async fn memo_get(
    state: tauri::State<'_, AppState>,
    session_id: SessionId,
    template_id: MemoTemplateId,
    transcript_id: TranscriptId,
) -> Result<Option<memo::generate::MemoView>, AppError> {
    let db = state.db.clone();
    let result = async {
        let db = db?;
        blocking(move || {
            memo::generate::view_for_source(&db, session_id, template_id, Some(transcript_id))
        })
        .await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Huỷ request `memo_generate` đang chạy của một cặp (Phiên, Template) nếu
/// có -- không lỗi khi không có gì đang chạy (idempotent, spec I/O Matrix
/// "Huỷ": "đang chạy, `memo_cancel` → `Cancelled`, memo cũ giữ"; gọi khi
/// không có gì đang chạy chỉ là no-op vô hại).
#[tauri::command]
#[specta::specta]
async fn memo_cancel(
    state: tauri::State<'_, AppState>,
    session_id: SessionId,
    template_id: MemoTemplateId,
) -> Result<(), AppError> {
    if let Ok(running) = state.memo_running.lock() {
        if let Some(token) = running.get(&(session_id, template_id)) {
            token.cancel();
        }
    }
    let result: Result<(), AppError> = Ok(());
    track_ipc_error(&state.db, result).await
}

/// Đường dẫn mặc định để xuất memo `.md` -- tên Phiên đã làm sạch (spec
/// Boundaries Always: "tên file mặc định từ tên Phiên đã làm sạch + `.md`").
/// Chỉ là một gợi ý tên file cho dialog lưu hệ thống, không bao giờ dùng làm
/// đường dẫn thật trong Container (spec Never) -- đó là lý do hàm này khác
/// hẳn `library_transcript_export`'s tên file dựa trên `transcript_id` đối
/// (opaque, không cần làm sạch).
fn sanitize_file_name_component(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_control() || "/\\:*?\"<>|".contains(ch) {
                '_'
            } else {
                ch
            }
        })
        .collect();
    let truncated: String = cleaned.trim().chars().take(150).collect();
    if truncated.is_empty() {
        "memo".to_string()
    } else {
        truncated
    }
}

/// Cùng kỹ thuật `save_transcript_export` (dialog trên main thread qua
/// `run_on_main_thread` + kênh `std::sync::mpsc`) -- xem doc của nó cho lý
/// do. Ghi bằng [`write_transcript_export`] (tên lịch sử, hàm chỉ là
/// `std::fs::write` -- dùng lại được cho bất kỳ nội dung text nào, không chỉ
/// transcript).
fn save_memo_export(
    app: &tauri::AppHandle,
    default_file_name: String,
    content: String,
) -> Result<bool, AppError> {
    let (tx, rx) = std::sync::mpsc::channel::<Option<std::path::PathBuf>>();
    app.run_on_main_thread(move || {
        let picked = rfd::FileDialog::new()
            .set_file_name(&default_file_name)
            .add_filter("Markdown", &["md"])
            .save_file();
        let _ = tx.send(picked);
    })
    .map_err(|err| AppError::new(Code::Storage, err.to_string()))?;
    let picked = rx
        .recv()
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))?;
    let Some(path) = picked else {
        return Ok(false);
    };
    write_transcript_export(&path, &content)?;
    Ok(true)
}

async fn memo_export_inner(
    app: &tauri::AppHandle,
    state: &AppState,
    session_id: SessionId,
    template_id: MemoTemplateId,
) -> Result<bool, AppError> {
    if is_session_deleting(&state.deleting, session_id) {
        return Err(session_deleting_error());
    }
    if is_wiping(&state.wiping) {
        return Err(wiping_error());
    }
    let db = state.db.clone()?;
    let (content, default_file_name) = blocking(move || {
        let memo = memo::generate::view(&db, session_id, template_id)?
            .ok_or_else(|| AppError::new(Code::Request, "Memo chưa tồn tại"))?;
        let title = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?))?
            .map(|row| row.title)
            .unwrap_or_default();
        Ok((
            memo.body,
            format!("{}.md", sanitize_file_name_component(&title)),
        ))
    })
    .await?;

    let app_for_dialog = app.clone();
    blocking(move || save_memo_export(&app_for_dialog, default_file_name, content)).await
}

/// Tải Copy/Tải `.md` của một memo -- xem [`memo_export_inner`] cho luồng
/// đọc + dialog + ghi thật, cùng mẫu `library_transcript_export`.
#[tauri::command]
#[specta::specta]
async fn memo_export(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: SessionId,
    template_id: MemoTemplateId,
) -> Result<bool, AppError> {
    let result = memo_export_inner(&app, &state, session_id, template_id).await;
    track_ipc_error(&state.db, result).await
}

/// Mở một URL `http(s)` bằng trình mở mặc định của OS (spec Boundaries
/// Always: "chỉ mở URL `http(s)` qua lệnh Rust `open_external_url` dùng
/// opener từ Rust (không nới capability frontend)"; spec Never: "Không nới
/// `opener:allow-open-url` trong capability" -- lệnh này gọi thẳng
/// `OpenerExt::open_url` từ Rust, frontend không bao giờ có quyền
/// `opener:allow-open-url` của riêng nó). Từ chối bất kỳ scheme nào khác
/// `http`/`https` (spec: link trong Markdown chỉ được phép mở URL đó) trước
/// khi chạm opener.
#[tauri::command]
#[specta::specta]
async fn open_external_url(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    url: String,
) -> Result<(), AppError> {
    let result = blocking(move || {
        let parsed = reqwest::Url::parse(&url)
            .map_err(|_| AppError::new(Code::Request, "URL không hợp lệ"))?;
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err(AppError::new(Code::Request, "Chỉ mở được URL http(s)"));
        }
        use tauri_plugin_opener::OpenerExt;
        app.opener()
            .open_url(url, None::<&str>)
            .map_err(|err| AppError::new(Code::Storage, err.to_string()))
    })
    .await;
    track_ipc_error(&state.db, result).await
}

/// Số liệu Settings → Lưu trữ (story 3.4) — xem [`library::store::storage_stats`]
/// cho logic đo thật (Media đệ quy dưới `media/`, DB = `app.db` +
/// `-wal`/`-shm`, số Phiên = số dòng `sessions`).
#[tauri::command]
#[specta::specta]
async fn library_storage_stats(
    state: tauri::State<'_, AppState>,
) -> Result<library::store::StorageStats, AppError> {
    let result = async {
        let db = state.db.clone()?;
        let root = state.data_dir.clone()?;
        blocking(move || library::store::storage_stats(&db, &root)).await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Mở `<data_dir>` bằng trình quản lý file của OS (spec Never: "Không mở
/// quyền `opener` rộng cho frontend" — gọi thẳng `OpenerExt::open_path` từ
/// Rust, frontend chỉ nhận `Result<(), AppError>`, không bao giờ thấy đường
/// dẫn thật). Lỗi mở (OS/plugin) trả category `storage` để UI hiện lỗi
/// inline (spec Always).
#[tauri::command]
#[specta::specta]
async fn library_open_data_dir(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    let result = async {
        let root = state.data_dir.clone()?;
        blocking(move || {
            use tauri_plugin_opener::OpenerExt;
            app.opener()
                .open_path(root.to_string_lossy().into_owned(), None::<&str>)
                .map_err(|err| AppError::new(Code::Storage, err.to_string()))
        })
        .await
    }
    .await;
    track_ipc_error(&state.db, result).await
}

/// Kết quả `library_wipe_all` (spec I/O Matrix "Xoá thành công", "Đang có
/// Job").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum WipeAllOutcome {
    Wiped,
    Busy,
}

/// Pure gate-order decision for `library_wipe_all` — cùng khuôn
/// [`decide_session_delete`] (spec Design Notes: "Cờ `wiping` toàn cục song
/// song với `deleting` per-session ... đặt trước khi hỏi Job bận nên Job bắt
/// đầu sau đó thấy cờ và bị từ chối"). `unmark_wiping` chạy trên mọi nhánh
/// thoát -- bận, lỗi hỏi bận, hay chính `do_wipe` lỗi -- để cờ không bao giờ
/// kẹt `true` (spec Always: "gỡ ở mọi nhánh thoát").
async fn decide_wipe_all<BusyFut, WipeFut>(
    mark_wiping: impl FnOnce(),
    any_active_job: impl FnOnce() -> BusyFut,
    do_wipe: impl FnOnce() -> WipeFut,
    unmark_wiping: impl FnOnce(),
) -> Result<WipeAllOutcome, AppError>
where
    BusyFut: std::future::Future<Output = Result<bool, AppError>>,
    WipeFut: std::future::Future<Output = Result<(), AppError>>,
{
    mark_wiping();

    let outcome = match any_active_job().await {
        Ok(true) => Ok(WipeAllOutcome::Busy),
        Ok(false) => do_wipe().await.map(|()| WipeAllOutcome::Wiped),
        Err(err) => Err(err),
    };

    unmark_wiping();
    outcome
}

async fn library_wipe_all_inner(state: &AppState) -> Result<WipeAllOutcome, AppError> {
    let db = state.db.clone()?;
    let root = state.data_dir.clone()?;
    let jobs = state.jobs.clone()?;
    let live = state.live.clone()?;
    let mark_wiping = state.wiping.clone();
    let unmark_wiping = state.wiping.clone();
    let export_slot = state.recording_export.clone();
    let memo_running_for_mark = state.memo_running.clone();

    decide_wipe_all(
        move || {
            mark_wiping.store(true, std::sync::atomic::Ordering::SeqCst);
            cancel_all_memo_requests(&memo_running_for_mark);
        },
        move || async move {
            if recording_export_active_for(&export_slot, None) {
                return Ok(true);
            }
            busy_when_live_running(live.is_running(), async {
                Ok(!jobs.snapshot().await?.is_empty())
            })
            .await
        },
        move || blocking(move || library::store::wipe_all(&db, &root)),
        move || unmark_wiping.store(false, std::sync::atomic::Ordering::SeqCst),
    )
    .await
}

/// Xoá toàn bộ dữ liệu họp (story 3.4, spec Approach): chặn khi có Job chưa
/// kết thúc (outcome `Busy`, không xoá gì) — xem [`decide_wipe_all`] cho thứ
/// tự gate thật và [`library::store::wipe_all`] cho logic xoá DB/media thật.
#[tauri::command]
#[specta::specta]
async fn library_wipe_all(state: tauri::State<'_, AppState>) -> Result<WipeAllOutcome, AppError> {
    let result = library_wipe_all_inner(&state).await;
    track_ipc_error(&state.db, result).await
}

/// Người dùng xác nhận đóng app: flush Live tới WAV/metadata bền vững, huỷ
/// Jobs đang chạy, rồi mới cho phép Tauri thoát. Proxy không nằm trên đường
/// đóng để một encode chậm không giữ app mở.
#[tauri::command]
#[specta::specta]
async fn app_close_confirm(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    let live = state.live.clone();
    let save_live = async move {
        if let Ok(live) = live {
            let running = live.clone();
            close::save_live_for_close(
                move || async move { running.is_running().await },
                move || async move { live.stop_for_close().await.map(|_| ()) },
            )
            .await?;
        }
        Ok(())
    };

    let jobs = state.jobs.clone();
    let cancel_jobs = async move {
        if let Ok(jobs) = jobs {
            let snapshot_jobs = jobs.clone();
            close::cancel_jobs_until_empty(
                move || {
                    let jobs = snapshot_jobs.clone();
                    async move {
                        Ok(jobs
                            .snapshot()
                            .await?
                            .into_iter()
                            .map(|job| job.job_id)
                            .collect::<Vec<_>>())
                    }
                },
                move |job_id| {
                    let jobs = jobs.clone();
                    async move {
                        let _ = jobs.cancel(job_id).await;
                    }
                },
                std::time::Duration::from_secs(4),
                std::time::Duration::from_millis(100),
            )
            .await?;
        }
        Ok(())
    };

    let result = close::confirm_then_exit(save_live, cancel_jobs, || {
        close::authorize_exit(&state);
        app.exit(0);
    })
    .await;
    if let Err(error) = result {
        close::release(&state);
        return Err(error);
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
fn app_close_stay(state: tauri::State<'_, AppState>) {
    close::release(&state);
}

/// Danh sách command/event production — nguồn duy nhất, dùng chung cho
/// `lib.rs` (đăng ký `invoke_handler`/`mount_events` thật) và test
/// `export_bindings` (sinh `src/lib/bindings.ts`). Không đăng ký gì từ
/// `spike_channel` ở đây.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            app_version,
            live_sources,
            live_open_permission_settings,
            live_set_source,
            live_start,
            live_stop,
            live_continue_recording_only,
            live_subscribe,
            live_unsubscribe,
            settings_get,
            settings_save,
            consent_policy,
            consent_accept,
            consent_decline,
            keys_list,
            keys_set,
            keys_delete,
            models_list,
            keys_test,
            diagnostics_summary,
            diagnostics_clear_logs,
            diagnostics_export,
            transcribe_start,
            transcribe_pick_files,
            transcribe_rerun,
            transcribe_recording,
            jobs_subscribe,
            jobs_unsubscribe,
            jobs_cancel,
            library_session_get,
            library_session_detail,
            library_transcript_export,
            library_sessions_list,
            library_recording_export,
            library_recording_export_cancel,
            library_proxy_relink,
            library_session_rename,
            library_session_delete,
            tags_list,
            tags_create,
            session_tags_attach,
            session_tags_detach,
            tags_delete,
            notes_get,
            notes_save,
            memo_templates_list,
            memo_template_create,
            memo_template_update,
            memo_template_delete,
            memo_templates_restore_defaults,
            memo_generate,
            memo_get,
            memo_cancel,
            memo_export,
            open_external_url,
            library_storage_stats,
            library_open_data_dir,
            library_wipe_all,
            app_close_confirm,
            app_close_stay,
        ])
        .events(collect_events![
            SettingsChanged,
            close::CloseRequested,
            boot::LiveRecoveryCompleted
        ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::error::Category;
    use crate::settings::Theme;
    use specta_typescript::Typescript;

    /// Nguồn duy nhất sinh `src/lib/bindings.ts`; CI chạy lại test này rồi
    /// `git diff --exit-code` để chặn binding lệch bản sinh.
    #[test]
    fn export_bindings() {
        specta_builder()
            .export(Typescript::default(), "../src/lib/bindings.ts")
            .expect("failed to export typescript bindings");
        let bindings_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/bindings.ts");
        let bindings = std::fs::read_to_string(bindings_path).unwrap();
        assert!(
            bindings.contains("sinceMs: number"),
            "reconnecting sinceMs must stay a JavaScript number in the generated contract"
        );
        assert!(
            bindings.contains("seq: number"),
            "Live sequence counters must stay JavaScript numbers in the generated contract"
        );
    }

    #[test]
    fn transcript_export_write_saves_the_rendered_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("transcript.txt");
        write_transcript_export(&path, "[00:00] hello").unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "[00:00] hello");
    }

    #[test]
    fn transcript_export_write_errors_for_an_unwritable_destination() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing").join("transcript.txt");
        let error = write_transcript_export(&path, "hello").unwrap_err();
        assert_eq!(error.category, Category::Storage);
    }

    // IPC wiring (spec Tasks): `Settings.transcribe_language`/`chunk_minutes`
    // must reach a new Job's `StartParams`/`RerunParams` -- proven at the
    // pure `build_start_params`/`build_rerun_params` seam instead of driving
    // the whole `transcribe_start_inner`/`transcribe_rerun_inner` gate chain
    // through a real DB.

    #[test]
    fn build_start_params_carries_settings_language_and_chunk_minutes() {
        let settings = settings::Settings {
            transcribe_model: "gemini-flash-lite-latest".to_string(),
            transcribe_language: crate::settings::TranscribeLanguage::Ja,
            chunk_minutes: 17,
            ..Default::default()
        };
        let consent = ConsentSnapshot::new(1, false);

        let params = build_start_params(
            &settings,
            std::path::PathBuf::from("/tmp/a.wav"),
            "hash".to_string(),
            Some("a.wav".to_string()),
            consent,
        );

        assert_eq!(params.language, crate::settings::TranscribeLanguage::Ja);
        assert_eq!(params.chunk_minutes, 17);
        assert_eq!(params.model, "gemini-flash-lite-latest");
        assert_eq!(params.consent, consent);
    }

    #[test]
    fn build_rerun_params_carries_settings_language_and_chunk_minutes() {
        let settings = settings::Settings {
            transcribe_model: "gemini-flash-lite-latest".to_string(),
            transcribe_language: crate::settings::TranscribeLanguage::Vi,
            chunk_minutes: 3,
            ..Default::default()
        };
        let consent = ConsentSnapshot::new(1, false);
        let session_id = SessionId::new();
        let transcript_id = TranscriptId::new();

        let params = build_rerun_params(
            &settings,
            session_id,
            transcript_id,
            one_range(),
            true,
            std::path::PathBuf::from("/tmp/proxy.flac"),
            consent,
        );

        assert_eq!(params.language, crate::settings::TranscribeLanguage::Vi);
        assert_eq!(params.chunk_minutes, 3);
        assert_eq!(params.model, "gemini-flash-lite-latest");
        assert!(params.discard_old);
        assert_eq!(params.session_id, session_id);
        assert_eq!(params.transcript_id, transcript_id);
        assert_eq!(params.consent, consent);
    }

    // I/O Matrix "DB không mở được": settings_get/settings_save trả AppError
    // category storage, không chạm DB thật. Test qua `get_settings`/
    // `save_settings_and_notify` trực tiếp (không cần tauri::State thật).

    #[test]
    fn get_settings_returns_storage_error_when_db_failed_to_open() {
        let db_state: Result<Arc<Db>, AppError> = Err(AppError::new(Code::Storage, "disk full"));

        let err = get_settings(&db_state)
            .expect_err("DB lỗi từ boot phải trả lỗi ngay, không chạm DB thật");

        assert_eq!(err.category, Category::Storage);
    }

    #[test]
    fn save_settings_and_notify_returns_storage_error_and_does_not_notify_when_db_failed_to_open() {
        let db_state: Result<Arc<Db>, AppError> = Err(AppError::new(Code::Storage, "disk full"));
        let mut calls = 0u32;

        let err = save_settings_and_notify(
            &db_state,
            Settings {
                theme: Theme::Dark,
                ..Default::default()
            },
            |_| calls += 1,
        )
        .expect_err("DB lỗi từ boot phải trả lỗi ngay, không phát event");

        assert_eq!(err.category, Category::Storage);
        assert_eq!(calls, 0, "không được gọi callback khi DB không mở được");
    }

    // I/O Matrix "Lưu settings": ghi bền phát callback đúng một lần với giá
    // trị mới; ghi lỗi không phát callback nào, trả lỗi category storage.

    #[test]
    fn save_settings_and_notify_calls_callback_once_with_new_value_on_success() {
        let dir = tempfile::tempdir().unwrap();
        let db: Result<Arc<Db>, AppError> = Ok(Arc::new(Db::open(dir.path()).unwrap()));
        let mut calls: Vec<Settings> = Vec::new();

        save_settings_and_notify(
            &db,
            Settings {
                theme: Theme::Dark,
                ..Default::default()
            },
            |saved| calls.push(saved.clone()),
        )
        .expect("ghi thành công phải trả Ok");

        assert_eq!(
            calls,
            vec![Settings {
                theme: Theme::Dark,
                ..Default::default()
            }]
        );
    }

    #[test]
    fn save_settings_and_notify_does_not_notify_when_write_fails() {
        let dir = tempfile::tempdir().unwrap();
        let db_inner = Db::open(dir.path()).unwrap();
        // Xoá bảng `settings` để buộc `settings::save` (qua
        // `repo::settings::upsert_many`) lỗi — mô phỏng một lần ghi thất bại
        // thật trên DB đã mở được (khác với hàng "DB không mở được" ở trên).
        db_inner
            .with_connection(|conn| {
                conn.execute("DROP TABLE settings", [])?;
                Ok(())
            })
            .unwrap();
        let db: Result<Arc<Db>, AppError> = Ok(Arc::new(db_inner));
        let mut calls = 0u32;

        let err = save_settings_and_notify(
            &db,
            Settings {
                theme: Theme::Dark,
                ..Default::default()
            },
            |_| calls += 1,
        )
        .expect_err("ghi lỗi (bảng đã bị xoá) phải trả Err, không gọi callback");

        assert_eq!(err.category, Category::Storage);
        assert_eq!(calls, 0, "không được gọi callback khi ghi lỗi");
    }

    // I/O Matrix "Lỗi IPC": command trả Err(auth) -> bộ đếm auth +1; ghi đếm
    // lỗi không làm đổi lỗi gốc. Test trực tiếp `note_command_result` (logic
    // thuần dùng bởi `track_ipc_error`) với một `Db` tạm, không cần runtime
    // async/tauri::State.

    #[test]
    fn note_command_result_increments_the_erroring_category_and_keeps_the_error_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        let original: Result<u8, AppError> = Err(AppError::new(Code::Auth, "key rejected"));

        note_command_result(&db, &original);

        let summary = crate::diagnostics::summary(&db).unwrap();
        let auth_count = summary
            .errors_by_category
            .iter()
            .find(|row| row.category == Category::Auth)
            .unwrap()
            .count;
        assert_eq!(auth_count, 1);
        // `original` vẫn còn nguyên vẹn sau khi đếm — không bị đổi/tiêu thụ.
        assert!(matches!(&original, Err(error) if error.category == Category::Auth));
    }

    #[test]
    fn note_command_result_does_nothing_on_ok() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        let ok: Result<u8, AppError> = Ok(1);

        note_command_result(&db, &ok);

        let summary = crate::diagnostics::summary(&db).unwrap();
        assert!(summary.errors_by_category.iter().all(|row| row.count == 0));
    }

    #[tokio::test]
    async fn track_ipc_error_records_the_category_and_preserves_the_error() {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(dir.path()).unwrap());
        let original: Result<u8, AppError> = Err(AppError::new(Code::Auth, "key rejected"));

        let returned = track_ipc_error(&Ok(db.clone()), original).await;

        assert_eq!(returned.unwrap_err().category, Category::Auth);
        let summary = crate::diagnostics::summary(&db).unwrap();
        assert_eq!(
            summary
                .errors_by_category
                .iter()
                .find(|row| row.category == Category::Auth)
                .unwrap()
                .count,
            1
        );
    }

    // `decide_transcribe_start` — spec 2.8 Always gate order "Consent → đuôi
    // file thuộc allow-list → hash + tra `source_hash` trong DB → tra
    // reservation JobRegistry → probe → có key dùng được → tạo Job" (I/O
    // Matrix "Trùng Phiên"/"Trùng Job"/"Sai định dạng"/"Không audio"/"Thiếu
    // key"). Each closure below increments a counter so a test can assert a
    // later gate's step was never reached once an earlier gate decided the
    // outcome — no `AppState`/DB/gateway needed.

    fn counting_extension(
        count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        ok: bool,
    ) -> impl FnOnce() -> Result<(), AppError> {
        move || {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if ok {
                Ok(())
            } else {
                Err(AppError::new(Code::Format, "unsupported extension"))
            }
        }
    }

    fn counting_hash(
        count: std::sync::Arc<std::sync::atomic::AtomicU32>,
    ) -> impl FnOnce() -> std::future::Ready<Result<String, AppError>> {
        move || {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(Ok("hash-123".to_string()))
        }
    }

    fn counting_lookup(
        count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        existing: Option<SessionId>,
    ) -> impl FnOnce(String) -> std::future::Ready<Result<Option<SessionId>, AppError>> {
        move |_hash: String| {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(Ok(existing))
        }
    }

    type ReservationFuture = std::future::Ready<Result<Option<(JobId, SessionId)>, AppError>>;

    fn counting_reservation(
        count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        existing: Option<(JobId, SessionId)>,
    ) -> impl FnOnce(String) -> ReservationFuture {
        move |_hash: String| {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(Ok(existing))
        }
    }

    fn counting_probe(
        count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        ok: bool,
    ) -> impl FnOnce() -> std::future::Ready<Result<(), AppError>> {
        move || {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(if ok {
                Ok(())
            } else {
                Err(AppError::new(Code::NoAudio, "no playable audio"))
            })
        }
    }

    fn counting_key_check(
        count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        has_key: bool,
    ) -> impl FnOnce() -> std::future::Ready<Result<bool, AppError>> {
        move || {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(Ok(has_key))
        }
    }

    fn counting_start(
        count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        job_id: JobId,
        session_id: SessionId,
    ) -> impl FnOnce(String) -> std::future::Ready<Result<registry::StartOutcome, AppError>> {
        move |_hash: String| {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(Ok(registry::StartOutcome::Started { job_id, session_id }))
        }
    }

    #[tokio::test]
    async fn decide_transcribe_start_missing_consent_never_checks_extension_or_starts_a_job() {
        let ext_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let hash_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let lookup_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let reservation_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let probe_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let key_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let start_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

        let result = decide_transcribe_start(
            false,
            counting_extension(ext_calls.clone(), true),
            counting_hash(hash_calls.clone()),
            counting_lookup(lookup_calls.clone(), None),
            counting_reservation(reservation_calls.clone(), None),
            counting_probe(probe_calls.clone(), true),
            counting_key_check(key_calls.clone(), true),
            counting_start(start_calls.clone(), JobId::new(), SessionId::new()),
        )
        .await;

        let err = result.expect_err("thiếu consent phải trả lỗi");
        assert_eq!(err.category, Category::Auth);
        assert_eq!(ext_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(hash_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(lookup_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(
            reservation_calls.load(std::sync::atomic::Ordering::SeqCst),
            0
        );
        assert_eq!(probe_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(key_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(start_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn decide_transcribe_start_wrong_extension_never_hashes_or_starts_a_job() {
        let ext_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let hash_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let probe_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let start_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

        let result = decide_transcribe_start(
            true,
            counting_extension(ext_calls.clone(), false),
            counting_hash(hash_calls.clone()),
            counting_lookup(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                None,
            ),
            counting_reservation(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                None,
            ),
            counting_probe(probe_calls.clone(), true),
            counting_key_check(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                true,
            ),
            counting_start(start_calls.clone(), JobId::new(), SessionId::new()),
        )
        .await;

        let err = result.expect_err("sai đuôi file phải trả lỗi");
        assert_eq!(err.category, Category::Format);
        assert_eq!(
            hash_calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "sai đuôi không được hash"
        );
        assert_eq!(probe_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(start_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn decide_transcribe_start_duplicate_session_hash_returns_existing_without_reservation_probe_or_key(
    ) {
        let lookup_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let reservation_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let probe_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let key_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let start_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let existing_session = SessionId::new();

        let result = decide_transcribe_start(
            true,
            counting_extension(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                true,
            ),
            counting_hash(std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0))),
            counting_lookup(lookup_calls.clone(), Some(existing_session)),
            counting_reservation(reservation_calls.clone(), None),
            counting_probe(probe_calls.clone(), true),
            counting_key_check(key_calls.clone(), true),
            counting_start(start_calls.clone(), JobId::new(), SessionId::new()),
        )
        .await
        .unwrap();

        assert_eq!(
            result,
            TranscribeStartOutcome::Existing {
                session_id: existing_session
            }
        );
        assert_eq!(lookup_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            reservation_calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "trùng Phiên không được tra reservation Job"
        );
        assert_eq!(
            probe_calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "trùng Phiên không được probe"
        );
        assert_eq!(
            key_calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "trùng Phiên không được kiểm key"
        );
        assert_eq!(
            start_calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "trùng Phiên không được tạo Job"
        );
    }

    #[tokio::test]
    async fn decide_transcribe_start_duplicate_job_reservation_returns_existing_job_without_probe_or_key(
    ) {
        let reservation_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let probe_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let key_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let start_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let existing_job = JobId::new();
        let existing_session = SessionId::new();

        let result = decide_transcribe_start(
            true,
            counting_extension(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                true,
            ),
            counting_hash(std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0))),
            counting_lookup(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                None,
            ),
            counting_reservation(
                reservation_calls.clone(),
                Some((existing_job, existing_session)),
            ),
            counting_probe(probe_calls.clone(), true),
            counting_key_check(key_calls.clone(), true),
            counting_start(start_calls.clone(), JobId::new(), SessionId::new()),
        )
        .await
        .unwrap();

        assert_eq!(
            result,
            TranscribeStartOutcome::ExistingJob {
                job_id: existing_job,
                session_id: existing_session,
            }
        );
        assert_eq!(
            reservation_calls.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        assert_eq!(
            probe_calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "trùng Job không được probe"
        );
        assert_eq!(
            key_calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "trùng Job không được kiểm key"
        );
        assert_eq!(
            start_calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "trùng Job không được tạo Job thứ hai"
        );
    }

    #[tokio::test]
    async fn decide_transcribe_start_no_playable_audio_never_checks_key_or_starts_a_job() {
        let probe_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let key_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let start_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

        let result = decide_transcribe_start(
            true,
            counting_extension(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                true,
            ),
            counting_hash(std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0))),
            counting_lookup(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                None,
            ),
            counting_reservation(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                None,
            ),
            counting_probe(probe_calls.clone(), false),
            counting_key_check(key_calls.clone(), true),
            counting_start(start_calls.clone(), JobId::new(), SessionId::new()),
        )
        .await;

        let err = result.expect_err("không audio phải trả lỗi");
        assert_eq!(err.category, Category::Format);
        assert_eq!(err.code, Code::NoAudio);
        assert_eq!(
            key_calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "không audio không được kiểm key"
        );
        assert_eq!(
            start_calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "không audio không được tạo Job"
        );
    }

    #[tokio::test]
    async fn decide_transcribe_start_no_usable_key_never_starts_a_job() {
        let key_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let start_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

        let result = decide_transcribe_start(
            true,
            counting_extension(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                true,
            ),
            counting_hash(std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0))),
            counting_lookup(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                None,
            ),
            counting_reservation(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                None,
            ),
            counting_probe(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                true,
            ),
            counting_key_check(key_calls.clone(), false),
            counting_start(start_calls.clone(), JobId::new(), SessionId::new()),
        )
        .await;

        let err = result.expect_err("thiếu key phải trả lỗi");
        assert_eq!(err.category, Category::Auth);
        assert_eq!(key_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            start_calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "thiếu key không được tạo Job"
        );
    }

    #[tokio::test]
    async fn decide_transcribe_start_every_gate_passing_creates_the_job() {
        let ext_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let hash_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let lookup_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let reservation_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let probe_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let key_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let start_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let job_id = JobId::new();
        let session_id = SessionId::new();

        let result = decide_transcribe_start(
            true,
            counting_extension(ext_calls.clone(), true),
            counting_hash(hash_calls.clone()),
            counting_lookup(lookup_calls.clone(), None),
            counting_reservation(reservation_calls.clone(), None),
            counting_probe(probe_calls.clone(), true),
            counting_key_check(key_calls.clone(), true),
            counting_start(start_calls.clone(), job_id, session_id),
        )
        .await
        .unwrap();

        assert_eq!(result, TranscribeStartOutcome::Job { job_id, session_id });
        assert_eq!(ext_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(hash_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(lookup_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            reservation_calls.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        assert_eq!(probe_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(key_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(start_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    /// Race giữa `lookup_existing_job` (snapshot) và `start_job`: actor tự
    /// trả `Existing` nguyên tử — `decide_transcribe_start` phải ánh xạ đó
    /// thành `ExistingJob`, không panic hay trả `Job` sai (spec Always: "Tạo
    /// Job trong actor là nguyên tử").
    #[tokio::test]
    async fn decide_transcribe_start_atomic_existing_from_start_job_maps_to_existing_job() {
        let job_id = JobId::new();
        let session_id = SessionId::new();

        let result = decide_transcribe_start(
            true,
            counting_extension(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                true,
            ),
            counting_hash(std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0))),
            counting_lookup(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                None,
            ),
            counting_reservation(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                None,
            ),
            counting_probe(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                true,
            ),
            counting_key_check(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                true,
            ),
            move |_hash: String| {
                std::future::ready(Ok(registry::StartOutcome::Existing { job_id, session_id }))
            },
        )
        .await
        .unwrap();

        assert_eq!(
            result,
            TranscribeStartOutcome::ExistingJob { job_id, session_id }
        );
    }

    // `decide_transcribe_rerun` — spec Tasks gate order "Consent → tra
    // Phiên/transcript thuộc Phiên → từ chối `primary` của Phiên live →
    // giải scope thành vùng → `NothingToRerun` nếu rỗng → Proxy tồn tại →
    // có key → Job hiện có hoặc start" (I/O Matrix "Chạy lại ...", "Không có
    // gì để chạy", "Thiếu Proxy", "Phiên live").

    fn rerun_session_row(kind: &str, proxy_ext: Option<&str>) -> repo::sessions::SessionRow {
        repo::sessions::SessionRow {
            id: SessionId::new(),
            kind: kind.to_string(),
            title: "cuộc họp".to_string(),
            source_hash: None,
            source_name: None,
            status: "complete".to_string(),
            recovered: false,
            duration_sec: 90.0,
            proxy_ext: proxy_ext.map(str::to_string),
            created_at: 0,
            updated_at: 0,
        }
    }

    fn rerun_transcript_row(
        session_id: SessionId,
        variant: repo::transcripts::Variant,
    ) -> repo::transcripts::TranscriptRow {
        repo::transcripts::TranscriptRow {
            id: TranscriptId::new(),
            session_id,
            variant,
            status: repo::transcripts::Status::Partial,
            model: "m".to_string(),
            language: None,
        }
    }

    fn counting_load(
        count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        session: repo::sessions::SessionRow,
        transcript: repo::transcripts::TranscriptRow,
    ) -> impl FnOnce() -> std::future::Ready<
        Result<(repo::sessions::SessionRow, repo::transcripts::TranscriptRow), AppError>,
    > {
        move || {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(Ok((session, transcript)))
        }
    }

    fn counting_ranges(
        count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        ranges: Vec<rerun::RerunRange>,
    ) -> impl FnOnce(
        &repo::sessions::SessionRow,
        &repo::transcripts::TranscriptRow,
    ) -> std::future::Ready<Result<Vec<rerun::RerunRange>, AppError>> {
        move |_session, _transcript| {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(Ok(ranges))
        }
    }

    fn counting_proxy(
        count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        result: Result<std::path::PathBuf, AppError>,
    ) -> impl FnOnce(
        &repo::sessions::SessionRow,
    ) -> std::future::Ready<Result<std::path::PathBuf, AppError>> {
        move |_session| {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(result)
        }
    }

    fn counting_rerun_key_check(
        count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        has_key: bool,
    ) -> impl FnOnce() -> std::future::Ready<Result<bool, AppError>> {
        move || {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(Ok(has_key))
        }
    }

    fn counting_rerun_start(
        count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        outcome: RerunOutcome,
    ) -> impl FnOnce(
        Vec<rerun::RerunRange>,
        std::path::PathBuf,
    ) -> std::future::Ready<Result<RerunOutcome, AppError>> {
        move |_ranges, _proxy_path| {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(Ok(outcome))
        }
    }

    fn zero_range() -> Vec<rerun::RerunRange> {
        Vec::new()
    }

    fn one_range() -> Vec<rerun::RerunRange> {
        vec![rerun::RerunRange {
            start_sample: 0,
            start_ms: 0,
            end_sample: 16_000,
            end_ms: 1_000,
        }]
    }

    #[tokio::test]
    async fn decide_transcribe_rerun_missing_consent_never_loads_or_starts() {
        let load_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let ranges_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let proxy_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let key_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let start_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let session_id = SessionId::new();

        let result = decide_transcribe_rerun(
            false,
            counting_load(
                load_calls.clone(),
                rerun_session_row("file", Some("flac")),
                rerun_transcript_row(session_id, repo::transcripts::Variant::Primary),
            ),
            counting_ranges(ranges_calls.clone(), one_range()),
            counting_proxy(
                proxy_calls.clone(),
                Ok(std::path::PathBuf::from("/tmp/proxy.flac")),
            ),
            counting_rerun_key_check(key_calls.clone(), true),
            counting_rerun_start(
                start_calls.clone(),
                RerunOutcome::Started {
                    job_id: JobId::new(),
                },
            ),
        )
        .await;

        assert_eq!(
            result.expect_err("thiếu consent phải trả lỗi").category,
            Category::Auth
        );
        assert_eq!(load_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(ranges_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(proxy_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(key_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(start_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn decide_transcribe_rerun_rejects_primary_of_a_live_session_before_resolving_scope() {
        let load_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let ranges_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let proxy_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let key_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let start_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let session_id = SessionId::new();

        let result = decide_transcribe_rerun(
            true,
            counting_load(
                load_calls.clone(),
                rerun_session_row("live", Some("flac")),
                rerun_transcript_row(session_id, repo::transcripts::Variant::Primary),
            ),
            counting_ranges(ranges_calls.clone(), one_range()),
            counting_proxy(
                proxy_calls.clone(),
                Ok(std::path::PathBuf::from("/tmp/proxy.flac")),
            ),
            counting_rerun_key_check(key_calls.clone(), true),
            counting_rerun_start(
                start_calls.clone(),
                RerunOutcome::Started {
                    job_id: JobId::new(),
                },
            ),
        )
        .await;

        assert_eq!(
            result
                .expect_err("primary của Phiên live phải bị chặn")
                .category,
            Category::Model
        );
        assert_eq!(load_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            ranges_calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "không được giải scope khi đã bị chặn ở gate live/primary"
        );
        assert_eq!(start_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn decide_transcribe_rerun_empty_ranges_is_nothing_to_rerun_without_proxy_key_or_start() {
        let load_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let ranges_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let proxy_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let key_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let start_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let session_id = SessionId::new();

        let result = decide_transcribe_rerun(
            true,
            counting_load(
                load_calls.clone(),
                rerun_session_row("file", Some("flac")),
                rerun_transcript_row(session_id, repo::transcripts::Variant::Primary),
            ),
            counting_ranges(ranges_calls.clone(), zero_range()),
            counting_proxy(
                proxy_calls.clone(),
                Ok(std::path::PathBuf::from("/tmp/proxy.flac")),
            ),
            counting_rerun_key_check(key_calls.clone(), true),
            counting_rerun_start(
                start_calls.clone(),
                RerunOutcome::Started {
                    job_id: JobId::new(),
                },
            ),
        )
        .await
        .unwrap();

        assert_eq!(result, TranscribeRerunOutcome::NothingToRerun);
        assert_eq!(proxy_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(key_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(start_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn decide_transcribe_rerun_missing_proxy_never_checks_key_or_starts() {
        let ranges_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let key_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let start_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let session_id = SessionId::new();

        let result = decide_transcribe_rerun(
            true,
            counting_load(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                rerun_session_row("file", None),
                rerun_transcript_row(session_id, repo::transcripts::Variant::Primary),
            ),
            counting_ranges(ranges_calls.clone(), one_range()),
            counting_proxy(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                Err(AppError::new(
                    Code::Storage,
                    "Phiên chưa có Proxy đã publish",
                )),
            ),
            counting_rerun_key_check(key_calls.clone(), true),
            counting_rerun_start(
                start_calls.clone(),
                RerunOutcome::Started {
                    job_id: JobId::new(),
                },
            ),
        )
        .await;

        assert_eq!(
            result.expect_err("thiếu Proxy phải trả lỗi").category,
            Category::Storage
        );
        assert_eq!(key_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(start_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn decide_transcribe_rerun_no_usable_key_never_starts() {
        let key_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let start_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let session_id = SessionId::new();

        let result = decide_transcribe_rerun(
            true,
            counting_load(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                rerun_session_row("file", Some("flac")),
                rerun_transcript_row(session_id, repo::transcripts::Variant::Primary),
            ),
            counting_ranges(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                one_range(),
            ),
            counting_proxy(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                Ok(std::path::PathBuf::from("/tmp/proxy.flac")),
            ),
            counting_rerun_key_check(key_calls.clone(), false),
            counting_rerun_start(
                start_calls.clone(),
                RerunOutcome::Started {
                    job_id: JobId::new(),
                },
            ),
        )
        .await;

        assert_eq!(
            result.expect_err("thiếu key phải trả lỗi").category,
            Category::Auth
        );
        assert_eq!(start_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn decide_transcribe_rerun_every_gate_passing_reports_the_registry_outcome() {
        let start_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let session_id = SessionId::new();
        let job_id = JobId::new();

        let result = decide_transcribe_rerun(
            true,
            counting_load(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                rerun_session_row("file", Some("flac")),
                rerun_transcript_row(session_id, repo::transcripts::Variant::Primary),
            ),
            counting_ranges(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                one_range(),
            ),
            counting_proxy(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                Ok(std::path::PathBuf::from("/tmp/proxy.flac")),
            ),
            counting_rerun_key_check(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                true,
            ),
            counting_rerun_start(start_calls.clone(), RerunOutcome::Existing { job_id }),
        )
        .await
        .unwrap();

        assert_eq!(result, TranscribeRerunOutcome::Existing { job_id });
        assert_eq!(start_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn decide_transcribe_rerun_maps_registry_busy_to_the_ipc_busy_outcome() {
        // Spec I/O Matrix "Different rerun": registry `RerunOutcome::Busy`
        // must surface as `TranscribeRerunOutcome::Busy`, still without an
        // error (the frontend treats it as a notice, not a failure).
        let start_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let session_id = SessionId::new();
        let job_id = JobId::new();

        let result = decide_transcribe_rerun(
            true,
            counting_load(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                rerun_session_row("file", Some("flac")),
                rerun_transcript_row(session_id, repo::transcripts::Variant::Primary),
            ),
            counting_ranges(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                one_range(),
            ),
            counting_proxy(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                Ok(std::path::PathBuf::from("/tmp/proxy.flac")),
            ),
            counting_rerun_key_check(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                true,
            ),
            counting_rerun_start(start_calls.clone(), RerunOutcome::Busy { job_id }),
        )
        .await
        .unwrap();

        assert_eq!(result, TranscribeRerunOutcome::Busy { job_id });
        assert_eq!(start_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    // `decide_proxy_relink` (story 2.7) — spec Tasks gate order "tra Phiên →
    // chặn Phiên live trước khi mở dialog → huỷ dialog → so hash/publish"
    // (I/O Matrix "Chọn lại khớp/sai", "Phiên live thiếu Proxy").

    fn relink_session_row(kind: &str) -> repo::sessions::SessionRow {
        repo::sessions::SessionRow {
            id: SessionId::new(),
            kind: kind.to_string(),
            title: "cuộc họp".to_string(),
            source_hash: Some("hash".to_string()),
            source_name: None,
            status: "complete".to_string(),
            recovered: false,
            duration_sec: 90.0,
            proxy_ext: Some("flac".to_string()),
            created_at: 0,
            updated_at: 0,
        }
    }

    fn counting_load_session(
        count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        session: repo::sessions::SessionRow,
    ) -> impl FnOnce() -> std::future::Ready<Result<repo::sessions::SessionRow, AppError>> {
        move || {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(Ok(session))
        }
    }

    fn counting_pick(
        count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        picked: Option<std::path::PathBuf>,
    ) -> impl FnOnce() -> std::future::Ready<Result<Option<std::path::PathBuf>, AppError>> {
        move || {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(Ok(picked))
        }
    }

    fn counting_relink(
        count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        outcome: library::store::RelinkOutcome,
    ) -> impl FnOnce(
        std::path::PathBuf,
    ) -> std::future::Ready<Result<library::store::RelinkOutcome, AppError>> {
        move |_picked| {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(Ok(outcome))
        }
    }

    #[tokio::test]
    async fn decide_proxy_relink_live_session_never_opens_dialog_or_relinks() {
        let load_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let pick_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let relink_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

        let result = decide_proxy_relink(
            counting_load_session(load_calls.clone(), relink_session_row("live")),
            counting_pick(
                pick_calls.clone(),
                Some(std::path::PathBuf::from("/tmp/picked.wav")),
            ),
            counting_relink(
                relink_calls.clone(),
                library::store::RelinkOutcome::Relinked,
            ),
        )
        .await
        .unwrap();

        assert_eq!(result, ProxyRelinkOutcome::LiveUnsupported);
        assert_eq!(load_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            pick_calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "Phiên live không được mở dialog"
        );
        assert_eq!(relink_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn decide_proxy_relink_cancelled_dialog_never_relinks() {
        let relink_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

        let result = decide_proxy_relink(
            counting_load_session(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                relink_session_row("file"),
            ),
            counting_pick(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                None,
            ),
            counting_relink(
                relink_calls.clone(),
                library::store::RelinkOutcome::Relinked,
            ),
        )
        .await
        .unwrap();

        assert_eq!(result, ProxyRelinkOutcome::Cancelled);
        assert_eq!(relink_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn decide_proxy_relink_maps_every_store_outcome() {
        for (store_outcome, expected) in [
            (
                library::store::RelinkOutcome::Relinked,
                ProxyRelinkOutcome::Relinked,
            ),
            (
                library::store::RelinkOutcome::HashMismatch,
                ProxyRelinkOutcome::HashMismatch,
            ),
            (
                library::store::RelinkOutcome::NotFileSession,
                ProxyRelinkOutcome::LiveUnsupported,
            ),
        ] {
            let result = decide_proxy_relink(
                counting_load_session(
                    std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                    relink_session_row("file"),
                ),
                counting_pick(
                    std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                    Some(std::path::PathBuf::from("/tmp/picked.wav")),
                ),
                counting_relink(
                    std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                    store_outcome,
                ),
            )
            .await
            .unwrap();
            assert_eq!(result, expected);
        }
    }

    // `decide_session_delete` (story 3.1) — spec Design Notes gate order
    // "Đánh dấu trước khi hỏi `is_busy`" and "gỡ ở mọi nhánh thoát, kể cả
    // lỗi".

    #[test]
    fn recording_export_slot_is_exclusive_released_by_its_owner_and_visible_to_delete_and_wipe() {
        let slot = std::sync::Mutex::new(None);
        let session_id = SessionId::new();
        let other_session = SessionId::new();
        let first = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let second = Arc::new(std::sync::atomic::AtomicBool::new(false));

        assert!(!recording_export_active_for(&slot, Some(session_id)));
        assert!(!recording_export_active_for(&slot, None));
        reserve_recording_export(&slot, session_id, first.clone()).unwrap();
        // A second export is refused while the slot is held.
        let error = reserve_recording_export(&slot, other_session, second.clone()).unwrap_err();
        assert_eq!(error.code, Code::Request);
        // Delete of the exported session and wipe are blocked; other sessions are not.
        assert!(recording_export_active_for(&slot, Some(session_id)));
        assert!(!recording_export_active_for(&slot, Some(other_session)));
        assert!(recording_export_active_for(&slot, None));

        // Only the owner's token releases the slot.
        release_recording_export(&slot, &second);
        assert!(recording_export_active_for(&slot, Some(session_id)));
        release_recording_export(&slot, &first);
        assert!(!recording_export_active_for(&slot, None));
        reserve_recording_export(&slot, other_session, second).unwrap();
    }

    #[test]
    fn recovering_session_rejects_delete_and_rerun_until_recovery_releases_it() {
        let session_id = SessionId::new();
        let recovering = std::sync::Mutex::new(std::collections::HashSet::new());
        recovering.lock().unwrap().insert(session_id);

        assert_eq!(
            recovering_delete_outcome(&recovering, session_id),
            Some(SessionDeleteOutcome::Busy)
        );
        assert!(is_session_recovering(&recovering, session_id));
        assert_eq!(session_recovering_error().code, Code::Request);

        recovering.lock().unwrap().remove(&session_id);
        assert_eq!(recovering_delete_outcome(&recovering, session_id), None);
        assert!(!is_session_recovering(&recovering, session_id));
    }

    fn counting_busy(
        count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        busy: bool,
    ) -> impl FnOnce() -> std::future::Ready<Result<bool, AppError>> {
        move || {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(Ok(busy))
        }
    }

    fn erroring_busy(
        count: std::sync::Arc<std::sync::atomic::AtomicU32>,
    ) -> impl FnOnce() -> std::future::Ready<Result<bool, AppError>> {
        move || {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(Err(AppError::new(
                Code::Storage,
                "injected: is_busy failure",
            )))
        }
    }

    fn counting_delete(
        count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        result: Result<(), AppError>,
    ) -> impl FnOnce() -> std::future::Ready<Result<(), AppError>> {
        move || {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(result)
        }
    }

    fn flagging(flag: std::sync::Arc<std::sync::atomic::AtomicBool>) -> impl FnOnce() {
        move || {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn running_live_session_makes_delete_and_wipe_busy_without_asking_jobs() {
        let asked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = asked.clone();
        let busy = busy_when_live_running(async { Ok(true) }, async move {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(false)
        })
        .await
        .unwrap();
        assert!(busy);
        assert!(!asked.load(std::sync::atomic::Ordering::SeqCst));

        assert!(
            !busy_when_live_running(async { Ok(false) }, async { Ok(false) })
                .await
                .unwrap()
        );
        assert!(
            busy_when_live_running(async { Ok(false) }, async { Ok(true) })
                .await
                .unwrap()
        );
        assert!(
            busy_when_live_running(async { Err(AppError::new(Code::Storage, "x")) }, async {
                Ok(false)
            })
            .await
            .is_err()
        );
    }

    #[test]
    fn export_extension_appends_instead_of_replacing_dotted_segments() {
        use std::path::PathBuf;
        assert_eq!(
            with_export_extension(PathBuf::from("/x/a.v2"), "wav"),
            PathBuf::from("/x/a.v2.wav")
        );
        assert_eq!(
            with_export_extension(PathBuf::from("/x/a"), "wav"),
            PathBuf::from("/x/a.wav")
        );
        assert_eq!(
            with_export_extension(PathBuf::from("/x/a.WAV"), "wav"),
            PathBuf::from("/x/a.WAV")
        );
    }

    #[tokio::test]
    async fn decide_session_delete_marks_before_checking_busy_and_unmarks_on_busy() {
        let marked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let unmarked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let delete_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

        let result = decide_session_delete(
            flagging(marked.clone()),
            counting_busy(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                true,
            ),
            counting_delete(delete_calls.clone(), Ok(())),
            flagging(unmarked.clone()),
        )
        .await
        .unwrap();

        assert_eq!(result, SessionDeleteOutcome::Busy);
        assert!(marked.load(std::sync::atomic::Ordering::SeqCst));
        assert!(unmarked.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(
            delete_calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "bận -> không được xoá gì"
        );
    }

    #[tokio::test]
    async fn decide_session_delete_deletes_and_unmarks_when_not_busy() {
        let marked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let unmarked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let busy_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let delete_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

        let result = decide_session_delete(
            flagging(marked.clone()),
            counting_busy(busy_calls.clone(), false),
            counting_delete(delete_calls.clone(), Ok(())),
            flagging(unmarked.clone()),
        )
        .await
        .unwrap();

        assert_eq!(result, SessionDeleteOutcome::Deleted);
        assert!(marked.load(std::sync::atomic::Ordering::SeqCst));
        assert!(unmarked.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(busy_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(delete_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn decide_session_delete_unmarks_when_is_busy_itself_errors() {
        let unmarked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let delete_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

        let err = decide_session_delete(
            flagging(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                false,
            ))),
            erroring_busy(std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0))),
            counting_delete(delete_calls.clone(), Ok(())),
            flagging(unmarked.clone()),
        )
        .await
        .unwrap_err();

        assert_eq!(err.category, crate::core::error::Category::Storage);
        assert!(unmarked.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(delete_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn decide_session_delete_unmarks_when_delete_itself_errors() {
        let unmarked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let err = decide_session_delete(
            flagging(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                false,
            ))),
            counting_busy(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                false,
            ),
            counting_delete(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                Err(AppError::new(Code::Storage, "injected: delete failure")),
            ),
            flagging(unmarked.clone()),
        )
        .await
        .unwrap_err();

        assert_eq!(err.category, crate::core::error::Category::Storage);
        assert!(unmarked.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn is_session_deleting_reflects_the_deleting_set() {
        let session_id = SessionId::new();
        let deleting = std::sync::Mutex::new(std::collections::HashSet::new());
        deleting.lock().unwrap().insert(session_id);

        assert!(is_session_deleting(&deleting, session_id));
        assert!(!is_session_deleting(&deleting, SessionId::new()));
    }

    #[test]
    fn is_wiping_reflects_the_flag() {
        let wiping = std::sync::atomic::AtomicBool::new(false);
        assert!(!is_wiping(&wiping));
        wiping.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(is_wiping(&wiping));
    }

    // `decide_wipe_all` (story 3.4) — cùng gate order/unmark-on-every-exit
    // với `decide_session_delete` ở trên, nhưng không theo session_id.

    #[tokio::test]
    async fn decide_wipe_all_marks_before_checking_busy_and_unmarks_on_busy() {
        let marked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let unmarked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let wipe_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

        let result = decide_wipe_all(
            flagging(marked.clone()),
            counting_busy(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                true,
            ),
            counting_delete(wipe_calls.clone(), Ok(())),
            flagging(unmarked.clone()),
        )
        .await
        .unwrap();

        assert_eq!(result, WipeAllOutcome::Busy);
        assert!(marked.load(std::sync::atomic::Ordering::SeqCst));
        assert!(unmarked.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(
            wipe_calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "bận -> không được xoá gì"
        );
    }

    #[tokio::test]
    async fn decide_wipe_all_wipes_and_unmarks_when_not_busy() {
        let marked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let unmarked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let busy_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let wipe_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

        let result = decide_wipe_all(
            flagging(marked.clone()),
            counting_busy(busy_calls.clone(), false),
            counting_delete(wipe_calls.clone(), Ok(())),
            flagging(unmarked.clone()),
        )
        .await
        .unwrap();

        assert_eq!(result, WipeAllOutcome::Wiped);
        assert!(marked.load(std::sync::atomic::Ordering::SeqCst));
        assert!(unmarked.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(busy_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(wipe_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn decide_wipe_all_unmarks_when_the_busy_check_itself_errors() {
        let unmarked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let wipe_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

        let err = decide_wipe_all(
            flagging(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                false,
            ))),
            erroring_busy(std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0))),
            counting_delete(wipe_calls.clone(), Ok(())),
            flagging(unmarked.clone()),
        )
        .await
        .unwrap_err();

        assert_eq!(err.category, crate::core::error::Category::Storage);
        assert!(unmarked.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(wipe_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn decide_wipe_all_unmarks_when_wipe_itself_errors() {
        let unmarked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let err = decide_wipe_all(
            flagging(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                false,
            ))),
            counting_busy(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                false,
            ),
            counting_delete(
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                Err(AppError::new(Code::Storage, "injected: wipe failure")),
            ),
            flagging(unmarked.clone()),
        )
        .await
        .unwrap_err();

        assert_eq!(err.category, crate::core::error::Category::Storage);
        assert!(unmarked.load(std::sync::atomic::Ordering::SeqCst));
    }
}
