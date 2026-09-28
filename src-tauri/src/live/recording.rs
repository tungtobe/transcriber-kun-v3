//! Durable WAV writer for Live sessions. The receiver and disk writer run on
//! their own OS thread; capture callbacks and the Live WebSocket task never do
//! file I/O and cannot block one another.

use std::fs::{self, File, OpenOptions};
use std::io::BufWriter;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use tokio::sync::{broadcast, watch};

use crate::audio::{CaptureController, PcmChunk, OUTPUT_CHANNELS, OUTPUT_SAMPLE_RATE};
use crate::core::error::{AppError, Code};
use crate::core::id::SessionId;
use crate::core::paths;
use crate::db::Db;
use crate::library::store;
use crate::settings::UiLanguage;

const WAV_SPEC: hound::WavSpec = hound::WavSpec {
    channels: OUTPUT_CHANNELS,
    sample_rate: OUTPUT_SAMPLE_RATE,
    bits_per_sample: 16,
    sample_format: hound::SampleFormat::Int,
};
const CHECKPOINT_BYTES: u64 = 160_000;
const CHECKPOINT_PERIOD: Duration = Duration::from_secs(5);
const POLL_PERIOD: Duration = Duration::from_millis(10);

type RecordingWriter = hound::WavWriter<BufWriter<File>>;

/// A running Live recording. Story 4.6 can return `session_id` immediately
/// and watch `terminal_errors()` independently from the WebSocket state.
pub struct RecordingHandle {
    session_id: SessionId,
    stop_tx: Option<Sender<()>>,
    terminal_errors: watch::Receiver<Option<AppError>>,
    worker: Option<JoinHandle<Result<(), AppError>>>,
}

impl RecordingHandle {
    pub fn session_id(&self) -> SessionId {
        self.session_id
    }

    /// Clone this receiver into the LiveSession actor. It changes only when a
    /// terminal device or storage failure occurs.
    pub fn terminal_errors(&self) -> watch::Receiver<Option<AppError>> {
        self.terminal_errors.clone()
    }

    /// Stops the file consumer and finalizes the current WAV header. The
    /// caller controls capture shutdown so it can order both operations with
    /// the rest of Live finalization.
    pub fn stop(mut self) -> Result<(), AppError> {
        self.signal_stop();
        self.join_worker()
    }

    fn signal_stop(&mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }
    }

    fn join_worker(&mut self) -> Result<(), AppError> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        worker.join().map_err(|_| {
            AppError::new(
                Code::Storage,
                "Live Recording writer thread stopped unexpectedly",
            )
        })?
    }
}

impl Drop for RecordingHandle {
    fn drop(&mut self) {
        self.signal_stop();
    }
}

/// Starts a durable Recording for an already-open capture source. This does
/// not inspect consent, keys, model availability, or network state. When the
/// saved UI preference is `System`, callers that know the actual WebView/UI
/// locale should pass it to [`start_with_locale`]; the fallback here resolves
/// the process system locale explicitly.
pub fn start(
    capture: Arc<CaptureController>,
    db: &Db,
    data_dir: &Path,
    ui_language: UiLanguage,
) -> Result<RecordingHandle, AppError> {
    start_with_locale(capture, db, data_dir, ui_language, None)
}

/// Variant for callers that can pass the resolved UI locale (for example the
/// WebView locale when the saved preference is `System`). The supplied locale
/// is used only for `UiLanguage::System`; explicit `vi`, `en`, or `ja` settings
/// always take precedence. If omitted, the OS process locale environment is
/// used as a documented fallback.
pub fn start_with_locale(
    capture: Arc<CaptureController>,
    db: &Db,
    data_dir: &Path,
    ui_language: UiLanguage,
    resolved_ui_locale: Option<&str>,
) -> Result<RecordingHandle, AppError> {
    start_inner(capture, db, data_dir, ui_language, resolved_ui_locale, None)
}

