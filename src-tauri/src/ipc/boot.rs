//! Chủ boot duy nhất (AD-18): resolve thư mục app, khởi tạo log, mở DB, gói
//! thành [`AppState`] để `lib.rs::run()` `manage()`. Không nơi nào khác được
//! gọi `Db::open`/`core::log::init_file_logging` trực tiếp trong production.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::Manager;
use tauri_specta::Event;

use crate::core::error::{AppError, Code};
use crate::core::id::{MemoTemplateId, SessionId};
use crate::db::Db;
use crate::gemini::keys::{KeyPoolHandle, KeyProvider, SystemClock};
use crate::gemini::{CancellationToken, GeminiGateway};
use crate::secrets::{NativeCredentialStore, SecretService};
use crate::transcribe::registry::{self, GatewayTranscriber, JobRegistryHandle};

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct LiveRecoveryCompleted {
    pub session_id: SessionId,
}

/// State managed toàn app. `db` là `Err` khi thư mục dữ liệu không mở được —
/// app vẫn khởi động bình thường, mọi command chạm DB trả lại đúng lỗi này
/// (spec I/O Matrix: "App vẫn khởi động; ... trả AppError category storage").
pub struct AppState {
    /// Story 4.1: process-wide live capture controller shared by IPC, the
    /// future Live transport, and durable recording.
    pub capture: Arc<crate::audio::CaptureController>,
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
    /// One LiveSession actor for the process. It owns the live generation and
    /// seq stream; capture/Recording remain available when its socket is down.
    pub live: Result<crate::live::LiveSessionHandle, AppError>,
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
    /// Live sessions claimed by startup recovery. Delete and rerun requests
    /// treat these rows as busy until the recovery worker commits or fails.
    pub recovering: Arc<Mutex<HashSet<SessionId>>>,
    /// Story 3.4: cờ toàn cục "đang xoá toàn bộ dữ liệu" (spec Always: "một
    /// cờ toàn cục trong `AppState` (chỉ `ipc/` đọc/ghi) làm mọi writer mới
    /// bị từ chối"). Đặt **trước** khi kiểm Job bận trong `decide_wipe_all`,
    /// gỡ ở mọi nhánh thoát -- cùng khuôn với `deleting` ở trên nhưng không
    /// theo từng session vì `library_wipe_all` chạm mọi Phiên cùng lúc.
    pub wiping: Arc<AtomicBool>,
    /// Story 3.7: registry "đang sinh memo" theo cặp (Phiên, Template) --
    /// chỉ `ipc::` đọc/ghi (spec Boundaries Always: "registry trong
    /// `AppState`, chỉ `ipc/` điều phối"). Một khoá có mặt nghĩa là đúng một
    /// request `memo_generate` đang bay cho cặp đó -- gọi trùng thấy khoá
    /// này trả `AlreadyRunning` mà không tạo `CancellationToken` mới;
    /// `memo_cancel` gọi `.cancel()` trên token đang giữ ở đây. Gỡ khoá luôn
    /// nằm ở chính `memo_generate` (nhánh kết thúc, mọi ngả) -- không đâu
    /// khác được phép xoá khooản này ngoại trừ khi Phiên bị xoá/wipe (huỷ
    /// token trước, để `memo_generate` tự gỡ khoá khi tỉnh dậy).
    pub memo_running: Arc<Mutex<HashMap<(SessionId, MemoTemplateId), CancellationToken>>>,
    /// One export at a time. The token is installed before the native save
    /// dialog opens so cancellation and duplicate requests share one owner.
    pub recording_export: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    /// Coalesces window/menu/Cmd+Q close requests and lets an approved exit
    /// pass through Tauri's `ExitRequested` callback exactly once.
    pub close_requested: Arc<AtomicBool>,
    pub close_confirmed: Arc<AtomicBool>,
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

