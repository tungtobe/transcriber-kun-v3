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
use crate::core::id::{SessionId, TagId};
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
const SOURCE_STOP_GRACE: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, PartialEq, Eq)]
enum StopMode {
    Immediate,
    AfterCaptureStops,
}

type RecordingWriter = hound::WavWriter<BufWriter<File>>;

/// A running Live recording. Story 4.6 can return `session_id` immediately
/// and watch `terminal_errors()` independently from the WebSocket state.
pub struct RecordingHandle {
    session_id: SessionId,
    stop_tx: Option<Sender<StopMode>>,
    terminal_errors: watch::Receiver<Option<AppError>>,
    worker: Option<JoinHandle<Result<(), AppError>>>,
    wav_finalized: Arc<std::sync::atomic::AtomicBool>,
}

#[derive(Debug)]
pub struct RecordingStopOutcome {
    pub wav_finalized: bool,
    pub error: Option<AppError>,
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
    pub fn stop(mut self) -> RecordingStopOutcome {
        self.request_stop();
        let error = self.join_worker().err();
        RecordingStopOutcome {
            wav_finalized: self.wav_finalized.load(Ordering::Acquire),
            error,
        }
    }

    /// Signals an intentional shutdown without blocking on the writer thread.
    /// The consumer drains already-published PCM before finalizing the WAV.
    pub fn request_stop(&mut self) {
        self.signal_stop(StopMode::Immediate);
    }

    /// Marks an intentional Live shutdown before the capture source is closed.
    /// The writer keeps consuming until capture stops, then drains queued PCM.
    pub fn begin_shutdown(&mut self) {
        self.signal_stop(StopMode::AfterCaptureStops);
    }

    fn signal_stop(&mut self, mode: StopMode) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(mode);
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
        self.signal_stop(StopMode::Immediate);
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
    let receiver = capture.subscribe();
    start_inner_with_receiver(
        capture,
        receiver,
        db,
        data_dir,
        ui_language,
        resolved_ui_locale,
        &[],
        None,
    )
}

/// Starts Recording with a receiver subscribed before capture opens. The
/// LiveSession actor uses this to retain the first capture chunk while source
/// initialization completes.
pub fn start_with_receiver(
    capture: Arc<CaptureController>,
    receiver: broadcast::Receiver<PcmChunk>,
    db: &Db,
    data_dir: &Path,
    ui_language: UiLanguage,
    resolved_ui_locale: Option<&str>,
) -> Result<RecordingHandle, AppError> {
    start_with_receiver_and_tags(
        capture,
        receiver,
        db,
        data_dir,
        ui_language,
        resolved_ui_locale,
        &[],
    )
}

/// Starts a Recording and attaches the initially selected tags atomically
/// with its session row.
pub fn start_with_receiver_and_tags(
    capture: Arc<CaptureController>,
    receiver: broadcast::Receiver<PcmChunk>,
    db: &Db,
    data_dir: &Path,
    ui_language: UiLanguage,
    resolved_ui_locale: Option<&str>,
    tag_ids: &[TagId],
) -> Result<RecordingHandle, AppError> {
    start_inner_with_receiver(
        capture,
        receiver,
        db,
        data_dir,
        ui_language,
        resolved_ui_locale,
        tag_ids,
        None,
    )
}

#[cfg(test)]
fn start_inner(
    capture: Arc<CaptureController>,
    db: &Db,
    data_dir: &Path,
    ui_language: UiLanguage,
    resolved_ui_locale: Option<&str>,
    fail_after_samples: Option<u32>,
) -> Result<RecordingHandle, AppError> {
    let receiver = capture.subscribe();
    start_inner_with_receiver(
        capture,
        receiver,
        db,
        data_dir,
        ui_language,
        resolved_ui_locale,
        &[],
        fail_after_samples,
    )
}

fn start_inner_with_receiver(
    capture: Arc<CaptureController>,
    receiver: broadcast::Receiver<PcmChunk>,
    db: &Db,
    data_dir: &Path,
    ui_language: UiLanguage,
    resolved_ui_locale: Option<&str>,
    tag_ids: &[TagId],
    fail_after_samples: Option<u32>,
) -> Result<RecordingHandle, AppError> {
    if capture.active_source().is_none() {
        return Err(AppError::new(
            Code::Permission,
            "Audio capture must be open before a Live Recording can start",
        ));
    }

    // The caller subscribes before opening the file and writing the row. Any
    // chunks which arrive during initialization are buffered independently;
    // overflow is detected as a storage failure rather than silently hidden.
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
        tag_ids,
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
    let wav_finalized = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_wav_finalized = wav_finalized.clone();
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
                worker_wav_finalized,
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
        wav_finalized,
    })
}