fn start_inner(
    capture: Arc<CaptureController>,
    db: &Db,
    data_dir: &Path,
    ui_language: UiLanguage,
    resolved_ui_locale: Option<&str>,
    fail_after_samples: Option<u32>,
) -> Result<RecordingHandle, AppError> {
    if capture.active_source().is_none() {
        return Err(AppError::new(
            Code::Permission,
            "Audio capture must be open before a Live Recording can start",
        ));
    }

    // Subscribe before opening the file and writing the row. Any chunks which
    // arrive during initialization are buffered independently; overflow is
    // detected as a storage failure rather than silently hidden.
    let receiver = capture.subscribe();
    let session_id = SessionId::new();
    let session_dir = paths::media_dir(data_dir, session_id);
    let recording_path = paths::recording_path(data_dir, session_id);
    if let Err(err) =
        fs::create_dir_all(paths::media_root(data_dir)).and_then(|()| fs::create_dir(&session_dir))
    {
        capture.stop_capture();
        return Err(AppError::new(Code::Storage, err.to_string()));
    }
    let result = initialize_session(
        db,
        &recording_path,
        session_id,
        &default_title(Local::now(), ui_language, resolved_ui_locale),
    );
    let (writer, sync_file) = match result {
        Ok(pair) => pair,
        Err(err) => {
            capture.stop_capture();
            remove_session_dir(&session_dir);
            return Err(err);
        }
    };

    let (stop_tx, stop_rx) = mpsc::channel();
    let (terminal_tx, terminal_errors) = watch::channel(None);
    let worker_capture = capture.clone();
    let written_samples = Arc::new(AtomicU64::new(0));
    let worker_written_samples = written_samples.clone();
    let thread_result = thread::Builder::new()
        .name(format!("live-recording-{session_id}"))
        .spawn(move || {
            run_consumer(
                receiver,
                stop_rx,
                writer,
                sync_file,
                Some(worker_capture),
                terminal_tx,
                fail_after_samples,
                worker_written_samples,
            )
        });

    let worker = match thread_result {
        Ok(worker) => worker,
        Err(err) => {
            capture.stop_capture();
            if let Err(cleanup_err) = store::delete_live_session(db, session_id) {
                tracing::warn!(session_id = %session_id, error = %cleanup_err, "could not remove Live session after writer thread creation failed");
            }
            remove_session_dir(&session_dir);
            return Err(AppError::new(Code::Storage, err.to_string()));
        }
    };

    Ok(RecordingHandle {
        session_id,
        stop_tx: Some(stop_tx),
        terminal_errors,
        worker: Some(worker),
    })
}

fn initialize_session(
    db: &Db,
    recording_path: &Path,
    session_id: SessionId,
    title: &str,
) -> Result<(RecordingWriter, File), AppError> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(recording_path)?;
    let sync_file = file.try_clone()?;
    let mut writer = hound::WavWriter::new(BufWriter::new(file), WAV_SPEC)
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))?;
    checkpoint(&mut writer, &sync_file)?;

    // The file and a valid empty RIFF header exist before the transaction is
    // committed. Caller removes both if any database operation fails.
    store::create_live_session(db, session_id, title)?;
    Ok((writer, sync_file))
}

