//! Adapter UI (AD-1): command/Channel tauri-specta; boot, đóng cửa sổ; nơi
//! duy nhất điều phối chéo feature. `specta_builder()` là nguồn duy nhất
//! liệt kê command/event production, dùng chung cho `lib.rs` (đăng ký thật)
//! và test `export_bindings` (sinh `src/lib/bindings.ts`).

pub mod boot;

#[cfg(test)]
mod spike_channel;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::Manager;
use tauri_specta::{collect_commands, collect_events, Builder, Event};

use crate::consent::{self, ConsentPolicy};
use crate::core::error::{AppError, Code};
use crate::core::id::{JobId, SessionId, TranscriptId};
use crate::core::paths;
use crate::db::{repo, Db};
use crate::diagnostics::{self, DiagnosticsSummary};
use crate::gemini::{CancellationToken, ConsentSnapshot, KeyTestResult, ModelInfo, ModelKind};
use crate::library;
use crate::secrets::{KeyId, KeyMetadata};
use crate::settings::{self, Settings, SettingsChanged};
use crate::transcribe::job::{CancelOutcome, JobEvent};
use crate::transcribe::registry::{self, RerunOutcome, RerunParams};
use crate::transcribe::rerun::{self, RerunScope};
use boot::AppState;

/// Phát khi cửa sổ chính bị yêu cầu đóng trong lúc registry bận (spec Design
/// Notes: "bận → emit `CloseRequested`") — UI hỏi xác nhận, gọi
/// `app_close_confirm` nếu người dùng đồng ý huỷ sạch rồi thoát.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct CloseRequested;

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

/// Kết quả `transcribe_start` (spec Always: thứ tự gate "Consent → hash + tra
/// `source_hash` → có key dùng được → tạo Job").
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
}

/// Pure gate-order decision for `transcribe_start` (spec Always: "Consent →
/// hash + tra `source_hash` → có key dùng được → tạo Job"), factored out so
/// it can be tested without a real `AppState`/DB/gateway: every I/O step is
/// injected as a closure, so a test can assert both the *outcome* and that a
/// later step's closure was never invoked once an earlier gate rejects (spec
/// I/O Matrix "Trùng": "không Job, không Gemini"; "Chưa Consent / thiếu
/// key": "Không tạo Job").
///
/// `compute_hash`/`lookup_existing_session`/`has_usable_key`/`start_job` are
/// each called at most once, strictly in gate order, and only when every
/// prior gate passed.
async fn decide_transcribe_start<HashFut, LookupFut, KeyFut, StartFut>(
    consent_current: bool,
    compute_hash: impl FnOnce() -> HashFut,
    lookup_existing_session: impl FnOnce(String) -> LookupFut,
    has_usable_key: impl FnOnce() -> KeyFut,
    start_job: impl FnOnce(String) -> StartFut,
) -> Result<TranscribeStartOutcome, AppError>
where
    HashFut: std::future::Future<Output = Result<String, AppError>>,
    LookupFut: std::future::Future<Output = Result<Option<SessionId>, AppError>>,
    KeyFut: std::future::Future<Output = Result<bool, AppError>>,
    StartFut: std::future::Future<Output = Result<(JobId, SessionId), AppError>>,
{
    if !consent_current {
        return Err(AppError::new(
            Code::Auth,
            "Current consent is required before transcription can start",
        ));
    }

    let hash = compute_hash().await?;

    if let Some(session_id) = lookup_existing_session(hash.clone()).await? {
        return Ok(TranscribeStartOutcome::Existing { session_id });
    }

    if !has_usable_key().await? {
        return Err(AppError::new(
            Code::Auth,
            "No usable Gemini API key is configured",
        ));
    }

    let (job_id, session_id) = start_job(hash).await?;
    Ok(TranscribeStartOutcome::Job { job_id, session_id })
}

async fn transcribe_start_inner(
    state: &AppState,
    path: String,
) -> Result<TranscribeStartOutcome, AppError> {
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
    let hash_path = source_path.clone();
    let lookup_db = db.clone();
    let secrets = state.secrets.clone();
    let jobs = state.jobs.clone();
    let model = settings.transcribe_model.clone();
    let language = settings.transcribe_language;
    let chunk_minutes = settings.chunk_minutes;

    decide_transcribe_start(
        consent.is_current(),
        move || blocking(move || crate::media::sha256_file(&hash_path)),
        move |hash: String| {
            blocking(move || {
                lookup_db
                    .with_connection(|conn| Ok(repo::sessions::find_by_source_hash(conn, &hash)?))
            })
        },
        move || async move {
            let keys = blocking(move || secrets.list()).await?;
            Ok(!keys.is_empty())
        },
        move |hash: String| async move {
            let jobs = jobs?;
            jobs.start(registry::StartParams {
                source_path,
                source_hash: hash,
                source_name,
                model,
                language,
                chunk_minutes,
                consent,
            })
            .await
        },
    )
    .await
}

/// Bắt đầu transcribe một file (spec Always: thứ tự gate đầy đủ nằm ở
/// [`transcribe_start_inner`]). `path` là đường dẫn tuyệt đối do caller cung
/// cấp — story này không mở dialog/nhận kéo thả (2.8/2.9 sẽ gọi lệnh này với
/// đường dẫn đã chọn).
#[tauri::command]
#[specta::specta]
async fn transcribe_start(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<TranscribeStartOutcome, AppError> {
    let result = transcribe_start_inner(&state, path).await;
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
    Started { job_id: JobId },
    Existing { job_id: JobId },
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
    })
}