    let recovering = Arc::new(Mutex::new(HashSet::new()));
    if let Err(err) = &db {
        tracing::error!(error = %err, "DB không mở được, app vẫn khởi động");
    } else if let Ok(db) = &db {
        // Chỉ một chỗ duy nhất gọi `note_boot` mỗi lần khởi động (spec
        // Always "Crash"): DB lỗi ở đây không được phép chặn boot, chỉ log
        // cảnh báo (spec I/O Matrix "Crash": "DB lỗi → boot vẫn tiếp tục").
        if let Err(err) = crate::diagnostics::note_boot(db) {
            tracing::warn!(error = %err, "không ghi được marker crash lúc boot");
        }

        // Epic 5 owns the Ducking marker; its restoration hook belongs after
        // migrations and before staging cleanup. There is no Ducking marker yet.
        match app.path().app_data_dir() {
            Ok(data_dir) => {
                // Detach old staging atomically before any Job/Live writers
                // start; recursive deletion happens off the setup path.
                match crate::library::store::detach_staging(&data_dir) {
                    Err(error) => {
                        tracing::warn!(error = %error, "could not detach staging at boot; skipping recovery");
                    }
                    Ok(_detached_staging) => {
                        match (
                            crate::library::store::live_recovery_candidates(db),
                            crate::library::store::live_proxy_candidates(db),
                            crate::library::store::media_refs(db),
                        ) {
                            (Ok(interrupted), Ok(proxy_repairs), Ok(media_snapshot)) => {
                                let recovery_ids: HashSet<SessionId> =
                                    interrupted.iter().map(|candidate| candidate.id).collect();
                                let proxy_ids: HashSet<SessionId> =
                                    proxy_repairs.iter().map(|candidate| candidate.id).collect();
                                let mut claimed = recovery_ids.clone();
                                claimed.extend(proxy_ids.iter().copied());
                                if let Ok(mut busy) = recovering.lock() {
                                    busy.extend(claimed.iter().copied());
                                }
                                let db = db.clone();
                                let root = data_dir.clone();
                                let busy = recovering.clone();
                                let app = app.clone();
                                tauri::async_runtime::spawn_blocking(move || {
                                    if let Err(error) =
                                        crate::library::store::cleanup_detached_staging(&root)
                                    {
                                        tracing::warn!(error = %error, "could not remove detached staging at boot");
                                    }
                                    // Full media scan can be unbounded with a large
                                    // library, so it and all Proxy work stay behind
                                    // setup. Only rows captured before actors start
                                    // are eligible for recovery in this boot.
                                    if let Err(error) =
                                        crate::library::store::reconcile_media_snapshot(
                                            &db,
                                            &root,
                                            media_snapshot,
                                        )
                                    {
                                        tracing::warn!(error = %error, "could not reconcile media at boot");
                                    }

                                    match crate::library::store::live_recovery_candidates(&db) {
                                        Ok(candidates) => {
                                            for candidate in
                                                candidates.into_iter().filter(|candidate| {
                                                    recovery_ids.contains(&candidate.id)
                                                })
                                            {
                                                let session_id = candidate.id;
                                                match crate::library::store::recover_live_session(
                                                    &db, &root, candidate,
                                                ) {
                                                    Ok(true) => {
                                                        if let Err(error) =
                                                            (LiveRecoveryCompleted { session_id })
                                                                .emit(&app)
                                                        {
                                                            tracing::warn!(session_id = %session_id, error = %error, "could not notify Home about Live recovery");
                                                        }
                                                    }
                                                    Ok(false) => {}
                                                    Err(error) => {
                                                        tracing::warn!(session_id = %session_id, error = %error, "could not recover interrupted Live session")
                                                    }
                                                }
                                            }
                                        }
                                        Err(error) => {
                                            tracing::warn!(error = %error, "could not scan interrupted Live sessions at boot")
                                        }
                                    }

                                    match crate::library::store::live_proxy_candidates(&db) {
                                        Ok(candidates) => {
                                            for candidate in
                                                candidates.into_iter().filter(|candidate| {
                                                    proxy_ids.contains(&candidate.id)
                                                })
                                            {
                                                let session_id = candidate.id;
                                                match crate::library::store::repair_live_proxy(
                                                    &db, &root, candidate,
                                                ) {
                                                    Ok(true) => {
                                                        if let Err(error) =
                                                            (LiveRecoveryCompleted { session_id })
                                                                .emit(&app)
                                                        {
                                                            tracing::warn!(session_id = %session_id, error = %error, "could not notify Home about Live Proxy repair");
                                                        }
                                                    }
                                                    Ok(false) => {}
                                                    Err(error) => {
                                                        tracing::warn!(session_id = %session_id, error = %error, "could not repair Live Proxy at boot")
                                                    }
                                                }
                                            }
                                        }
                                        Err(error) => {
                                            tracing::warn!(error = %error, "could not scan Live Proxy repairs at boot")
                                        }
                                    }

                                    if let Ok(mut set) = busy.lock() {
                                        for session_id in claimed {
                                            set.remove(&session_id);
                                        }
                                    }
                                });
                            }
                            (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => {
                                tracing::warn!(error = %error, "could not scan Live sessions at boot");
                            }
                        }
                    }
                }
            }
            Err(error) => {
                tracing::warn!(error = %error, "could not resolve data directory; skipping recovery");
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

    let capture = Arc::new(crate::audio::CaptureController::production());
    let live = match (&db, &data_dir) {
        (Ok(db), Ok(data_dir)) => {
            let (handle, actor) = crate::live::channel(
                capture.clone(),
                db.clone(),
                data_dir.clone(),
                key_pool.clone(),
                crate::gemini::live::LiveGateway::production(key_pool.clone()),
            );
            tauri::async_runtime::spawn(actor.run());
            Ok(handle)
        }
        (Err(error), _) | (_, Err(error)) => Err(error.clone()),
    };

    AppState {
        capture,
        db,
        data_dir,
        secrets,
        key_pool,
        gateway,
        live,
        jobs,
        deleting: Arc::new(Mutex::new(HashSet::new())),
        recovering,
        wiping: Arc::new(AtomicBool::new(false)),
        memo_running: Arc::new(Mutex::new(HashMap::new())),
        recording_export: Arc::new(Mutex::new(None)),
        close_requested: Arc::new(AtomicBool::new(false)),
        close_confirmed: Arc::new(AtomicBool::new(false)),
        _log_guard: log_guard,
    }
}