fn run_consumer(
    mut receiver: broadcast::Receiver<PcmChunk>,
    stop_rx: Receiver<()>,
    mut writer: RecordingWriter,
    sync_file: File,
    capture: Option<Arc<CaptureController>>,
    terminal_tx: watch::Sender<Option<AppError>>,
    fail_after_samples: Option<u32>,
    written_samples: Arc<AtomicU64>,
) -> Result<(), AppError> {
    let mut expected_sample = None;
    let mut uncheckpointed_bytes = 0_u64;
    let mut last_checkpoint = Instant::now();

    loop {
        match stop_rx.try_recv() {
            Ok(()) | Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => {}
        }

        if capture
            .as_ref()
            .is_some_and(|controller| controller.active_source().is_none())
        {
            let err = AppError::new(
                Code::Permission,
                "Audio capture stopped while the Live Recording was running",
            );
            return fail_recording(
                &mut writer,
                &sync_file,
                capture.as_ref(),
                &terminal_tx,
                err,
                true,
            );
        }

        match receiver.try_recv() {
            Ok(chunk) => {
                if let Some(source_error) = chunk.source_errors.first() {
                    return fail_recording(
                        &mut writer,
                        &sync_file,
                        capture.as_ref(),
                        &terminal_tx,
                        source_error.error.clone(),
                        true,
                    );
                }
                if chunk.sample_rate != OUTPUT_SAMPLE_RATE
                    || chunk.channels != OUTPUT_CHANNELS
                    || chunk.samples.len() != crate::audio::OUTPUT_CHUNK_SAMPLES
                {
                    return fail_recording(
                        &mut writer,
                        &sync_file,
                        capture.as_ref(),
                        &terminal_tx,
                        AppError::new(Code::Storage, "Live capture emitted an invalid PCM chunk"),
                        true,
                    );
                }
                if expected_sample.is_some_and(|expected| chunk.start_sample != expected) {
                    return fail_recording(
                        &mut writer,
                        &sync_file,
                        capture.as_ref(),
                        &terminal_tx,
                        AppError::new(
                            Code::Storage,
                            "Live capture lost PCM samples; Recording stopped to preserve continuity",
                        ),
                        true,
                    );
                }

                if fail_after_samples.is_some_and(|limit| writer.len() >= limit) {
                    return fail_recording(
                        &mut writer,
                        &sync_file,
                        capture.as_ref(),
                        &terminal_tx,
                        AppError::new(Code::Storage, "Injected Live Recording write failure"),
                        true,
                    );
                }

                for sample in &chunk.samples {
                    if let Err(err) = writer.write_sample(*sample) {
                        return fail_recording(
                            &mut writer,
                            &sync_file,
                            capture.as_ref(),
                            &terminal_tx,
                            AppError::new(Code::Storage, err.to_string()),
                            true,
                        );
                    }
                }
                written_samples.fetch_add(chunk.samples.len() as u64, Ordering::Release);
                expected_sample = Some(chunk.start_sample + chunk.samples.len() as u64);
                uncheckpointed_bytes = uncheckpointed_bytes
                    .saturating_add((chunk.samples.len() * std::mem::size_of::<i16>()) as u64);

                if uncheckpointed_bytes >= CHECKPOINT_BYTES
                    || last_checkpoint.elapsed() >= CHECKPOINT_PERIOD
                {
                    if let Err(err) = checkpoint(&mut writer, &sync_file) {
                        return fail_recording(
                            &mut writer,
                            &sync_file,
                            capture.as_ref(),
                            &terminal_tx,
                            err,
                            false,
                        );
                    }
                    uncheckpointed_bytes = 0;
                    last_checkpoint = Instant::now();
                }
            }
            Err(broadcast::error::TryRecvError::Lagged(count)) => {
                return fail_recording(
                    &mut writer,
                    &sync_file,
                    capture.as_ref(),
                    &terminal_tx,
                    AppError::new(
                        Code::Storage,
                        format!("Live Recording consumer lagged and lost {count} PCM chunks"),
                    ),
                    true,
                );
            }
            Err(broadcast::error::TryRecvError::Closed) => break,
            Err(broadcast::error::TryRecvError::Empty) => thread::sleep(POLL_PERIOD),
        }
    }

    if let Err(err) = checkpoint(&mut writer, &sync_file) {
        return fail_recording(
            &mut writer,
            &sync_file,
            capture.as_ref(),
            &terminal_tx,
            err,
            false,
        );
    }
    if let Err(err) = writer.finalize() {
        return report_terminal_failure(
            capture.as_ref(),
            &terminal_tx,
            AppError::new(Code::Storage, err.to_string()),
        );
    }
    if let Err(err) = sync_file.sync_data() {
        return report_terminal_failure(
            capture.as_ref(),
            &terminal_tx,
            AppError::new(Code::Storage, err.to_string()),
        );
    }
    Ok(())
}

