//! Adapter UI (AD-1): command/Channel tauri-specta; boot, đóng cửa sổ; nơi
//! duy nhất điều phối chéo feature. `specta_builder()` là nguồn duy nhất
//! liệt kê command/event production, dùng chung cho `lib.rs` (đăng ký thật)
//! và test `export_bindings` (sinh `src/lib/bindings.ts`).

pub mod boot;

#[cfg(test)]
mod spike_channel;

use std::sync::Arc;

use tauri::Manager;
use tauri_specta::{collect_commands, collect_events, Builder, Event};

use crate::consent::{self, ConsentPolicy};
use crate::core::error::{AppError, Code};
use crate::db::Db;
use crate::diagnostics::{self, DiagnosticsSummary};
use crate::gemini::{CancellationToken, ConsentSnapshot, KeyTestResult, ModelInfo, ModelKind};
use crate::secrets::{KeyId, KeyMetadata};
use crate::settings::{self, Settings, SettingsChanged};
use boot::AppState;

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
            diagnostics_export
        ])
        .events(collect_events![SettingsChanged])
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
}
