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

use crate::library::store::ClaimGuard;

/// The Recording export currently holding the one export slot. `session_id`
/// lets delete/wipe refuse (Busy) while the session's WAV is being read.
#[derive(Debug, Clone)]
pub struct ActiveRecordingExport {
    pub session_id: SessionId,
    pub cancel: Arc<AtomicBool>,
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
    /// One process-local recommendation preview. Restart, cancel, apply, or a
    /// newer preview makes the prior token unusable.
    pub recommended_previews: Arc<crate::settings::recommended::RecommendedPreviewStore>,
    /// One export at a time. The token is installed before the native save
    /// dialog opens so cancellation and duplicate requests share one owner.
    pub recording_export: Arc<Mutex<Option<ActiveRecordingExport>>>,
    /// Coalesces window/menu/Cmd+Q close requests and lets an approved exit
    /// pass through Tauri's `ExitRequested` callback exactly once.
    /// Unix ms of the active close request, `0` when none. Claims expire so a
    /// frontend that never answered cannot block later close attempts.
    pub close_requested: Arc<std::sync::atomic::AtomicU64>,
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

        match app.path().app_data_dir() {
            Ok(data_dir) => {
                // Ducking marker: after migrations (`Db::open` above) and
                // before staging cleanup/recovery.
                restore_ducked_volume(&mut *crate::audio::output_volume::platform_volume(), &data_dir);
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
                                // Interrupted sessions are claimed up front; Proxy
                                // repairs are claimed lazily, one at a time.
                                if let Ok(mut busy) = recovering.lock() {
                                    busy.extend(recovery_ids.iter().copied());
                                }
                                let db = db.clone();
                                let root = data_dir.clone();
                                let busy = recovering.clone();
                                let app = app.clone();
                                tauri::async_runtime::spawn_blocking(move || {
                                    run_boot_recovery(
                                        &db,
                                        &root,
                                        media_snapshot,
                                        &recovery_ids,
                                        &proxy_ids,
                                        busy,
                                        |session_id| {
                                            if let Err(error) =
                                                (LiveRecoveryCompleted { session_id }).emit(&app)
                                            {
                                                tracing::warn!(session_id = %session_id, error = %error, "could not notify Home about Live recovery");
                                            }
                                        },
                                    );
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
            tauri::async_runtime::spawn(actor
                    .with_recovering(recovering.clone())
                    .with_playback(crate::audio::playback::platform_backend)
                    .with_ducking(
                        crate::audio::output_volume::platform_volume(),
                        crate::core::paths::ducking_marker_path(data_dir),
                    )
                    .run());
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
        recommended_previews: Arc::new(Default::default()),
        recording_export: Arc::new(Mutex::new(None)),
        close_requested: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        close_confirmed: Arc::new(AtomicBool::new(false)),
        _log_guard: log_guard,
    }
}

/// Restores the system volume a crashed run left ducked (marker-guarded: only
/// the recorded device, only while its volume is still the app's ducked
/// level), then drops the marker. Also used on app exit.
pub fn restore_ducked_volume(
    volume: &mut dyn crate::audio::output_volume::OutputVolume,
    data_dir: &std::path::Path,
) {
    crate::audio::output_volume::restore_from_marker(
        volume,
        &crate::core::paths::ducking_marker_path(data_dir),
    );
}

/// The startup recovery worker body. Interrupted sessions (`recovery_ids`)
/// stay claimed for the whole run (released on every exit, panic included);
/// Proxy repairs (`proxy_ids`) claim each session lazily, only while it is
/// being encoded, and skip one that another worker (a just-finished Stop)
/// already holds. `notify` runs for each session that changed.
fn run_boot_recovery(
    db: &Db,
    root: &std::path::Path,
    media_snapshot: Vec<(SessionId, String, Option<String>)>,
    recovery_ids: &HashSet<SessionId>,
    proxy_ids: &HashSet<SessionId>,
    busy: Arc<Mutex<HashSet<SessionId>>>,
    notify: impl Fn(SessionId),
) {
    // The caller inserted `recovery_ids` into `busy`; this guard releases them
    // on every exit, panic included.
    let _claims = ClaimGuard::new(busy.clone(), recovery_ids.clone());
    if let Err(error) = crate::library::store::cleanup_detached_staging(root) {
        tracing::warn!(error = %error, "could not remove detached staging at boot");
    }
    // Full media scan can be unbounded with a large library, so it and all
    // Proxy work stay behind setup. Only rows captured before actors start are
    // eligible for recovery in this boot.
    if let Err(error) = crate::library::store::reconcile_media_snapshot(db, root, media_snapshot) {
        tracing::warn!(error = %error, "could not reconcile media at boot");
    }

    match crate::library::store::live_recovery_candidates(db) {
        Ok(candidates) => {
            for candidate in candidates
                .into_iter()
                .filter(|candidate| recovery_ids.contains(&candidate.id))
            {
                let session_id = candidate.id;
                match crate::library::store::recover_live_session(db, root, candidate) {
                    Ok(true) => notify(session_id),
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

    match crate::library::store::live_proxy_candidates(db) {
        Ok(candidates) => {
            for candidate in candidates
                .into_iter()
                .filter(|candidate| proxy_ids.contains(&candidate.id))
            {
                let session_id = candidate.id;
                let Some(_claim) = ClaimGuard::try_claim(&busy, session_id) else {
                    continue;
                };
                match crate::library::store::repair_live_proxy(db, root, candidate) {
                    Ok(true) => notify(session_id),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_restores_a_crashed_duck_and_drops_the_marker() {
        use crate::audio::output_volume::{fake::FakeVolume, Ducker};
        let root = tempfile::tempdir().unwrap();
        let marker = crate::core::paths::ducking_marker_path(root.path());
        let fake = FakeVolume::with_device("spk", 0.8);
        let mut ducker = Ducker::new(Box::new(fake.clone()), marker.clone());
        ducker.duck();
        drop(ducker); // crash while ducked
        assert!(fake.level("spk").unwrap() < 0.3);
        restore_ducked_volume(&mut fake.clone(), root.path());
        assert!((fake.level("spk").unwrap() - 0.8).abs() < 1e-4);
        assert!(!marker.exists());
    }

    #[test]
    fn claim_guard_releases_claims_on_panic() {
        let id = SessionId::new();
        let other = SessionId::new();
        let set = Arc::new(Mutex::new(HashSet::from([id, other])));
        let worker_set = set.clone();
        let result = std::panic::catch_unwind(move || {
            let _guard = ClaimGuard::new(worker_set, HashSet::from([id]));
            panic!("recovery blew up");
        });
        assert!(result.is_err());
        let held = set.lock().unwrap_or_else(|e| e.into_inner());
        assert!(!held.contains(&id));
        assert!(held.contains(&other));
    }

    fn write_wav(path: &std::path::Path) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for sample in 0..16_000 {
            writer.write_sample((sample % 100) as i16).unwrap();
        }
        writer.finalize().unwrap();
    }

    /// Returns `(interrupted, completed_without_proxy)` session ids.
    fn seed_boot_sessions(root: &std::path::Path, db: &Db) -> (SessionId, SessionId) {
        use crate::library::store;
        let interrupted = SessionId::new();
        store::create_live_session(db, interrupted, "interrupted").unwrap();
        let completed = SessionId::new();
        store::create_live_session(db, completed, "completed").unwrap();
        let transcript = store::create_live_transcript(db, completed, "m", None).unwrap();
        store::append_live_batch(db, completed, transcript, &[], 1.0, "finalizing").unwrap();
        store::finalize_live_session_minimal(db, completed, 1.0).unwrap();
        for id in [interrupted, completed] {
            let dir = crate::core::paths::media_dir(root, id);
            std::fs::create_dir_all(&dir).unwrap();
            write_wav(&crate::core::paths::recording_path(root, id));
        }
        (interrupted, completed)
    }

    #[test]
    fn boot_recovery_recovers_and_repairs_then_releases_every_claim() {
        let root = tempfile::tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        let (interrupted, completed) = seed_boot_sessions(root.path(), &db);
        let busy = Arc::new(Mutex::new(HashSet::from([interrupted])));
        let notified = Mutex::new(Vec::new());
        let snapshot = crate::library::store::media_refs(&db).unwrap();

        run_boot_recovery(
            &db,
            root.path(),
            snapshot,
            &HashSet::from([interrupted]),
            &HashSet::from([completed]),
            busy.clone(),
            |id| notified.lock().unwrap().push(id),
        );

        let notified = notified.into_inner().unwrap();
        assert!(notified.contains(&interrupted));
        assert!(notified.contains(&completed));
        assert!(busy.lock().unwrap().is_empty(), "no claim may leak");
        db.with_connection(|conn| {
            let recovered = crate::db::repo::sessions::get(conn, interrupted)?.unwrap();
            assert_eq!(recovered.status, "complete");
            assert!(recovered.recovered);
            let repaired = crate::db::repo::sessions::get(conn, completed)?.unwrap();
            assert_eq!(repaired.proxy_ext.as_deref(), Some("flac"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn boot_recovery_claims_proxy_repairs_lazily_and_skips_a_busy_one() {
        let root = tempfile::tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        let (_interrupted, completed) = seed_boot_sessions(root.path(), &db);
        // Another worker (a just-finished Stop) already owns this session.
        let busy = Arc::new(Mutex::new(HashSet::from([completed])));
        let notified = Mutex::new(Vec::new());
        let snapshot = crate::library::store::media_refs(&db).unwrap();

        run_boot_recovery(
            &db,
            root.path(),
            snapshot,
            &HashSet::new(),
            &HashSet::from([completed]),
            busy.clone(),
            |id| notified.lock().unwrap().push(id),
        );

        assert!(notified.into_inner().unwrap().is_empty());
        // The foreign claim is untouched; nothing was repaired under it.
        assert!(busy.lock().unwrap().contains(&completed));
        db.with_connection(|conn| {
            let row = crate::db::repo::sessions::get(conn, completed)?.unwrap();
            assert_eq!(row.proxy_ext, None);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn boot_recovery_releases_claims_even_when_the_worker_panics() {
        let root = tempfile::tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        let (interrupted, _completed) = seed_boot_sessions(root.path(), &db);
        let busy = Arc::new(Mutex::new(HashSet::from([interrupted])));
        let snapshot = crate::library::store::media_refs(&db).unwrap();
        let worker_busy = busy.clone();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_boot_recovery(
                &db,
                root.path(),
                snapshot,
                &HashSet::from([interrupted]),
                &HashSet::new(),
                worker_busy,
                |_| panic!("notification blew up"),
            );
        }));

        assert!(result.is_err());
        assert!(busy.lock().unwrap_or_else(|e| e.into_inner()).is_empty());
    }
}