async fn transcribe_rerun_inner(
    state: &AppState,
    session_id: SessionId,
    transcript_id: TranscriptId,
    scope: RerunScope,
) -> Result<TranscribeRerunOutcome, AppError> {
    let db = state.db.clone()?;
    let data_dir = state.data_dir.clone()?;
    let settings = blocking({
        let db = db.clone();
        move || Ok(settings::load(&db))
    })
    .await?;
    let consent =
        ConsentSnapshot::new(settings.consent_accepted_version, settings.consent_declined);
    let model = settings.transcribe_model.clone();
    let language = settings.transcribe_language;
    let chunk_minutes = settings.chunk_minutes;
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
            jobs.start_rerun(RerunParams {
                session_id,
                transcript_id,
                ranges,
                discard_old: matches!(scope, RerunScope::All),
                proxy_path,
                model,
                language,
                chunk_minutes,
                consent,
            })
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

/// Người dùng xác nhận đóng app khi registry đang bận (spec Design Notes:
/// "đồng ý → huỷ sạch rồi thoát"): huỷ mọi Job hiện có, chờ tối đa ~4 s để
/// mỗi Job dọn xong (huỷ có hiệu lực ≤ 2 s — spec Always), rồi thoát tiến
/// trình thật sự bất kể kết quả chờ, vì người dùng đã đồng ý đóng.
#[tauri::command]
#[specta::specta]
async fn app_close_confirm(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), AppError> {
    if let Ok(jobs) = state.jobs.clone() {
        if let Ok(pending) = jobs.snapshot().await {
            for job in &pending {
                let _ = jobs.cancel(job.job_id).await;
            }
        }
        for _ in 0..40 {
            match jobs.snapshot().await {
                Ok(remaining) if remaining.is_empty() => break,
                _ => {}
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
    app.exit(0);
    Ok(())
}

/// Danh sách command/event production — nguồn duy nhất, dùng chung cho
/// `lib.rs` (đăng ký `invoke_handler`/`mount_events` thật) và test
/// `export_bindings` (sinh `src/lib/bindings.ts`). Không đăng ký gì từ
/// `spike_channel` ở đây.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            app_version,
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
            transcribe_rerun,
            jobs_subscribe,
            jobs_cancel,
            library_session_get,
            app_close_confirm
        ])
        .events(collect_events![SettingsChanged, CloseRequested])
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
        assert_eq!(original.unwrap_err().category, Category::Auth);
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

    // `decide_transcribe_start` — spec Always gate order "Consent → hash +
    // tra `source_hash` → có key dùng được → tạo Job" (I/O Matrix "Trùng",
    // "Chưa Consent / thiếu key"). Each closure below increments a counter
    // so a test can assert a later gate's step was never reached once an
    // earlier gate decided the outcome — no `AppState`/DB/gateway needed.

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
    ) -> impl FnOnce(String) -> std::future::Ready<Result<(JobId, SessionId), AppError>> {
        move |_hash: String| {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(Ok((job_id, session_id)))
        }
    }

    #[tokio::test]
    async fn decide_transcribe_start_missing_consent_never_computes_hash_or_starts_a_job() {
        let hash_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let lookup_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let key_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let start_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

        let result = decide_transcribe_start(
            false,
            counting_hash(hash_calls.clone()),
            counting_lookup(lookup_calls.clone(), None),
            counting_key_check(key_calls.clone(), true),
            counting_start(start_calls.clone(), JobId::new(), SessionId::new()),
        )
        .await;

        let err = result.expect_err("thiếu consent phải trả lỗi");
        assert_eq!(err.category, Category::Auth);
        assert_eq!(hash_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(lookup_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(key_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(start_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn decide_transcribe_start_duplicate_hash_returns_existing_without_key_check_or_job() {
        let hash_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let lookup_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let key_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let start_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let existing_session = SessionId::new();

        let result = decide_transcribe_start(
            true,
            counting_hash(hash_calls.clone()),
            counting_lookup(lookup_calls.clone(), Some(existing_session)),
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
        assert_eq!(hash_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(lookup_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            key_calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "trùng hash không được kiểm key"
        );
        assert_eq!(
            start_calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "trùng hash không được tạo Job"
        );
    }

    #[tokio::test]
    async fn decide_transcribe_start_no_usable_key_never_starts_a_job() {
        let hash_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let lookup_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let key_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let start_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

        let result = decide_transcribe_start(
            true,
            counting_hash(hash_calls.clone()),
            counting_lookup(lookup_calls.clone(), None),
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
        let hash_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let lookup_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let key_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let start_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let job_id = JobId::new();
        let session_id = SessionId::new();

        let result = decide_transcribe_start(
            true,
            counting_hash(hash_calls.clone()),
            counting_lookup(lookup_calls.clone(), None),
            counting_key_check(key_calls.clone(), true),
            counting_start(start_calls.clone(), job_id, session_id),
        )
        .await
        .unwrap();

        assert_eq!(result, TranscribeStartOutcome::Job { job_id, session_id });
        assert_eq!(hash_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(lookup_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(key_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(start_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
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
}
