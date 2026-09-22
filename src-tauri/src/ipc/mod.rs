//! Adapter UI (AD-1): command/Channel tauri-specta; boot, đóng cửa sổ; nơi
//! duy nhất điều phối chéo feature. `specta_builder()` là nguồn duy nhất
//! liệt kê command/event production, dùng chung cho `lib.rs` (đăng ký thật)
//! và test `export_bindings` (sinh `src/lib/bindings.ts`).

pub mod boot;

#[cfg(test)]
mod spike_channel;

use std::sync::Arc;

use tauri_specta::{collect_commands, collect_events, Builder, Event};

use crate::consent::{self, ConsentPolicy};
use crate::core::error::{AppError, Code};
use crate::db::Db;
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

/// Đọc toàn bộ settings hiện tại — xem [`get_settings`] cho logic thật.
#[tauri::command]
#[specta::specta]
async fn settings_get(state: tauri::State<'_, AppState>) -> Result<Settings, AppError> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || get_settings(&db))
        .await
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))?
}

/// Return the Rust-owned consent policy and the durable decision in one
/// typed snapshot. A policy read failure is a storage error; no fallback
/// version is invented in the frontend.
#[tauri::command]
#[specta::specta]
async fn consent_policy(state: tauri::State<'_, AppState>) -> Result<ConsentPolicy, AppError> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || get_consent_policy(&db))
        .await
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))?
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
    tauri::async_runtime::spawn_blocking(move || {
        save_settings_and_notify(&db, settings, |saved| {
            if let Err(err) = SettingsChanged(saved.clone()).emit(&app) {
                tracing::warn!(error = %err, "phát event settingsChanged thất bại");
            }
        })
    })
    .await
    .map_err(|err| AppError::new(Code::Storage, err.to_string()))?
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
    tauri::async_runtime::spawn_blocking(move || {
        let next = accept_consent_snapshot(settings);
        save_settings_and_notify(&db, next.clone(), |saved| {
            if let Err(err) = SettingsChanged(saved.clone()).emit(&app) {
                tracing::warn!(error = %err, "phát event consent accept thất bại");
            }
        })?;
        Ok(next)
    })
    .await
    .map_err(|err| AppError::new(Code::Storage, err.to_string()))?
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
    tauri::async_runtime::spawn_blocking(move || {
        let next = decline_consent_snapshot(settings);
        save_settings_and_notify(&db, next.clone(), |saved| {
            if let Err(err) = SettingsChanged(saved.clone()).emit(&app) {
                tracing::warn!(error = %err, "phát event consent decline thất bại");
            }
        })?;
        Ok(next)
    })
    .await
    .map_err(|err| AppError::new(Code::Storage, err.to_string()))?
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
            consent_decline
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
}