fn fail_recording(
    writer: &mut RecordingWriter,
    sync_file: &File,
    capture: Option<&Arc<CaptureController>>,
    terminal_tx: &watch::Sender<Option<AppError>>,
    mut error: AppError,
    checkpoint_before_exit: bool,
) -> Result<(), AppError> {
    if checkpoint_before_exit {
        if let Err(checkpoint_error) = checkpoint(writer, sync_file) {
            error = checkpoint_error;
        }
    }
    report_terminal_failure(capture, terminal_tx, error)
}

fn report_terminal_failure(
    capture: Option<&Arc<CaptureController>>,
    terminal_tx: &watch::Sender<Option<AppError>>,
    error: AppError,
) -> Result<(), AppError> {
    if let Some(capture) = capture {
        capture.stop_capture();
    }
    let _ = terminal_tx.send(Some(error.clone()));
    Err(error)
}

fn checkpoint(writer: &mut RecordingWriter, sync_file: &File) -> Result<(), AppError> {
    writer
        .flush()
        .map_err(|err| AppError::new(Code::Storage, err.to_string()))?;
    sync_file.sync_data()?;
    Ok(())
}

fn remove_session_dir(session_dir: &Path) {
    if let Err(err) = fs::remove_dir_all(session_dir) {
        if err.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(path = %session_dir.display(), error = %err, "could not clean Live session directory after initialization failure");
        }
    }
}

fn default_title(
    now: DateTime<Local>,
    ui_language: UiLanguage,
    resolved_ui_locale: Option<&str>,
) -> String {
    let process_locale = system_locale();
    match resolve_ui_language(
        ui_language,
        resolved_ui_locale.or(process_locale.as_deref()),
    ) {
        UiLanguage::Ja => now.format("%Y年%-m月%-d日 %H:%M:%S").to_string(),
        UiLanguage::Vi => now.format("%d/%m/%Y %H:%M:%S").to_string(),
        UiLanguage::En | UiLanguage::System => now.format("%Y-%m-%d %H:%M:%S").to_string(),
    }
}

