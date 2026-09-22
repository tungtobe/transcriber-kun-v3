//! Chủ boot duy nhất (AD-18): resolve thư mục app, khởi tạo log, mở DB, gói
//! thành [`AppState`] để `lib.rs::run()` `manage()`. Không nơi nào khác được
//! gọi `Db::open`/`core::log::init_file_logging` trực tiếp trong production.

use std::sync::Arc;

use tauri::Manager;

use crate::core::error::{AppError, Code};
use crate::db::Db;

/// State managed toàn app. `db` là `Err` khi thư mục dữ liệu không mở được —
/// app vẫn khởi động bình thường, mọi command chạm DB trả lại đúng lỗi này
/// (spec I/O Matrix: "App vẫn khởi động; ... trả AppError category storage").
pub struct AppState {
    pub db: Result<Arc<Db>, AppError>,
    // Giữ sống suốt vòng đời app — drop sớm sẽ ngắt worker ghi log không
    // đồng bộ của `tracing-appender`. Không đọc trực tiếp ở đâu khác nên
    // đặt `_` để không bị cảnh báo "chưa dùng", nhưng vẫn public để test có
    // thể quan sát log đã khởi tạo hay chưa nếu cần.
    pub _log_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

/// Boot một lần lúc `setup` (`lib.rs`). Không panic trong bất kỳ nhánh nào —
/// mọi lỗi resolve thư mục/mở DB đều biến thành `AppError` giữ trong state.
pub fn boot<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> AppState {
    let log_guard = match app.path().app_log_dir() {
        Ok(log_dir) => {
            match crate::core::log::init_file_logging(&log_dir) {
                Ok(guard) => Some(guard),
                Err(err) => {
                    eprintln!("[boot] không khởi tạo được log file, tiếp tục không ghi log ra file: {err}");
                    None
                }
            }
        }
        Err(err) => {
            eprintln!(
                "[boot] không resolve được thư mục log, tiếp tục không ghi log ra file: {err}"
            );
            None
        }
    };

    let db = app
        .path()
        .app_data_dir()
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))
        .and_then(|data_dir| Db::open(&data_dir))
        .map(Arc::new);

    if let Err(err) = &db {
        tracing::error!(error = %err, "DB không mở được, app vẫn khởi động");
    }

    AppState {
        db,
        _log_guard: log_guard,
    }
}