fn initialize_session(
    db: &Db,
    recording_path: &Path,
    session_id: SessionId,
    title: &str,
    tag_ids: &[TagId],
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
    store::create_live_session_with_tags(db, session_id, title, tag_ids)?;
    Ok((writer, sync_file))
}

fn run_consumer(
    mut receiver: broadcast::Receiver<PcmChunk>,
    stop_rx: Receiver<StopMode>,
    mut writer: RecordingWriter,
    sync_file: File,
    capture: Option<Arc<CaptureController>>,
    terminal_tx: watch::Sender<Option<AppError>>,
    fail_after_samples: Option<u32>,
    written_samples: Arc<AtomicU64>,
    wav_finalized: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), AppError> {
    let mut expected_sample = None;
    let mut uncheckpointed_bytes = 0_u64;
    let mut last_checkpoint = Instant::now();
    let mut stop_mode = None;
    // Sources that reported an error. Recording only fails once every input
    // that was active has errored (Mixed keeps going on the surviving one).
    let mut errored_sources: std::collections::HashSet<String> = std::collections::HashSet::new();

    loop {
        if stop_mode.is_none() {
            match stop_rx.try_recv() {
                Ok(mode) => stop_mode = Some(mode),
                Err(TryRecvError::Disconnected) => stop_mode = Some(StopMode::Immediate),
                Err(TryRecvError::Empty) => {}
            }
        }

        let source_stopped = capture
            .as_ref()
            .is_some_and(|controller| controller.active_source().is_none());
        if stop_mode.is_none() && source_stopped {
            // A device can disappear independently of Live stop. Give the
            // actor's intentional shutdown signal a short window to arrive.
            thread::sleep(SOURCE_STOP_GRACE);
            match stop_rx.try_recv() {
                Ok(mode) => stop_mode = Some(mode),
                Err(TryRecvError::Disconnected) => stop_mode = Some(StopMode::Immediate),
                Err(TryRecvError::Empty) => {
                    let err = AppError::new(
                        Code::Permission,
                        "Audio capture stopped while the Live Recording was running",
                    );
                    return fail_recording(
                        writer,
                        &sync_file,
                        capture.as_ref(),
                        &terminal_tx,
                        err,
                        true,
                        &wav_finalized,
                    );
                }
            }
        }

        match receiver.try_recv() {
            Ok(chunk) => {
                if let Some(source_error) = chunk.source_errors.first() {
                    for reported in &chunk.source_errors {
                        errored_sources.insert(reported.source.clone());
                    }
                    let active_inputs = capture
                        .as_ref()
                        .and_then(|controller| controller.active_source())
                        .map_or(1, |source| if source.starts_with("mixed:") { 2 } else { 1 });
                    if errored_sources.len() >= active_inputs {
                        return fail_recording(
                            writer,
                            &sync_file,
                            capture.as_ref(),
                            &terminal_tx,
                            source_error.error.clone(),
                            true,
                            &wav_finalized,
                        );
                    }
                    tracing::warn!(
                        failed = errored_sources.len(),
                        active = active_inputs,
                        "a Live audio input failed; recording continues on the remaining input"
                    );
                }
                if chunk.sample_rate != OUTPUT_SAMPLE_RATE
                    || chunk.channels != OUTPUT_CHANNELS
                    || chunk.samples.len() != crate::audio::OUTPUT_CHUNK_SAMPLES
                {
                    return fail_recording(
                        writer,
                        &sync_file,
                        capture.as_ref(),
                        &terminal_tx,
                        AppError::new(Code::Storage, "Live capture emitted an invalid PCM chunk"),
                        true,
                        &wav_finalized,
                    );
                }
                if expected_sample.is_some_and(|expected| chunk.start_sample != expected) {
                    return fail_recording(
                        writer,
                        &sync_file,
                        capture.as_ref(),
                        &terminal_tx,
                        AppError::new(
                            Code::Storage,
                            "Live capture lost PCM samples; Recording stopped to preserve continuity",
                        ),
                        true,
                        &wav_finalized,
                    );
                }

                if fail_after_samples.is_some_and(|limit| writer.len() >= limit) {
                    return fail_recording(
                        writer,
                        &sync_file,
                        capture.as_ref(),
                        &terminal_tx,
                        AppError::new(Code::Storage, "Injected Live Recording write failure"),
                        true,
                        &wav_finalized,
                    );
                }

                for sample in &chunk.samples {
                    if let Err(err) = writer.write_sample(*sample) {
                        return fail_recording(
                            writer,
                            &sync_file,
                            capture.as_ref(),
                            &terminal_tx,
                            AppError::new(Code::Storage, err.to_string()),
                            true,
                            &wav_finalized,
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
                            writer,
                            &sync_file,
                            capture.as_ref(),
                            &terminal_tx,
                            err,
                            false,
                            &wav_finalized,
                        );
                    }
                    uncheckpointed_bytes = 0;
                    last_checkpoint = Instant::now();
                }
            }
            Err(broadcast::error::TryRecvError::Lagged(count)) => {
                // Keep the timeline intact: the missed chunks become silence.
                tracing::warn!(
                    chunks = count,
                    "Live Recording consumer lagged; filling silence"
                );
                let silence_samples =
                    count.saturating_mul(crate::audio::OUTPUT_CHUNK_SAMPLES as u64);
                for _ in 0..silence_samples {
                    if let Err(err) = writer.write_sample(0) {
                        return fail_recording(
                            writer,
                            &sync_file,
                            capture.as_ref(),
                            &terminal_tx,
                            AppError::new(Code::Storage, err.to_string()),
                            true,
                            &wav_finalized,
                        );
                    }
                }
                written_samples.fetch_add(silence_samples, Ordering::Release);
                expected_sample = expected_sample.map(|expected| expected + silence_samples);
                uncheckpointed_bytes = uncheckpointed_bytes.saturating_add(
                    silence_samples.saturating_mul(std::mem::size_of::<i16>() as u64),
                );
            }
            Err(broadcast::error::TryRecvError::Closed) => break,
            Err(broadcast::error::TryRecvError::Empty) => match stop_mode {
                Some(StopMode::Immediate) => break,
                Some(StopMode::AfterCaptureStops) if source_stopped => break,
                Some(StopMode::AfterCaptureStops) | None => thread::sleep(POLL_PERIOD),
            },
        }
    }

    if let Err(err) = checkpoint(&mut writer, &sync_file) {
        return fail_recording(
            writer,
            &sync_file,
            capture.as_ref(),
            &terminal_tx,
            err,
            false,
            &wav_finalized,
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
    wav_finalized.store(true, Ordering::Release);
    Ok(())
}

fn fail_recording(
    mut writer: RecordingWriter,
    sync_file: &File,
    capture: Option<&Arc<CaptureController>>,
    terminal_tx: &watch::Sender<Option<AppError>>,
    mut error: AppError,
    checkpoint_before_exit: bool,
    wav_finalized: &std::sync::atomic::AtomicBool,
) -> Result<(), AppError> {
    if checkpoint_before_exit {
        if let Err(checkpoint_error) = checkpoint(&mut writer, sync_file) {
            error = checkpoint_error;
        }
    }
    match writer.finalize() {
        Ok(()) => match sync_file.sync_data() {
            Ok(()) => wav_finalized.store(true, Ordering::Release),
            Err(sync_error) => error = AppError::new(Code::Storage, sync_error.to_string()),
        },
        Err(finalize_error) => {
            error = AppError::new(Code::Storage, finalize_error.to_string());
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
        OUTPUT_CHUNK_SAMPLES,
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
        let stopped = handle.stop();
        assert!(stopped.wav_finalized);
        assert!(stopped.error.is_none());

        let mut wav = hound::WavReader::open(paths::recording_path(root.path(), id)).unwrap();
        assert_eq!(wav.spec().sample_rate, 16_000);
        assert_eq!(wav.spec().channels, 1);
        assert_eq!(wav.spec().bits_per_sample, 16);
        assert!(wav.duration() >= 1_600);
        assert!(wav.samples::<i16>().any(|sample| sample.unwrap() != 0));
        assert_eq!(capture.active_source().as_deref(), Some("mic:Built-in"));
    }

    #[test]
    fn pre_subscribed_recording_receiver_writes_the_first_capture_chunk() {
        let root = tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        let capture = Arc::new(CaptureController::new(Arc::new(TestBackend::default())));
        let recording_receiver = capture.subscribe();
        let mut observer = capture.subscribe();
        capture.set_source("mic:Built-in").unwrap();

        // Wait until the first output-clock tick has published the synchronous
        // TestStream input. The recording receiver was subscribed before open.
        let deadline = Instant::now() + Duration::from_secs(2);
        let first_chunk = loop {
            match observer.try_recv() {
                Ok(chunk) => break chunk,
                Err(broadcast::error::TryRecvError::Lagged(count)) => {
                    panic!("observer unexpectedly lagged by {count} chunks")
                }
                Err(broadcast::error::TryRecvError::Closed) => panic!("capture closed"),
                Err(broadcast::error::TryRecvError::Empty) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(2));
                }
                Err(broadcast::error::TryRecvError::Empty) => {
                    panic!("capture did not publish its first chunk")
                }
            }
        };
        assert_eq!(first_chunk.start_sample, 0);
        assert!(first_chunk.samples.iter().any(|sample| *sample != 0));

        let handle = start_with_receiver(
            capture.clone(),
            recording_receiver,
            &db,
            root.path(),
            UiLanguage::En,
            None,
        )
        .unwrap();
        let id = handle.session_id();
        let mut handle = handle;
        handle.begin_shutdown();
        capture.stop_capture();
        let stopped = handle.stop();
        assert!(stopped.wav_finalized);
        assert!(stopped.error.is_none());

        let mut wav = hound::WavReader::open(paths::recording_path(root.path(), id)).unwrap();
        let wav_samples = wav.samples::<i16>().map(Result::unwrap).collect::<Vec<_>>();
        assert!(wav_samples.len() >= OUTPUT_CHUNK_SAMPLES);
        assert_eq!(
            &wav_samples[..OUTPUT_CHUNK_SAMPLES],
            first_chunk.samples.as_slice(),
            "WAV must begin with the chunk published during source initialization"
        );
    }

    #[test]
    fn writer_keeps_all_thirty_six_thousand_chunks_independent_of_live_ring_outage() {
        let root = tempdir().unwrap();
        let path = root.path().join("sixty-minute-continuous.wav");
        let (writer, sync_file) = new_test_writer(&path);
        let (tx, rx) = broadcast::channel(36_001);
        for index in 0..36_000_u64 {
            tx.send(PcmChunk {
                start_sample: index * crate::audio::OUTPUT_CHUNK_SAMPLES as u64,
                sample_rate: OUTPUT_SAMPLE_RATE,
                channels: OUTPUT_CHANNELS,
                samples: vec![index as i16; crate::audio::OUTPUT_CHUNK_SAMPLES],
                gated_samples: vec![index as i16; crate::audio::OUTPUT_CHUNK_SAMPLES],
                source_errors: Vec::new(),
            })
            .unwrap();
        }
        // The production writer has its own capture receiver. Dropping this
        // sender closes only that synthetic stream after its full backlog is
        // drained; the Gemini ring's 600-chunk eviction is tested separately.
        drop(tx);
        let (_stop_tx, stop_rx) = mpsc::channel();
        let (terminal_tx, _terminal_rx) = watch::channel(None);
        let written_samples = Arc::new(AtomicU64::new(0));
        let worker_samples = written_samples.clone();
        let result = thread::spawn(move || {
            run_consumer(
                rx,
                stop_rx,
                writer,
                sync_file,
                None,
                terminal_tx,
                None,
                worker_samples,
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            )
        })
        .join()
        .unwrap();

        result.unwrap();
        let expected_samples = 36_000 * crate::audio::OUTPUT_CHUNK_SAMPLES as u64;
        assert_eq!(written_samples.load(Ordering::Acquire), expected_samples);
        let wav = hound::WavReader::open(path).unwrap();
        assert_eq!(wav.duration() as u64, expected_samples);
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
        let stopped = handle.stop();
        assert!(stopped.wav_finalized);
        assert_eq!(
            stopped.error.as_ref().map(|error| error.category),
            Some(crate::core::error::Category::Storage)
        );
    }

    #[test]
    fn consumer_lag_fills_silence_and_keeps_recording() {
        let root = tempdir().unwrap();
        let (tx, rx) = broadcast::channel(1);
        tx.send(test_chunk(0, 0.3)).unwrap();
        tx.send(test_chunk(1_600, 0.3)).unwrap();
        tx.send(test_chunk(3_200, 0.3)).unwrap();
        drop(tx);
        let (_stop_tx, stop_rx) = mpsc::channel();
        let (terminal_tx, terminal_rx) = watch::channel(None);
        let path = root.path().join("lag.wav");
        let (writer, sync_file) = new_test_writer(&path);
        let written = Arc::new(AtomicU64::new(0));
        run_consumer(
            rx,
            stop_rx,
            writer,
            sync_file,
            None,
            terminal_tx,
            None,
            written.clone(),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .unwrap();
        assert!(terminal_rx.borrow().is_none());
        let samples = hound::WavReader::open(&path)
            .unwrap()
            .samples::<i16>()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        assert_eq!(samples.len(), 4_800);
        assert_eq!(written.load(Ordering::Acquire), 4_800);
        assert!(samples[..3_200].iter().all(|sample| *sample == 0));
        assert!(samples[3_200..].iter().all(|sample| *sample != 0));
    }

    #[test]
    fn one_failed_mixed_input_keeps_recording_but_losing_every_input_fails() {
        let root = tempdir().unwrap();
        let capture = Arc::new(CaptureController::new(Arc::new(TestBackend::default())));
        capture.set_source("mixed:Built-in").unwrap();
        let (tx, rx) = broadcast::channel(8);
        let failure = |source: &str| crate::audio::AudioSourceError {
            source: source.to_owned(),
            error: AppError::new(Code::Permission, "device disconnected"),
        };
        tx.send(PcmChunk {
            source_errors: vec![failure("mic:Built-in")],
            ..test_chunk(0, 0.3)
        })
        .unwrap();
        tx.send(test_chunk(1_600, 0.3)).unwrap();
        tx.send(PcmChunk {
            source_errors: vec![failure("system")],
            ..test_chunk(3_200, 0.3)
        })
        .unwrap();
        tx.send(test_chunk(4_800, 0.3)).unwrap();
        let (_stop_tx, stop_rx) = mpsc::channel();
        let (terminal_tx, _terminal_rx) = watch::channel(None);
        let path = root.path().join("mixed.wav");
        let (writer, sync_file) = new_test_writer(&path);
        let result = run_consumer(
            rx,
            stop_rx,
            writer,
            sync_file,
            Some(capture),
            terminal_tx,
            None,
            Arc::new(AtomicU64::new(0)),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
        assert!(result.is_err());
        // The first two chunks were kept; the all-inputs-failed chunk is not.
        let wav = hound::WavReader::open(&path).unwrap();
        assert_eq!(wav.duration(), 3_200);
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
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
        let wav_path = root.path().join("device-error.wav");
        let (writer, sync_file) = new_test_writer(&wav_path);
        let wav_finalized = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_wav_finalized = wav_finalized.clone();
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
                worker_wav_finalized,
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
        assert!(wav_finalized.load(Ordering::Acquire));
        assert_eq!(hound::WavReader::open(wav_path).unwrap().duration(), 0);
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
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            )
            .unwrap();
        });
        for index in 0..50_u64 {
            tx.send(test_chunk(index * 1_600, 0.3)).unwrap();
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

    #[test]
    fn boot_recovery_crash_child_waits_for_parent_kill() {
        let Ok(root) = std::env::var("LIVE_BOOT_RECOVERY_ROOT") else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        let db = Db::open(&root).unwrap();
        let session_id = crate::core::id::SessionId::new();
        store::create_live_session(&db, session_id, "Force killed Live").unwrap();
        let transcript_id =
            store::create_live_transcript(&db, session_id, "crash-test", None).unwrap();
        store::append_live_batch(
            &db,
            session_id,
            transcript_id,
            &[crate::db::repo::segments::SegmentDraft {
                start_sec: 0.0,
                end_sec: 1.0,
                kind: crate::db::repo::segments::SegmentKind::Text,
                gap_reason: None,
                text: "durably flushed before kill".to_owned(),
                speaker: None,
            }],
            5.0,
            "recording",
        )
        .unwrap();

        let wav_path = paths::recording_path(&root, session_id);
        fs::create_dir_all(wav_path.parent().unwrap()).unwrap();
        let (writer, sync_file) = new_test_writer(&wav_path);
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
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            )
            .unwrap();
        });
        for index in 0..50_u64 {
            tx.send(test_chunk(index * 1_600, 0.3)).unwrap();
        }
        while written_samples.load(Ordering::Acquire) < 80_000 {
            thread::sleep(Duration::from_millis(10));
        }
        for index in 50..52_u64 {
            tx.send(test_chunk(index * 1_600, 0.3)).unwrap();
        }
        while written_samples.load(Ordering::Acquire) < 83_200 {
            thread::sleep(Duration::from_millis(10));
        }
        println!("LIVE_BOOT_RECOVERY_CHECKPOINTED:{session_id}");
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
            gated_samples: vec![
                (value * i16::MAX as f32) as i16;
                crate::audio::OUTPUT_CHUNK_SAMPLES
            ],
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
                microphone_permission: crate::audio::PermissionState::Unknown,
                system_permission: crate::audio::PermissionState::Unknown,
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