fn resolve_ui_language(preference: UiLanguage, system_locale: Option<&str>) -> UiLanguage {
    if preference != UiLanguage::System {
        return preference;
    }
    match system_locale
        .unwrap_or_default()
        .split(['_', '-', '.', '@'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "ja" => UiLanguage::Ja,
        "vi" => UiLanguage::Vi,
        _ => UiLanguage::En,
    }
}

fn system_locale() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty() && value != "C" && value != "POSIX")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{
        CaptureBackend, InputBlock, InputCallback, InputErrorCallback, InputFormat, LiveMicrophone,
        LiveSources, PreparedInput, PreparedSourceSet, PreparedStream, SourceInput,
    };
    use crate::db::repo;
    use std::io::{BufRead, BufReader, Read};
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::tempdir;

    #[test]
    fn recording_start_requires_capture_before_creating_a_row_or_file() {
        let root = tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        let capture = Arc::new(CaptureController::new(Arc::new(TestBackend::default())));

        let error = match start(capture, &db, root.path(), UiLanguage::En) {
            Err(error) => error,
            Ok(_) => panic!("Recording must wait until capture is open"),
        };

        assert_eq!(error.category, crate::core::error::Category::Permission);
        assert!(!paths::media_root(root.path()).exists());
        db.with_connection(|conn| {
            assert!(repo::sessions::list(conn)?.is_empty());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn storage_initialization_failure_stops_capture_without_a_row_or_header_file() {
        let root = tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        let blocked_root = root.path().join("blocked-data-root");
        fs::write(&blocked_root, b"not a directory").unwrap();
        let capture = Arc::new(CaptureController::new(Arc::new(TestBackend::default())));
        capture.set_source("mic:Built-in").unwrap();

        let error = match start(capture.clone(), &db, &blocked_root, UiLanguage::En) {
            Err(error) => error,
            Ok(_) => panic!("blocked storage root must reject Recording startup"),
        };

        assert_eq!(error.category, crate::core::error::Category::Storage);
        assert!(capture.active_source().is_none());
        db.with_connection(|conn| {
            assert!(repo::sessions::list(conn)?.is_empty());
            Ok(())
        })
        .unwrap();
        assert!(blocked_root.is_file());
    }

    #[test]
    fn database_insert_failure_removes_header_file_and_live_row() {
        let root = tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        db.with_connection(|conn| {
            conn.execute_batch(
                "CREATE TRIGGER reject_live_session BEFORE INSERT ON sessions \
                 WHEN NEW.kind = 'live' BEGIN SELECT RAISE(ABORT, 'injected insert failure'); END;",
            )?;
            Ok(())
        })
        .unwrap();
        let capture = Arc::new(CaptureController::new(Arc::new(TestBackend::default())));
        capture.set_source("mic:Built-in").unwrap();

        let error = match start(capture.clone(), &db, root.path(), UiLanguage::En) {
            Err(error) => error,
            Ok(_) => panic!("injected DB failure must reject Recording startup"),
        };

        assert_eq!(error.category, crate::core::error::Category::Storage);
        assert!(capture.active_source().is_none());
        assert!(fs::read_dir(paths::media_root(root.path()))
            .unwrap()
            .next()
            .is_none());
        db.with_connection(|conn| {
            assert!(repo::sessions::list(conn)?.is_empty());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn recording_starts_offline_and_keeps_writing_when_another_receiver_is_unused() {
        let root = tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        let capture = Arc::new(CaptureController::new(Arc::new(TestBackend::default())));
        capture.set_source("mic:Built-in").unwrap();
        // A separate, unread subscriber represents a failed/lagging Live WS.
        let _websocket_audio = capture.subscribe();

        let handle = start(capture.clone(), &db, root.path(), UiLanguage::Ja).unwrap();
        let id = handle.session_id();
        let row = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, id)?.unwrap()))
            .unwrap();
        assert_eq!(row.kind, "live");
        assert_eq!(row.status, "recording");
        assert!(paths::recording_path(root.path(), id).is_file());
        std::thread::sleep(Duration::from_millis(350));
        handle.stop().unwrap();

        let mut wav = hound::WavReader::open(paths::recording_path(root.path(), id)).unwrap();
        assert_eq!(wav.spec().sample_rate, 16_000);
        assert_eq!(wav.spec().channels, 1);
        assert_eq!(wav.spec().bits_per_sample, 16);
        assert!(wav.duration() >= 1_600);
        assert!(wav.samples::<i16>().any(|sample| sample.unwrap() != 0));
        assert_eq!(capture.active_source().as_deref(), Some("mic:Built-in"));
    }

    #[test]
    fn storage_write_failure_stops_capture_and_keeps_row_and_checkpointed_audio() {
        let root = tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        let capture = Arc::new(CaptureController::new(Arc::new(TestBackend::default())));
        capture.set_source("mic:Built-in").unwrap();

        let handle = start_inner(
            capture.clone(),
            &db,
            root.path(),
            UiLanguage::En,
            None,
            Some(1_600),
        )
        .unwrap();
        let id = handle.session_id();
        std::thread::sleep(Duration::from_millis(350));
        let terminal = handle.terminal_errors();
        let error = terminal
            .borrow()
            .clone()
            .expect("storage failure is terminal");
        assert_eq!(error.category, crate::core::error::Category::Storage);
        assert!(capture.active_source().is_none());
        assert_eq!(
            db.with_connection(|conn| Ok(repo::sessions::get(conn, id)?.unwrap().status))
                .unwrap(),
            "recording"
        );
        let file = paths::recording_path(root.path(), id);
        assert!(file.is_file());
        let mut wav = hound::WavReader::open(file).unwrap();
        assert!(wav.duration() >= 1_600);
        assert!(wav.samples::<i16>().any(|sample| sample.unwrap() != 0));
        let _ = handle.stop();
    }

    #[test]
    fn consumer_lag_is_a_storage_error_and_releases_capture() {
        let root = tempdir().unwrap();
        let capture = Arc::new(CaptureController::new(Arc::new(TestBackend::default())));
        capture.set_source("mic:Built-in").unwrap();
        let (tx, rx) = broadcast::channel(1);
        tx.send(test_chunk(0, 0.3)).unwrap();
        tx.send(test_chunk(1_600, 0.3)).unwrap();
        let (_stop_tx, stop_rx) = mpsc::channel();
        let (terminal_tx, terminal_rx) = watch::channel(None);
        let (writer, sync_file) = new_test_writer(&root.path().join("lag.wav"));
        let handle = thread::spawn({
            let capture = capture.clone();
            move || {
                run_consumer(
                    rx,
                    stop_rx,
                    writer,
                    sync_file,
                    Some(capture),
                    terminal_tx,
                    None,
                    Arc::new(AtomicU64::new(0)),
                )
            }
        });
        let result = handle.join().unwrap();
        assert_eq!(
            result.unwrap_err().category,
            crate::core::error::Category::Storage
        );
        assert_eq!(
            terminal_rx.borrow().as_ref().unwrap().category,
            crate::core::error::Category::Storage
        );
        assert!(capture.active_source().is_none());
        drop(tx);
    }

    #[test]
    fn sample_discontinuity_stops_capture_and_keeps_the_prior_contiguous_chunk() {
        let root = tempdir().unwrap();
        let capture = Arc::new(CaptureController::new(Arc::new(TestBackend::default())));
        capture.set_source("mic:Built-in").unwrap();
        let (tx, rx) = broadcast::channel(4);
        tx.send(test_chunk(0, 0.3)).unwrap();
        tx.send(test_chunk(3_200, 0.3)).unwrap();
        let (_stop_tx, stop_rx) = mpsc::channel();
        let (terminal_tx, terminal_rx) = watch::channel(None);
        let (writer, sync_file) = new_test_writer(&root.path().join("discontinuity.wav"));
        let worker_capture = capture.clone();
        let result = thread::spawn(move || {
            run_consumer(
                rx,
                stop_rx,
                writer,
                sync_file,
                Some(worker_capture),
                terminal_tx,
                None,
                Arc::new(AtomicU64::new(0)),
            )
        })
        .join()
        .unwrap();

        assert_eq!(
            result.unwrap_err().category,
            crate::core::error::Category::Storage
        );
        assert_eq!(
            terminal_rx.borrow().as_ref().unwrap().category,
            crate::core::error::Category::Storage
        );
        assert!(capture.active_source().is_none());
        let wav = hound::WavReader::open(root.path().join("discontinuity.wav")).unwrap();
        assert_eq!(wav.duration(), 1_600);
    }

    #[test]
    fn source_device_error_stops_capture_without_misreporting_storage() {
        let root = tempdir().unwrap();
        let capture = Arc::new(CaptureController::new(Arc::new(TestBackend::default())));
        capture.set_source("mic:Built-in").unwrap();
        let (tx, rx) = broadcast::channel(2);
        tx.send(PcmChunk {
            source_errors: vec![crate::audio::AudioSourceError {
                source: "mic:Built-in".to_owned(),
                error: AppError::new(Code::Permission, "device disconnected"),
            }],
            ..test_chunk(0, 0.3)
        })
        .unwrap();
        let (_stop_tx, stop_rx) = mpsc::channel();
        let (terminal_tx, terminal_rx) = watch::channel(None);
        let (writer, sync_file) = new_test_writer(&root.path().join("device-error.wav"));
        let worker_capture = capture.clone();
        let result = thread::spawn(move || {
            run_consumer(
                rx,
                stop_rx,
                writer,
                sync_file,
                Some(worker_capture),
                terminal_tx,
                None,
                Arc::new(AtomicU64::new(0)),
            )
        })
        .join()
        .unwrap();

        assert_eq!(
            result.unwrap_err().category,
            crate::core::error::Category::Permission
        );
        assert_eq!(
            terminal_rx.borrow().as_ref().unwrap().category,
            crate::core::error::Category::Permission
        );
        assert!(capture.active_source().is_none());
    }

    #[test]
    fn title_uses_explicit_ui_or_system_locale_without_transcribe_language_guessing() {
        assert_eq!(
            resolve_ui_language(UiLanguage::System, Some("ja_JP.UTF-8")),
            UiLanguage::Ja
        );
        assert_eq!(
            resolve_ui_language(UiLanguage::System, Some("vi_VN")),
            UiLanguage::Vi
        );
        assert_eq!(
            resolve_ui_language(UiLanguage::System, None),
            UiLanguage::En
        );
        assert_eq!(
            resolve_ui_language(UiLanguage::Vi, Some("ja_JP")),
            UiLanguage::Vi
        );
        assert!(default_title(Local::now(), UiLanguage::System, Some("ja_JP")).contains('年'));
        assert!(default_title(Local::now(), UiLanguage::System, Some("vi_VN")).contains('/'));
    }

    #[test]
    fn force_kill_after_checkpoint_leaves_a_reader_valid_wave() {
        let dir = tempdir().unwrap();
        let wav_path = dir.path().join("checkpoint.wav");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "live::recording::tests::checkpoint_writer_child_waits_for_parent_kill",
                "--nocapture",
            ])
            .env("LIVE_RECORDING_KILL_TEST_PATH", &wav_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (checkpoint_tx, checkpoint_rx) = mpsc::channel();
        thread::spawn(move || {
            let result = BufReader::new(stdout)
                .lines()
                .any(|line| line.is_ok_and(|line| line.contains("RECORDING_CHECKPOINTED")));
            let _ = checkpoint_tx.send(result);
        });
        let saw_checkpoint = checkpoint_rx.recv_timeout(Duration::from_secs(15));
        if !matches!(saw_checkpoint, Ok(true)) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("child did not checkpoint before timeout: {saw_checkpoint:?}");
        }
        child.kill().unwrap();
        let _ = child.wait();

        let mut wav = hound::WavReader::open(&wav_path).unwrap();
        assert_eq!(wav.spec().sample_rate, 16_000);
        assert_eq!(wav.spec().channels, 1);
        assert_eq!(wav.spec().bits_per_sample, 16);
        assert_eq!(wav.duration(), 80_000);
        let consumed_samples_before_kill = 52 * crate::audio::OUTPUT_CHUNK_SAMPLES as u32;
        assert!(consumed_samples_before_kill - wav.duration() <= OUTPUT_SAMPLE_RATE * 5);
        assert!(wav.samples::<i16>().any(|sample| sample.unwrap() != 0));
    }

    #[test]
    fn checkpoint_writer_child_waits_for_parent_kill() {
        let Ok(path) = std::env::var("LIVE_RECORDING_KILL_TEST_PATH") else {
            return;
        };
        let (writer, sync_file) = new_test_writer(Path::new(&path));
        let (tx, rx) = broadcast::channel(64);
        let (_stop_tx, stop_rx) = mpsc::channel();
        let (terminal_tx, _terminal_rx) = watch::channel(None);
        let written_samples = Arc::new(AtomicU64::new(0));
        let worker_written_samples = written_samples.clone();
        thread::spawn(move || {
            run_consumer(
                rx,
                stop_rx,
                writer,
                sync_file,
                None,
                terminal_tx,
                None,
                worker_written_samples,
            )
            .unwrap();
        });
        for index in 0..50_u64 {
            tx.send(test_chunk(index * 1_600, 0.3)).unwrap();
            std::thread::sleep(Duration::from_millis(100));
        }
        loop {
            let mut header = [0_u8; 44];
            if File::open(&path)
                .and_then(|mut file| file.read_exact(&mut header))
                .is_ok()
                && &header[0..4] == b"RIFF"
                && u32::from_le_bytes(header[40..44].try_into().unwrap()) >= 160_000
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        for index in 50..52_u64 {
            tx.send(test_chunk(index * 1_600, 0.3)).unwrap();
        }
        while written_samples.load(Ordering::Acquire) < 83_200 {
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut checkpoint_header = [0_u8; 44];
        File::open(&path)
            .and_then(|mut file| file.read_exact(&mut checkpoint_header))
            .unwrap();
        assert_eq!(
            u32::from_le_bytes(checkpoint_header[40..44].try_into().unwrap()),
            160_000,
            "two consumed chunks after checkpoint remain the bounded uncheckpointed tail"
        );
        println!("RECORDING_CHECKPOINTED");
        use std::io::Write;
        std::io::stdout().flush().unwrap();
        loop {
            thread::park();
        }
    }

    fn new_test_writer(path: &Path) -> (RecordingWriter, File) {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        let sync_file = file.try_clone().unwrap();
        let mut writer = hound::WavWriter::new(BufWriter::new(file), WAV_SPEC).unwrap();
        checkpoint(&mut writer, &sync_file).unwrap();
        (writer, sync_file)
    }

    fn test_chunk(start_sample: u64, value: f32) -> PcmChunk {
        PcmChunk {
            start_sample,
            sample_rate: OUTPUT_SAMPLE_RATE,
            channels: OUTPUT_CHANNELS,
            samples: vec![(value * i16::MAX as f32) as i16; crate::audio::OUTPUT_CHUNK_SAMPLES],
            source_errors: Vec::new(),
        }
    }

    #[derive(Default)]
    struct TestBackend {
        starts: Arc<AtomicUsize>,
    }

    impl CaptureBackend for TestBackend {
        fn live_sources(&self) -> Result<LiveSources, AppError> {
            Ok(LiveSources {
                microphones: vec![LiveMicrophone {
                    source: "mic:Built-in".to_owned(),
                    name: "Built-in".to_owned(),
                    is_default: true,
                }],
                default_microphone: Some("mic:Built-in".to_owned()),
                system_available: false,
            })
        }

        fn prepare(
            &self,
            inputs: &[SourceInput],
            generation: u64,
            on_audio: InputCallback,
            _on_error: InputErrorCallback,
        ) -> Result<PreparedSourceSet, AppError> {
            Ok(PreparedSourceSet {
                inputs: inputs
                    .iter()
                    .map(|input| {
                        let format = InputFormat {
                            side: input.side,
                            source: input.source.clone(),
                            sample_rate: OUTPUT_SAMPLE_RATE,
                            channels: 1,
                        };
                        PreparedInput {
                            stream: Box::new(TestStream {
                                generation,
                                format: format.clone(),
                                on_audio: on_audio.clone(),
                                starts: self.starts.clone(),
                            }),
                            format,
                        }
                    })
                    .collect(),
            })
        }
    }

    struct TestStream {
        generation: u64,
        format: InputFormat,
        on_audio: InputCallback,
        starts: Arc<AtomicUsize>,
    }

    impl PreparedStream for TestStream {
        fn start(&mut self) -> Result<(), AppError> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            (self.on_audio)(InputBlock {
                generation: self.generation,
                side: self.format.side,
                sample_rate: self.format.sample_rate,
                channels: self.format.channels,
                samples: vec![0.3; 1_600],
            });
            Ok(())
        }
    }
}
