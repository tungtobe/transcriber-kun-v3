//! Chủ boot duy nhất (AD-18): resolve thư mục app, khởi tạo log, mở DB, gói
//! thành [`AppState`] để `lib.rs::run()` `manage()`. Không nơi nào khác được
//! gọi `Db::open`/`core::log::init_file_logging` trực tiếp trong production.

use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use tauri::Manager;

use crate::core::error::{AppError, Code};
use crate::core::id::SessionId;
use crate::db::Db;
use crate::gemini::keys::{KeyPoolHandle, KeyProvider, SystemClock};
use crate::gemini::GeminiGateway;
use crate::secrets::{NativeCredentialStore, SecretService};
use crate::transcribe::registry::{self, GatewayTranscriber, JobRegistryHandle};

/// State managed toàn app. `db` là `Err` khi thư mục dữ liệu không mở được —
/// app vẫn khởi động bình thường, mọi command chạm DB trả lại đúng lỗi này
/// (spec I/O Matrix: "App vẫn khởi động; ... trả AppError category storage").
pub struct AppState {
    pub db: Result<Arc<Db>, AppError>,
    /// Cùng gốc `app_data_dir` dùng bởi `db`/`registry::channel` (Proxy nằm
    /// dưới `<data_dir>/media/<session-id>/proxy.<ext>` -- spec 2.5: Chạy lại
    /// cần tra Proxy này trước khi tạo Job, không qua registry actor cho một
    /// việc thuần đọc đường dẫn).
    pub data_dir: Result<std::path::PathBuf, AppError>,
    pub secrets: Arc<SecretService<NativeCredentialStore>>,
    pub key_pool: KeyPoolHandle,
    /// One process-wide HTTP client/gateway. A client-construction failure is
    /// retained as a typed error so boot still reaches the UI.
    pub gateway: Result<Arc<GeminiGateway>, AppError>,
    /// Story 2.4: the one in-memory sequential Job queue for file
    /// transcription. `Err` only when `db`/`gateway` themselves failed to
    /// initialize — `transcribe_start` surfaces that same storage error
    /// instead of ever touching a registry built on a broken foundation.
    pub jobs: Result<JobRegistryHandle, AppError>,
    /// Story 3.1: khoá "deleting" theo session (spec Design Notes: "một
    /// `HashSet<SessionId>` trong `AppState` -- chỉ `ipc/` đọc/ghi", AD-1).
    /// Đánh dấu **trước** khi hỏi `JobRegistry::is_busy` trong
    /// `decide_session_delete`, gỡ ở mọi nhánh thoát (kể cả lỗi) -- một
    /// `transcribe_rerun`/`library_proxy_relink`/`library_transcript_export`
    /// cho cùng session trong lúc này bị từ chối bằng `Code::Request` (spec
    /// I/O Matrix "Race Chạy lại/relink").
    pub deleting: Arc<Mutex<HashSet<SessionId>>>,
    /// Story 3.4: cờ toàn cục "đang xoá toàn bộ dữ liệu" (spec Always: "một
    /// cờ toàn cục trong `AppState` (chỉ `ipc/` đọc/ghi) làm mọi writer mới
    /// bị từ chối"). Đặt **trước** khi kiểm Job bận trong `decide_wipe_all`,
    /// gỡ ở mọi nhánh thoát -- cùng khuôn với `deleting` ở trên nhưng không
    /// theo từng session vì `library_wipe_all` chạm mọi Phiên cùng lúc.
    pub wiping: Arc<AtomicBool>,
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
    } else if let Ok(db) = &db {
        // Chỉ một chỗ duy nhất gọi `note_boot` mỗi lần khởi động (spec
        // Always "Crash"): DB lỗi ở đây không được phép chặn boot, chỉ log
        // cảnh báo (spec I/O Matrix "Crash": "DB lỗi → boot vẫn tiếp tục").
        if let Err(err) = crate::diagnostics::note_boot(db) {
            tracing::warn!(error = %err, "không ghi được marker crash lúc boot");
        }

        // Story 2.3 AD-18: dọn mọi thứ một crash giữa publish và commit có
        // thể để lại (`library::store::reconcile`) — non-fatal, root là
        // `app_data_dir` giống `Db::open` ở trên (cùng gốc chứa `media/`).
        match app.path().app_data_dir() {
            Ok(data_dir) => {
                if let Err(err) = crate::library::store::reconcile(db, &data_dir) {
                    tracing::warn!(error = %err, "không reconcile được thư viện lúc boot");
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, "không resolve được thư mục dữ liệu, bỏ qua reconcile lúc boot");
            }
        }
    }

    let secrets = Arc::new(SecretService::new(NativeCredentialStore));
    let provider: Arc<dyn KeyProvider> = secrets.clone();
    let (key_pool, key_pool_actor) = KeyPoolHandle::channel(provider, Arc::new(SystemClock));
    tauri::async_runtime::spawn(key_pool_actor.run());

    // Native credential-store access is deliberately lazy and fallible: a
    // locked/unavailable Keychain or Credential Manager must not stop boot.
    let initial_pool = key_pool.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(err) = initial_pool.refresh().await {
            tracing::warn!(error = %err, "kho khoá OS chưa sẵn sàng; app vẫn tiếp tục");
        }
    });

    let gateway = GeminiGateway::production(key_pool.clone()).map(Arc::new);
    if let Err(err) = &gateway {
        tracing::error!(error = %err, "không khởi tạo được Gemini gateway");
    }

    // Story 2.4: one `JobRegistry` actor for the process, rooted at the same
    // `app_data_dir` as `db`/`library::store::reconcile` above (so staging
    // lives under `media/.staging` next to published Proxy media). Both `db`
    // and `gateway` must already be usable — a registry built on a broken DB
    // or gateway could never commit or transcribe anything anyway.
    let jobs = match (&db, &gateway, app.path().app_data_dir()) {
        (Ok(db), Ok(gateway), Ok(data_dir)) => {
            let transcriber = Arc::new(GatewayTranscriber::new(gateway.clone()));
            let (handle, actor) = registry::channel(db.clone(), data_dir, transcriber);
            tauri::async_runtime::spawn(actor.run());
            Ok(handle)
        }
        (Err(err), _, _) | (_, Err(err), _) => Err(err.clone()),
        (_, _, Err(err)) => Err(AppError::new(Code::Storage, err.to_string())),
    };

    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|err| AppError::new(Code::Storage, err.to_string()));

    AppState {
        db,
        data_dir,
        secrets,
        key_pool,
        gateway,
        jobs,
        deleting: Arc::new(Mutex::new(HashSet::new())),
        wiping: Arc::new(AtomicBool::new(false)),
        _log_guard: log_guard,
    }
}
