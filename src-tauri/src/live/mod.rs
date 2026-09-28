//! Process-wide LiveSession actor. It owns one live generation, its durable
//! transcript cursor and the seq stream shared by all IPC subscribers.

pub mod recording;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::ipc::Channel;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::audio::{CaptureController, OUTPUT_SAMPLE_RATE};
use crate::core::error::{AppError, Code};
use crate::core::id::{SessionId, TranscriptId};
use crate::db::repo::segments::{GapReason, SegmentDraft, SegmentKind};
use crate::db::Db;
use crate::gemini::keys::KeyPoolHandle;
use crate::gemini::live::{
    LiveConnectionState as GatewayConnectionState, LiveEvent as GatewayEvent, LiveGateway,
    LiveRunConfig,
};
use crate::gemini::{CancellationToken, ConsentSnapshot};
use crate::library::store;
use crate::settings::{TranscribeLanguage, UiLanguage};

const COMMAND_CAPACITY: usize = 64;
const INTERNAL_CAPACITY: usize = 256;
// The output clock advances in 100ms chunks, then the actor can observe a
// threshold one 250ms poll late. A 4.6s threshold leaves margin for both.
const FLUSH_SAMPLES: u64 = OUTPUT_SAMPLE_RATE as u64 * 23 / 5;
const FLUSH_CHECK_PERIOD: Duration = Duration::from_millis(250);
const OLD_GENERATION_DRAIN: Duration = Duration::from_secs(1);

fn actor_error() -> AppError {
    AppError::new(Code::Storage, "LiveSession actor unavailable")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Reconnecting {
        #[specta(type = specta_typescript::Number)]
        since_ms: i64,
    },
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum RecordingState {
    Active,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LiveSegment {
    pub start_sec: f64,
    pub end_sec: f64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LiveSnapshot {
    pub session_id: Option<SessionId>,
    pub transcript_id: Option<TranscriptId>,
    pub recording: RecordingState,
    pub connection: ConnectionState,
    pub duration_sec: f64,
}

/// Typed Live stream. The ready variant carries the snapshot and cursor sent
/// by the actor in the same command that registers the Channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LiveEvent {
    Ready {
        #[specta(type = specta_typescript::Number)]
        seq: u64,
        snapshot: LiveSnapshot,
    },
    Delta {
        #[specta(type = specta_typescript::Number)]
        seq: u64,
        text: String,
    },
    Turn {
        #[specta(type = specta_typescript::Number)]
        seq: u64,
    },
    Segment {
        #[specta(type = specta_typescript::Number)]
        seq: u64,
        segment: LiveSegment,
    },
    Gap {
        #[specta(type = specta_typescript::Number)]
        seq: u64,
        start_sec: f64,
        end_sec: f64,
        reason: String,
    },
    Recording {
        #[specta(type = specta_typescript::Number)]
        seq: u64,
        state: RecordingState,
    },
    Connection {
        #[specta(type = specta_typescript::Number)]
        seq: u64,
        state: ConnectionState,
    },
    Log {
        #[specta(type = specta_typescript::Number)]
        seq: u64,
        message: String,
    },
    Error {
        #[specta(type = specta_typescript::Number)]
        seq: u64,
        error: AppError,
    },
    Done {
        #[specta(type = specta_typescript::Number)]
        seq: u64,
    },
    Final {
        #[specta(type = specta_typescript::Number)]
        seq: u64,
        session_id: SessionId,
        transcript_id: TranscriptId,
        duration_sec: f64,
    },
}

#[derive(Debug, Clone)]
pub struct LiveStartParams {
    pub source: String,
    pub language: TranscribeLanguage,
    pub locale: Option<String>,
    pub ui_language: UiLanguage,
    pub model: String,
    pub consent: ConsentSnapshot,
}

#[derive(Clone)]
pub struct LiveSessionHandle {
    commands: mpsc::Sender<Command>,
}

impl LiveSessionHandle {
    pub async fn start(&self, params: LiveStartParams) -> Result<SessionId, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Start { params, reply })
            .await
            .map_err(|_| actor_error())?;
        response.await.map_err(|_| actor_error())?
    }

    pub async fn stop(&self) -> Result<(), AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Stop { reply })
            .await
            .map_err(|_| actor_error())?;
        response.await.map_err(|_| actor_error())?
    }

    pub async fn subscribe(&self, channel: Channel<LiveEvent>) -> Result<(), AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Subscribe { channel, reply })
            .await
            .map_err(|_| actor_error())?;
        response.await.map_err(|_| actor_error())
    }
}

enum Command {
    Start {
        params: LiveStartParams,
        reply: oneshot::Sender<Result<SessionId, AppError>>,
    },
    Stop {
        reply: oneshot::Sender<Result<(), AppError>>,
    },
    Subscribe {
        channel: Channel<LiveEvent>,
        reply: oneshot::Sender<()>,
    },
}

enum Internal {
    GatewayEvent {
        generation: u64,
        event: GatewayEvent,
    },
    GatewayEnded {
        generation: u64,
        result: Result<(), crate::gemini::live::LiveFailure>,
    },
}

struct RunningSession {
    generation: u64,
    session_id: SessionId,
    transcript_id: TranscriptId,
    baseline_sample: u64,
    last_flush_sample: u64,
    pending: Vec<SegmentDraft>,
    sentence_buffer: String,
    sentence_start: Option<u64>,
    sentence_end: u64,
    recording: Option<recording::RecordingHandle>,
    terminal_errors: watch::Receiver<Option<AppError>>,
    gateway_cancel: CancellationToken,
    gateway_task: JoinHandle<()>,
    connection: ConnectionState,
    recording_state: RecordingState,
    storage_error_reported: bool,
}

pub struct LiveSessionActor {
    receiver: mpsc::Receiver<Command>,
    internal_sender: mpsc::Sender<Internal>,
    internal_receiver: mpsc::Receiver<Internal>,
    capture: Arc<CaptureController>,
    db: Arc<Db>,
    data_dir: PathBuf,
    key_pool: KeyPoolHandle,
    gateway: LiveGateway,
    next_generation: u64,
    seq: u64,
    subscribers: Vec<Channel<LiveEvent>>,
    snapshot: LiveSnapshot,
    running: Option<RunningSession>,
}

/// Creates the one process-wide LiveSession handle and actor.
pub fn channel(
    capture: Arc<CaptureController>,
    db: Arc<Db>,
    data_dir: PathBuf,
    key_pool: KeyPoolHandle,
    gateway: LiveGateway,
) -> (LiveSessionHandle, LiveSessionActor) {
    let (commands, receiver) = mpsc::channel(COMMAND_CAPACITY);
    let (internal_sender, internal_receiver) = mpsc::channel(INTERNAL_CAPACITY);
    (
        LiveSessionHandle { commands },
        LiveSessionActor {
            receiver,
            internal_sender,
            internal_receiver,
            capture,
            db,
            data_dir,
            key_pool,
            gateway,
            next_generation: 1,
            seq: 0,
            subscribers: Vec::new(),
            snapshot: LiveSnapshot {
                session_id: None,
                transcript_id: None,
                recording: RecordingState::Stopped,
                connection: ConnectionState::Stopped,
                duration_sec: 0.0,
            },
            running: None,
        },
    )
}

impl LiveSessionActor {
    pub async fn run(mut self) {
        let mut flush_check = tokio::time::interval(FLUSH_CHECK_PERIOD);
        flush_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                command = self.receiver.recv() => match command {
                    Some(Command::Start { params, reply }) => {
                        let result = self.start(params).await;
                        let _ = reply.send(result);
                    }
                    Some(Command::Stop { reply }) => {
                        let result = self.finish_current(None).await;
                        let _ = reply.send(result);
                    }
                    Some(Command::Subscribe { channel, reply }) => {
                        self.register_subscriber(channel);
                        let _ = reply.send(());
                    }
                    None => {
                        let _ = self.finish_current(None).await;
                        break;
                    }
                },
                internal = self.internal_receiver.recv() => {
                    if let Some(internal) = internal {
                        self.handle_internal(internal).await;
                    }
                }
                _ = flush_check.tick() => {
                    self.check_running_session().await;
                }
            }
        }
    }

    async fn start(&mut self, params: LiveStartParams) -> Result<SessionId, AppError> {
        if self.running.is_some() {
            return Err(AppError::new(
                Code::Request,
                "A Live session is already running",
            ));
        }
        if !params.consent.is_current() {
            return Err(AppError::new(
                Code::Blocked,
                "Gemini Live requires current consent",
            ));
        }
        if !self.key_pool.has_eligible_key().await? {
            return Err(AppError::new(
                Code::Auth,
                "No configured Gemini key is available",
            ));
        }

        // Subscribe before opening capture so synchronous/first-tick audio is
        // retained for both the independent WAV writer and the Live gateway.
        let gateway_audio = self.capture.subscribe();
        let recording_audio = self.capture.subscribe();
        let baseline_sample = self.capture.sample_clock();
        let capture = self.capture.clone();
        let source = params.source.clone();
        match tokio::task::spawn_blocking(move || capture.set_source(&source)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Err(error),
            Err(err) => {
                let cleanup = self.capture.clone();
                let _ = tokio::task::spawn_blocking(move || cleanup.stop_capture()).await;
                return Err(AppError::new(Code::Storage, err.to_string()));
            }
        }

        let capture = self.capture.clone();
        let db = self.db.clone();
        let data_dir = self.data_dir.clone();
        let ui_language = params.ui_language;
        let locale = params.locale.clone();
        let recording_result = tokio::task::spawn_blocking(move || {
            recording::start_with_receiver(
                capture,
                recording_audio,
                &db,
                &data_dir,
                ui_language,
                locale.as_deref(),
            )
        })
        .await;
        let recording = match recording_result {
            Ok(Ok(recording)) => recording,
            Ok(Err(error)) => return Err(error),
            Err(err) => {
                let cleanup = self.capture.clone();
                let _ = tokio::task::spawn_blocking(move || cleanup.stop_capture()).await;
                return Err(AppError::new(Code::Storage, err.to_string()));
            }
        };
        let session_id = recording.session_id();
        let transcript = {
            let db = self.db.clone();
            let model = params.model.clone();
            let language = params.language.as_code().map(str::to_owned);
            match tokio::task::spawn_blocking(move || {
                store::create_live_transcript(&db, session_id, &model, language.as_deref())
            })
            .await
            {
                Ok(result) => result,
                Err(err) => Err(AppError::new(Code::Storage, err.to_string())),
            }
        };
        let transcript_id = match transcript {
            Ok(id) => id,
            Err(error) => {
                // The session row and checkpointed WAV are recovery evidence;
                // stop both workers, but keep those durable artifacts.
                let mut recording = recording;
                recording.begin_shutdown();
                let capture = self.capture.clone();
                let _ = tokio::task::spawn_blocking(move || capture.stop_capture()).await;
                let _ = tokio::task::spawn_blocking(move || recording.stop()).await;
                return Err(error);
            }
        };

        let generation = self.next_generation;
        self.next_generation = self.next_generation.saturating_add(1);
        let cancellation = CancellationToken::new();
        let (gateway_events, mut gateway_event_rx) = mpsc::channel(64);
        let sender = self.internal_sender.clone();
        let gateway = self.gateway.clone();
        let task_cancellation = cancellation.clone();
        let config = LiveRunConfig {
            model: params.model,
            language: params.language,
        };
        let consent = params.consent;
        // This task owns one gateway run; its reconnect loop replaces sockets
        // sequentially and does not mint actor generations. A future source,
        // target, or redetect swap within a session should bump the generation
        // before its replacement task can publish events.
        let gateway_task = tokio::spawn(async move {
            let run = gateway.run(
                config,
                consent,
                gateway_audio,
                gateway_events,
                task_cancellation,
            );
            tokio::pin!(run);
            let mut events_open = true;
            let result = loop {
                tokio::select! {
                    result = &mut run => break result,
                    event = gateway_event_rx.recv(), if events_open => {
                        if let Some(event) = event {
                            if sender.send(Internal::GatewayEvent { generation, event }).await.is_err() {
                                return;
                            }
                        } else {
                            events_open = false;
                        }
                    }
                }
            };
            while let Some(event) = gateway_event_rx.recv().await {
                if sender
                    .send(Internal::GatewayEvent { generation, event })
                    .await
                    .is_err()
                {
                    return;
                }
            }
            let _ = sender
                .send(Internal::GatewayEnded { generation, result })
                .await;
        });
        let terminal_errors = recording.terminal_errors();
        self.running = Some(RunningSession {
            generation,
            session_id,
            transcript_id,
            baseline_sample,
            last_flush_sample: baseline_sample,
            pending: Vec::new(),
            sentence_buffer: String::new(),
            sentence_start: None,
            sentence_end: baseline_sample,
            recording: Some(recording),
            terminal_errors,
            gateway_cancel: cancellation,
            gateway_task,
            connection: ConnectionState::Connecting,
            recording_state: RecordingState::Active,
            storage_error_reported: false,
        });
        self.snapshot = self.snapshot_for_running();
        self.emit(LiveEvent::Recording {
            seq: 0,
            state: RecordingState::Active,
        });
        self.emit(LiveEvent::Connection {
            seq: 0,
            state: ConnectionState::Connecting,
        });
        self.emit(LiveEvent::Log {
            seq: 0,
            message: "Live session started".to_owned(),
        });
        Ok(session_id)
    }

    async fn handle_internal(&mut self, internal: Internal) {
        match internal {
            Internal::GatewayEvent { generation, event } => {
                let current = self
                    .running
                    .as_ref()
                    .is_some_and(|running| running.generation == generation);
                if !current {
                    return;
                }
                match event {
                    GatewayEvent::ConnectionChanged { state } => {
                        let current = self
                            .running
                            .as_ref()
                            .map(|running| running.connection)
                            .unwrap_or(ConnectionState::Stopped);
                        let state = transition_connection(current, state, epoch_ms());
                        if let Some(running) = self.running.as_mut() {
                            running.connection = state;
                        }
                        self.snapshot = self.snapshot_for_running();
                        self.emit(LiveEvent::Connection { seq: 0, state });
                    }
                    GatewayEvent::InputTranscription {
                        text,
                        sample_start,
                        sample_end,
                    } => {
                        let text = text.expose().clone();
                        self.emit(LiveEvent::Delta {
                            seq: 0,
                            text: text.clone(),
                        });
                        let completed = if let Some(running) = self.running.as_mut() {
                            if running.sentence_buffer.is_empty() {
                                running.sentence_start = Some(sample_start);
                            }
                            running.sentence_buffer.push_str(&text);
                            running.sentence_end = sample_end.max(sample_start);
                            split_complete_sentences(running)
                        } else {
                            Vec::new()
                        };
                        for segment in completed {
                            self.emit(LiveEvent::Segment { seq: 0, segment });
                        }
                    }
                    GatewayEvent::TurnComplete {
                        sample_start,
                        sample_end,
                    } => {
                        let completed = if let Some(running) = self.running.as_mut() {
                            if !running.sentence_buffer.trim().is_empty() {
                                let start = running.sentence_start.unwrap_or(sample_start);
                                let end = sample_end.max(running.sentence_end).max(start);
                                let text = std::mem::take(&mut running.sentence_buffer);
                                let draft = text_draft(start, end, running.baseline_sample, text);
                                running.pending.push(draft.clone());
                                running.sentence_start = None;
                                Some(LiveSegment {
                                    start_sec: draft.start_sec,
                                    end_sec: draft.end_sec,
                                    text: draft.text,
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        if let Some(segment) = completed {
                            self.emit(LiveEvent::Segment { seq: 0, segment });
                        }
                        self.emit(LiveEvent::Turn { seq: 0 });
                        if self.flush_active("recording").await.is_err() {
                            // Persistent transcript storage is terminal for
                            // this Live session: stop capture and keep the
                            // prior DB checkpoints plus durable WAV bounded.
                            let _ = self.finish_current(None).await;
                        }
                    }
                    GatewayEvent::AudioGap {
                        start_sample,
                        end_sample,
                    } => {
                        let mapped = if let Some(running) = self.running.as_mut() {
                            let start = relative_seconds(start_sample, running.baseline_sample);
                            let end =
                                relative_seconds(end_sample, running.baseline_sample).max(start);
                            running.pending.push(SegmentDraft {
                                start_sec: start,
                                end_sec: end,
                                kind: SegmentKind::Gap,
                                gap_reason: Some(GapReason::Disconnected),
                                text: String::new(),
                                speaker: None,
                            });
                            Some((start, end))
                        } else {
                            None
                        };
                        if let Some((start_sec, end_sec)) = mapped {
                            self.emit(LiveEvent::Gap {
                                seq: 0,
                                start_sec,
                                end_sec,
                                reason: "disconnected".to_owned(),
                            });
                        }
                    }
                }
            }
            Internal::GatewayEnded { generation, result } => {
                let current = self
                    .running
                    .as_ref()
                    .is_some_and(|running| running.generation == generation);
                if !current {
                    return;
                }
                let emit_stopped = if let Some(running) = self.running.as_mut() {
                    let changed = running.connection != ConnectionState::Stopped;
                    running.connection = ConnectionState::Stopped;
                    changed
                } else {
                    false
                };
                if emit_stopped {
                    self.snapshot = self.snapshot_for_running();
                    self.emit(LiveEvent::Connection {
                        seq: 0,
                        state: ConnectionState::Stopped,
                    });
                }
                if let Err(failure) = result {
                    match failure {
                        crate::gemini::live::LiveFailure::Cancelled => {}
                        crate::gemini::live::LiveFailure::Gateway(error) => {
                            self.emit(LiveEvent::Error { seq: 0, error });
                        }
                        crate::gemini::live::LiveFailure::SetupRejected => {
                            self.emit(LiveEvent::Error {
                                seq: 0,
                                error: AppError::new(Code::Model, "Gemini Live setup was rejected"),
                            });
                        }
                    }
                }
            }
        }
    }

    async fn check_running_session(&mut self) {
        let writer_error = self
            .running
            .as_ref()
            .and_then(|running| running.terminal_errors.borrow().clone());
        if let Some(error) = writer_error {
            let _ = self.finish_current(Some(error)).await;
            return;
        }
        let due = self.running.as_ref().is_some_and(|running| {
            self.capture
                .sample_clock()
                .saturating_sub(running.last_flush_sample)
                >= FLUSH_SAMPLES
        });
        if due {
            if self.flush_active("recording").await.is_err() {
                let _ = self.finish_current(None).await;
            }
        }
    }

    async fn flush_active(&mut self, status: &'static str) -> Result<(), AppError> {
        let Some(running) = self.running.as_mut() else {
            return Ok(());
        };
        let sample_clock = self.capture.sample_clock();
        let duration_sec = relative_seconds(sample_clock, running.baseline_sample);
        let session_id = running.session_id;
        let transcript_id = running.transcript_id;
        let generation = running.generation;
        let pending = std::mem::take(&mut running.pending);
        let batch = pending.clone();
        let db = self.db.clone();
        let persisted = tokio::task::spawn_blocking(move || {
            store::append_live_batch(&db, session_id, transcript_id, &batch, duration_sec, status)
        })
        .await;
        let persisted = match persisted {
            Ok(result) => result,
            Err(err) => Err(AppError::new(Code::Storage, err.to_string())),
        };
        match persisted {
            Ok(()) => {
                if let Some(running) = self
                    .running
                    .as_mut()
                    .filter(|running| running.generation == generation)
                {
                    running.last_flush_sample = sample_clock;
                    running.storage_error_reported = false;
                }
                self.snapshot = self.snapshot_for_running();
                Ok(())
            }
            Err(error) => {
                let mut report = false;
                if let Some(running) = self
                    .running
                    .as_mut()
                    .filter(|running| running.generation == generation)
                {
                    running.pending.extend(pending);
                    if !running.storage_error_reported {
                        running.storage_error_reported = true;
                        report = true;
                    }
                }
                if report {
                    self.emit(LiveEvent::Error {
                        seq: 0,
                        error: error.clone(),
                    });
                }
                Err(error)
            }
        }
    }

    async fn finish_current(&mut self, writer_error: Option<AppError>) -> Result<(), AppError> {
        if self.running.is_none() {
            return Ok(());
        }
        let final_segment = if let Some(running) = self.running.as_mut() {
            if !running.sentence_buffer.trim().is_empty() {
                let start = running.sentence_start.unwrap_or(running.sentence_end);
                let text = std::mem::take(&mut running.sentence_buffer);
                let draft = text_draft(
                    start,
                    running.sentence_end.max(start),
                    running.baseline_sample,
                    text,
                );
                running.pending.push(draft.clone());
                running.sentence_start = None;
                Some(LiveSegment {
                    start_sec: draft.start_sec,
                    end_sec: draft.end_sec,
                    text: draft.text,
                })
            } else {
                None
            }
        } else {
            None
        };
        if let Some(segment) = final_segment {
            self.emit(LiveEvent::Segment { seq: 0, segment });
        }
        let mut running = self.running.take().expect("running session checked above");
        running.gateway_cancel.cancel();
        self.snapshot.connection = ConnectionState::Stopped;
        self.emit(LiveEvent::Connection {
            seq: 0,
            state: ConnectionState::Stopped,
        });

        if let Some(recording) = running.recording.as_mut() {
            recording.begin_shutdown();
        }
        let capture = self.capture.clone();
        let _ = tokio::task::spawn_blocking(move || capture.stop_capture()).await;

        let recording_result = if let Some(recording) = running.recording.take() {
            match tokio::task::spawn_blocking(move || recording.stop()).await {
                Ok(result) => result,
                Err(err) => Err(AppError::new(Code::Storage, err.to_string())),
            }
        } else {
            Ok(())
        };
        match tokio::time::timeout(OLD_GENERATION_DRAIN, &mut running.gateway_task).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => {}
            Err(_) => running.gateway_task.abort(),
        }
        let final_error = writer_error.or_else(|| recording_result.err());
        if let Some(error) = &final_error {
            self.emit(LiveEvent::Error {
                seq: 0,
                error: error.clone(),
            });
        }

        let sample_clock = self.capture.sample_clock();
        let duration_sec = relative_seconds(sample_clock, running.baseline_sample);
        let db = self.db.clone();
        let session_id = running.session_id;
        let transcript_id = running.transcript_id;
        let pending = std::mem::take(&mut running.pending);
        let batch = pending.clone();
        let final_result = tokio::task::spawn_blocking(move || {
            store::append_live_batch(
                &db,
                session_id,
                transcript_id,
                &batch,
                duration_sec,
                "finalizing",
            )
        })
        .await;
        let final_result = match final_result {
            Ok(result) => result,
            Err(err) => Err(AppError::new(Code::Storage, err.to_string())),
        };
        if let Err(error) = final_result {
            self.emit(LiveEvent::Error {
                seq: 0,
                error: error.clone(),
            });
            self.snapshot = LiveSnapshot {
                session_id: Some(session_id),
                transcript_id: Some(transcript_id),
                recording: RecordingState::Failed,
                connection: ConnectionState::Stopped,
                duration_sec,
            };
            self.emit(LiveEvent::Recording {
                seq: 0,
                state: RecordingState::Failed,
            });
            self.emit(LiveEvent::Done { seq: 0 });
            return Err(error);
        }
        let _ = pending;
        let recording_state = if final_error.is_some() {
            RecordingState::Failed
        } else {
            RecordingState::Stopped
        };
        self.snapshot = LiveSnapshot {
            session_id: Some(session_id),
            transcript_id: Some(transcript_id),
            recording: recording_state,
            connection: ConnectionState::Stopped,
            duration_sec,
        };
        self.emit(LiveEvent::Recording {
            seq: 0,
            state: recording_state,
        });
        self.emit(LiveEvent::Done { seq: 0 });
        self.emit(LiveEvent::Final {
            seq: 0,
            session_id,
            transcript_id,
            duration_sec,
        });
        self.emit(LiveEvent::Log {
            seq: 0,
            message: "Live session stopped".to_owned(),
        });
        match final_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn snapshot_for_running(&self) -> LiveSnapshot {
        self.running.as_ref().map_or_else(
            || self.snapshot.clone(),
            |running| LiveSnapshot {
                session_id: Some(running.session_id),
                transcript_id: Some(running.transcript_id),
                recording: running.recording_state,
                connection: running.connection,
                duration_sec: relative_seconds(
                    self.capture.sample_clock(),
                    running.baseline_sample,
                ),
            },
        )
    }

    fn register_subscriber(&mut self, channel: Channel<LiveEvent>) {
        let ready = LiveEvent::Ready {
            seq: self.seq,
            snapshot: self.snapshot.clone(),
        };
        if channel.send(ready).is_ok() {
            self.subscribers.push(channel);
        }
    }

    fn emit(&mut self, event: LiveEvent) {
        self.seq = self.seq.saturating_add(1);
        let event = with_seq(event, self.seq);
        self.subscribers
            .retain(|channel| channel.send(event.clone()).is_ok());
    }
}

fn with_seq(event: LiveEvent, seq: u64) -> LiveEvent {
    match event {
        LiveEvent::Ready { snapshot, .. } => LiveEvent::Ready { seq, snapshot },
        LiveEvent::Delta { text, .. } => LiveEvent::Delta { seq, text },
        LiveEvent::Turn { .. } => LiveEvent::Turn { seq },
        LiveEvent::Segment { segment, .. } => LiveEvent::Segment { seq, segment },
        LiveEvent::Gap {
            start_sec,
            end_sec,
            reason,
            ..
        } => LiveEvent::Gap {
            seq,
            start_sec,
            end_sec,
            reason,
        },
        LiveEvent::Recording { state, .. } => LiveEvent::Recording { seq, state },
        LiveEvent::Connection { state, .. } => LiveEvent::Connection { seq, state },
        LiveEvent::Log { message, .. } => LiveEvent::Log { seq, message },
        LiveEvent::Error { error, .. } => LiveEvent::Error { seq, error },
        LiveEvent::Done { .. } => LiveEvent::Done { seq },
        LiveEvent::Final {
            session_id,
            transcript_id,
            duration_sec,
            ..
        } => LiveEvent::Final {
            seq,
            session_id,
            transcript_id,
            duration_sec,
        },
    }
}

fn map_connection(state: GatewayConnectionState) -> ConnectionState {
    match state {
        GatewayConnectionState::Connecting => ConnectionState::Connecting,
        GatewayConnectionState::Connected => ConnectionState::Connected,
        GatewayConnectionState::Reconnecting => ConnectionState::Reconnecting {
            since_ms: epoch_ms(),
        },
        GatewayConnectionState::Stopped => ConnectionState::Stopped,
    }
}

fn transition_connection(
    current: ConnectionState,
    gateway: GatewayConnectionState,
    now_ms: i64,
) -> ConnectionState {
    match gateway {
        GatewayConnectionState::Reconnecting => match current {
            ConnectionState::Reconnecting { since_ms } => {
                ConnectionState::Reconnecting { since_ms }
            }
            _ => ConnectionState::Reconnecting { since_ms: now_ms },
        },
        other => map_connection(other),
    }
}

fn epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn relative_seconds(sample: u64, baseline_sample: u64) -> f64 {
    sample.saturating_sub(baseline_sample) as f64 / f64::from(OUTPUT_SAMPLE_RATE)
}

fn text_draft(
    start_sample: u64,
    end_sample: u64,
    baseline_sample: u64,
    text: String,
) -> SegmentDraft {
    SegmentDraft {
        start_sec: relative_seconds(start_sample, baseline_sample),
        end_sec: relative_seconds(end_sample.max(start_sample), baseline_sample),
        kind: SegmentKind::Text,
        gap_reason: None,
        text: text.trim().to_owned(),
        speaker: None,
    }
}

fn split_complete_sentences(running: &mut RunningSession) -> Vec<LiveSegment> {
    let split_points: Vec<(usize, usize)> = running
        .sentence_buffer
        .char_indices()
        .enumerate()
        .filter_map(|(char_idx, (byte_idx, ch))| {
            matches!(ch, '.' | '!' | '?' | '。' | '！' | '？' | '\n')
                .then_some((byte_idx + ch.len_utf8(), char_idx + 1))
        })
        .collect();
    if split_points.is_empty() {
        return Vec::new();
    }
    let buffer = std::mem::take(&mut running.sentence_buffer);
    let total_chars = buffer.chars().count().max(1) as u64;
    let sample_start = running.sentence_start.unwrap_or(running.sentence_end);
    let sample_end = running.sentence_end.max(sample_start);
    let sample_span = sample_end.saturating_sub(sample_start);
    let mut consumed_byte = 0;
    let mut consumed_chars = 0_u64;
    let mut previous_end = sample_start;
    let mut completed = Vec::new();
    for (byte_end, char_end) in split_points {
        let sentence = buffer[consumed_byte..byte_end].trim();
        let sentence_end = sample_start.saturating_add(
            ((sample_span as u128 * char_end as u128) / total_chars as u128) as u64,
        );
        if !sentence.is_empty() {
            let draft = text_draft(
                previous_end,
                sentence_end.max(previous_end),
                running.baseline_sample,
                sentence.to_owned(),
            );
            completed.push(LiveSegment {
                start_sec: draft.start_sec,
                end_sec: draft.end_sec,
                text: draft.text.clone(),
            });
            running.pending.push(draft);
        }
        previous_end = sentence_end.max(previous_end);
        consumed_byte = byte_end;
        consumed_chars = char_end as u64;
    }
    running.sentence_buffer = buffer[consumed_byte..].to_owned();
    if running.sentence_buffer.trim().is_empty() {
        running.sentence_buffer.clear();
        running.sentence_start = None;
    } else {
        let tail_start = sample_start.saturating_add(
            ((sample_span as u128 * consumed_chars as u128) / total_chars as u128) as u64,
        );
        running.sentence_start = Some(tail_start.max(previous_end));
    }
    completed
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
    use crate::gemini::keys::{KeyProvider, SystemClock};
    use crate::gemini::live::{ConnectFuture, LiveSocketConnector};
    use crate::secrets::{KeyId, KeyMaterial};
    use std::sync::Mutex;
    use tauri::ipc::InvokeResponseBody;
    use tempfile::TempDir;

    struct EmptyProvider;

    impl KeyProvider for EmptyProvider {
        fn load_keys(&self) -> Result<Vec<KeyMaterial>, AppError> {
            Ok(Vec::new())
        }
    }

    struct OneKeyProvider;

    impl KeyProvider for OneKeyProvider {
        fn load_keys(&self) -> Result<Vec<KeyMaterial>, AppError> {
            Ok(vec![KeyMaterial::new(
                KeyId::from_opaque("live-start-test"),
                "AIzaSyLIVE_START_TEST_KEY".to_owned(),
            )])
        }
    }

    struct PendingConnector;

    impl LiveSocketConnector for PendingConnector {
        fn connect<'a>(
            &'a self,
            _api_key: &'a crate::core::Sensitive<String>,
        ) -> ConnectFuture<'a> {
            Box::pin(std::future::pending())
        }
    }

    #[derive(Default)]
    struct TestBackend;

    impl CaptureBackend for TestBackend {
        fn live_sources(&self) -> Result<LiveSources, AppError> {
            Ok(LiveSources {
                microphones: vec![LiveMicrophone {
                    source: "mic:test".to_owned(),
                    name: "Test microphone".to_owned(),
                    is_default: true,
                }],
                default_microphone: Some("mic:test".to_owned()),
                system_available: true,
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
    }

    impl PreparedStream for TestStream {
        fn start(&mut self) -> Result<(), AppError> {
            (self.on_audio)(InputBlock {
                generation: self.generation,
                side: self.format.side,
                sample_rate: self.format.sample_rate,
                channels: self.format.channels,
                samples: vec![0.25; OUTPUT_CHUNK_SAMPLES],
            });
            Ok(())
        }
    }

    struct Fixture {
        _root: TempDir,
        db: Arc<Db>,
        capture: Arc<CaptureController>,
        actor: LiveSessionActor,
    }

    fn fixture() -> Fixture {
        let root = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(root.path()).unwrap());
        let capture = Arc::new(CaptureController::new(Arc::new(TestBackend)));
        let (key_pool, _key_actor) =
            KeyPoolHandle::channel(Arc::new(EmptyProvider), Arc::new(SystemClock));
        let gateway = LiveGateway::production(key_pool.clone());
        let (_, actor) = channel(
            capture.clone(),
            db.clone(),
            root.path().to_path_buf(),
            key_pool,
            gateway,
        );
        Fixture {
            _root: root,
            db,
            capture,
            actor,
        }
    }

    fn collect_channel(events: Arc<Mutex<Vec<LiveEvent>>>) -> Channel<LiveEvent> {
        Channel::new(move |body| {
            let event = match body {
                InvokeResponseBody::Json(json) => serde_json::from_str(&json).unwrap(),
                InvokeResponseBody::Raw(_) => panic!("Live events must serialize as JSON"),
            };
            events.lock().unwrap().push(event);
            Ok(())
        })
    }

    fn add_recording(fixture: &mut Fixture, generation: u64) -> (SessionId, TranscriptId) {
        fixture.capture.set_source("mic:test").unwrap();
        let recording = recording::start_with_locale(
            fixture.capture.clone(),
            &fixture.db,
            fixture._root.path(),
            UiLanguage::En,
            None,
        )
        .unwrap();
        let session_id = recording.session_id();
        let transcript_id =
            store::create_live_transcript(&fixture.db, session_id, "live", None).unwrap();
        let baseline_sample = fixture.capture.sample_clock();
        let terminal_errors = recording.terminal_errors();
        let gateway_cancel = CancellationToken::new();
        let task_cancel = gateway_cancel.clone();
        let gateway_task = tokio::spawn(async move {
            task_cancel.cancelled().await;
        });
        fixture.actor.running = Some(RunningSession {
            generation,
            session_id,
            transcript_id,
            baseline_sample,
            last_flush_sample: baseline_sample,
            pending: Vec::new(),
            sentence_buffer: String::new(),
            sentence_start: None,
            sentence_end: baseline_sample,
            recording: Some(recording),
            terminal_errors,
            gateway_cancel,
            gateway_task,
            connection: ConnectionState::Connecting,
            recording_state: RecordingState::Active,
            storage_error_reported: false,
        });
        fixture.actor.next_generation = generation.saturating_add(1);
        fixture.actor.snapshot = fixture.actor.snapshot_for_running();
        (session_id, transcript_id)
    }

    #[test]
    fn reconnecting_state_serializes_since_ms_in_camel_case() {
        let state = ConnectionState::Reconnecting {
            since_ms: 1_735_555_123_456,
        };
        assert_eq!(
            serde_json::to_value(state).unwrap(),
            serde_json::json!({ "type": "reconnecting", "sinceMs": 1_735_555_123_456_i64 })
        );
        assert_eq!(
            serde_json::to_value(LiveEvent::Connection { seq: 9, state }).unwrap(),
            serde_json::json!({
                "type": "connection",
                "seq": 9,
                "state": { "type": "reconnecting", "sinceMs": 1_735_555_123_456_i64 }
            })
        );
    }

    #[test]
    fn periodic_flush_threshold_leaves_room_for_one_actor_poll() {
        assert_eq!(relative_seconds(FLUSH_SAMPLES, 0), 4.6);
        assert_eq!(
            relative_seconds(
                FLUSH_SAMPLES + (OUTPUT_SAMPLE_RATE as u64 / 10) + (OUTPUT_SAMPLE_RATE as u64 / 4),
                0,
            ),
            4.95,
            "one output chunk plus one maximum poll after threshold stays below five seconds"
        );
    }

    #[test]
    fn reconnect_start_is_preserved_until_connected_then_reset() {
        let first = transition_connection(
            ConnectionState::Connected,
            GatewayConnectionState::Reconnecting,
            100,
        );
        assert_eq!(first, ConnectionState::Reconnecting { since_ms: 100 });
        assert_eq!(
            transition_connection(first, GatewayConnectionState::Reconnecting, 200),
            first
        );
        assert_eq!(
            transition_connection(first, GatewayConnectionState::Connected, 200),
            ConnectionState::Connected
        );
        assert_eq!(
            transition_connection(
                ConnectionState::Connected,
                GatewayConnectionState::Reconnecting,
                300,
            ),
            ConnectionState::Reconnecting { since_ms: 300 }
        );
    }

    #[test]
    fn remount_gets_atomic_snapshot_cursor_then_next_event_sequence() {
        let mut fixture = fixture();
        fixture.actor.seq = 14;
        fixture.actor.snapshot.connection = ConnectionState::Reconnecting { since_ms: 1234 };
        let events = Arc::new(Mutex::new(Vec::new()));
        fixture
            .actor
            .register_subscriber(collect_channel(events.clone()));
        fixture.actor.emit(LiveEvent::Delta {
            seq: 0,
            text: "next".to_owned(),
        });

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            LiveEvent::Ready { seq: 14, snapshot }
                if snapshot.connection == ConnectionState::Reconnecting { since_ms: 1234 }
        ));
        assert!(matches!(&events[1], LiveEvent::Delta { seq: 15, text } if text == "next"));
    }

    #[test]
    fn failed_subscriber_is_dropped_without_interrupting_healthy_stream() {
        let mut fixture = fixture();
        let failed: Channel<LiveEvent> =
            Channel::new(|_| Err(std::io::Error::other("subscriber closed").into()));
        fixture.actor.register_subscriber(failed);
        assert!(fixture.actor.subscribers.is_empty());

        let events = Arc::new(Mutex::new(Vec::new()));
        fixture
            .actor
            .register_subscriber(collect_channel(events.clone()));
        fixture.actor.emit(LiveEvent::Turn { seq: 0 });
        assert_eq!(fixture.actor.subscribers.len(), 1);
        assert!(matches!(
            events.lock().unwrap().as_slice(),
            [LiveEvent::Ready { seq: 0, .. }, LiveEvent::Turn { seq: 1 }]
        ));
    }

    #[tokio::test]
    async fn multiple_complete_sentences_get_monotonic_sample_ranges() {
        let baseline_sample = 80_000;
        let (terminal_errors, terminal_rx) = watch::channel(None);
        drop(terminal_errors);
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let gateway_task = tokio::spawn(async move {
            task_cancel.cancelled().await;
        });
        let mut running = RunningSession {
            generation: 1,
            session_id: SessionId::new(),
            transcript_id: TranscriptId::new(),
            baseline_sample,
            last_flush_sample: baseline_sample,
            pending: Vec::new(),
            sentence_buffer: "First. Second.".to_owned(),
            sentence_start: Some(baseline_sample),
            sentence_end: baseline_sample + 3_200,
            recording: None,
            terminal_errors: terminal_rx,
            gateway_cancel: cancel,
            gateway_task,
            connection: ConnectionState::Connected,
            recording_state: RecordingState::Active,
            storage_error_reported: false,
        };

        let completed = split_complete_sentences(&mut running);
        assert_eq!(completed.len(), 2);
        assert_eq!(completed[0].text, "First.");
        assert_eq!(completed[1].text, "Second.");
        assert_eq!(running.pending.len(), 2);
        assert!(running.pending[0].end_sec > running.pending[0].start_sec);
        assert!(running.pending[1].end_sec > running.pending[1].start_sec);
        assert_eq!(running.pending[0].start_sec, 0.0);
        assert!(running.pending[1].start_sec >= running.pending[0].end_sec);
        assert_eq!(running.pending[1].end_sec, 0.2);
        assert!(running.sentence_buffer.is_empty());
        running.gateway_cancel.cancel();
    }

    #[tokio::test]
    async fn sentence_segment_is_emitted_immediately_then_persisted_on_flush() {
        let mut fixture = fixture();
        let (session_id, transcript_id) = add_recording(&mut fixture, 1);
        let baseline = fixture.actor.running.as_ref().unwrap().baseline_sample;
        let events = Arc::new(Mutex::new(Vec::new()));
        fixture
            .actor
            .register_subscriber(collect_channel(events.clone()));

        fixture
            .actor
            .handle_internal(Internal::GatewayEvent {
                generation: 1,
                event: GatewayEvent::InputTranscription {
                    text: crate::core::sensitive::Sensitive::new("Hello.".to_owned()),
                    sample_start: baseline + 800,
                    sample_end: baseline + 1_600,
                },
            })
            .await;

        assert!(events.lock().unwrap().iter().any(|event| matches!(
            event,
            LiveEvent::Segment { segment, .. } if segment.text == "Hello."
        )));
        let persisted_before = fixture
            .db
            .with_connection(|conn| Ok(repo::segments::list_for_transcript(conn, transcript_id)?))
            .unwrap();
        assert!(persisted_before.is_empty());
        assert_eq!(fixture.actor.running.as_ref().unwrap().pending.len(), 1);

        fixture.actor.flush_active("recording").await.unwrap();
        let persisted = fixture
            .db
            .with_connection(|conn| Ok(repo::segments::list_for_transcript(conn, transcript_id)?))
            .unwrap();
        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0].text, "Hello.");
        assert!(persisted[0].start_sec >= 0.0);
        assert!(persisted[0].end_sec > persisted[0].start_sec);
        fixture.actor.finish_current(None).await.unwrap();
        let row = fixture
            .db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(row.status, "finalizing");
    }

    #[tokio::test]
    async fn gateway_audio_gap_is_persisted_as_disconnected_segment_with_sample_range() {
        let mut fixture = fixture();
        let (session_id, transcript_id) = add_recording(&mut fixture, 1);
        let baseline = fixture.actor.running.as_ref().unwrap().baseline_sample;
        let events = Arc::new(Mutex::new(Vec::new()));
        fixture
            .actor
            .register_subscriber(collect_channel(events.clone()));

        fixture
            .actor
            .handle_internal(Internal::GatewayEvent {
                generation: 1,
                event: GatewayEvent::AudioGap {
                    start_sample: baseline + 1_600,
                    end_sample: baseline + 4_800,
                },
            })
            .await;
        assert!(events.lock().unwrap().iter().any(|event| matches!(
            event,
            LiveEvent::Gap { start_sec, end_sec, reason, .. }
                if *start_sec == 0.1 && *end_sec == 0.3 && reason == "disconnected"
        )));

        let running = fixture.actor.running.as_ref().unwrap();
        assert_eq!(running.pending.len(), 1);
        assert_eq!(running.pending[0].kind, SegmentKind::Gap);
        assert_eq!(running.pending[0].gap_reason, Some(GapReason::Disconnected));
        assert_eq!(running.pending[0].start_sec, 0.1);
        assert_eq!(running.pending[0].end_sec, 0.3);
        fixture.actor.flush_active("recording").await.unwrap();

        let rows = fixture
            .db
            .with_connection(|conn| Ok(repo::segments::list_for_transcript(conn, transcript_id)?))
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, SegmentKind::Gap);
        assert_eq!(rows[0].gap_reason, Some(GapReason::Disconnected));
        assert_eq!(rows[0].start_sec, 0.1);
        assert_eq!(rows[0].end_sec, 0.3);
        assert!(rows[0].text.is_empty());
        fixture.actor.finish_current(None).await.unwrap();
        let row = fixture
            .db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(row.status, "finalizing");
    }

    #[tokio::test]
    async fn stale_gateway_generation_cannot_change_replacement_or_stopped_session() {
        let mut fixture = fixture();
        let (_session_id, _transcript_id) = add_recording(&mut fixture, 2);
        let sequence = fixture.actor.seq;
        let pending_before = fixture.actor.running.as_ref().unwrap().pending.len();
        fixture
            .actor
            .handle_internal(Internal::GatewayEvent {
                generation: 1,
                event: GatewayEvent::InputTranscription {
                    text: crate::core::sensitive::Sensitive::new("stale.".to_owned()),
                    sample_start: 0,
                    sample_end: 1_600,
                },
            })
            .await;
        assert_eq!(fixture.actor.seq, sequence);
        assert_eq!(
            fixture.actor.running.as_ref().unwrap().pending.len(),
            pending_before
        );

        fixture.actor.finish_current(None).await.unwrap();
        let stopped_sequence = fixture.actor.seq;
        fixture
            .actor
            .handle_internal(Internal::GatewayEvent {
                generation: 2,
                event: GatewayEvent::ConnectionChanged {
                    state: GatewayConnectionState::Connected,
                },
            })
            .await;
        assert_eq!(fixture.actor.seq, stopped_sequence);
        assert_eq!(fixture.actor.snapshot.connection, ConnectionState::Stopped);
    }

    #[tokio::test]
    async fn gateway_ended_authoritatively_stops_connection_when_final_notice_was_dropped() {
        let mut fixture = fixture();
        let _ = add_recording(&mut fixture, 1);
        fixture.actor.running.as_mut().unwrap().connection = ConnectionState::Connected;
        fixture.actor.snapshot = fixture.actor.snapshot_for_running();

        fixture
            .actor
            .handle_internal(Internal::GatewayEnded {
                generation: 1,
                result: Ok(()),
            })
            .await;

        assert_eq!(
            fixture.actor.running.as_ref().unwrap().connection,
            ConnectionState::Stopped
        );
        assert_eq!(fixture.actor.snapshot.connection, ConnectionState::Stopped);
        fixture.actor.finish_current(None).await.unwrap();
    }

    #[tokio::test]
    async fn database_flush_failure_restores_pending_then_stops_recording_for_recovery() {
        let mut fixture = fixture();
        let (session_id, transcript_id) = add_recording(&mut fixture, 1);
        let baseline = fixture.actor.running.as_ref().unwrap().baseline_sample;
        fixture
            .actor
            .handle_internal(Internal::GatewayEvent {
                generation: 1,
                event: GatewayEvent::InputTranscription {
                    text: crate::core::sensitive::Sensitive::new("Retain me.".to_owned()),
                    sample_start: baseline + 800,
                    sample_end: baseline + 1_600,
                },
            })
            .await;
        fixture
            .db
            .with_connection(|conn| {
                conn.execute_batch("DROP TABLE segments")?;
                Ok(())
            })
            .unwrap();

        let error = fixture.actor.flush_active("recording").await.unwrap_err();
        assert_eq!(error.code, Code::Storage);
        assert_eq!(fixture.actor.running.as_ref().unwrap().pending.len(), 1);
        assert!(fixture.actor.finish_current(None).await.is_err());
        assert!(fixture.actor.running.is_none());
        assert_eq!(fixture.actor.snapshot.recording, RecordingState::Failed);
        assert!(fixture.capture.active_source().is_none());
        assert!(crate::core::paths::recording_path(fixture._root.path(), session_id).is_file());
        assert_eq!(
            fixture
                .db
                .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
                .unwrap()
                .status,
            "recording"
        );
        let _ = transcript_id;
    }

    #[tokio::test]
    async fn no_eligible_key_rejects_start_before_capture_is_opened() {
        let mut fixture = fixture();
        let (key_pool, key_actor) =
            KeyPoolHandle::channel(Arc::new(EmptyProvider), Arc::new(SystemClock));
        tokio::spawn(key_actor.run());
        key_pool.refresh().await.unwrap();
        fixture.actor.key_pool = key_pool.clone();
        fixture.actor.gateway = LiveGateway::production(key_pool);
        let error = fixture
            .actor
            .start(LiveStartParams {
                source: "mic:test".to_owned(),
                language: TranscribeLanguage::Auto,
                locale: None,
                ui_language: UiLanguage::En,
                model: "live-test".to_owned(),
                consent: ConsentSnapshot::new(crate::consent::CURRENT_VERSION, false),
            })
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Auth);
        assert!(fixture.capture.active_source().is_none());
    }

    #[tokio::test]
    async fn actor_start_records_the_first_chunk_emitted_during_capture_open() {
        let mut fixture = fixture();
        let (key_pool, key_actor) =
            KeyPoolHandle::channel(Arc::new(OneKeyProvider), Arc::new(SystemClock));
        tokio::spawn(key_actor.run());
        key_pool.refresh().await.unwrap();
        fixture.actor.key_pool = key_pool.clone();
        fixture.actor.gateway = LiveGateway::new(Arc::new(PendingConnector), key_pool);

        let session_id = fixture
            .actor
            .start(LiveStartParams {
                source: "mic:test".to_owned(),
                language: TranscribeLanguage::Auto,
                locale: None,
                ui_language: UiLanguage::En,
                model: "live-start-test".to_owned(),
                consent: ConsentSnapshot::new(crate::consent::CURRENT_VERSION, false),
            })
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while fixture.capture.sample_clock() < OUTPUT_CHUNK_SAMPLES as u64 {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .expect("capture should publish its first output chunk");
        fixture.actor.finish_current(None).await.unwrap();

        let path = crate::core::paths::recording_path(fixture._root.path(), session_id);
        let mut wav = hound::WavReader::open(path).unwrap();
        assert!(wav.duration() >= OUTPUT_CHUNK_SAMPLES as u32);
        assert!(wav.samples::<i16>().any(|sample| sample.unwrap() != 0));
    }

    #[tokio::test]
    async fn busy_and_stale_consent_starts_are_rejected_before_opening_another_capture() {
        let mut busy = fixture();
        let (session_id, _) = add_recording(&mut busy, 1);
        let error = busy
            .actor
            .start(LiveStartParams {
                source: "mic:test".to_owned(),
                language: TranscribeLanguage::Auto,
                locale: None,
                ui_language: UiLanguage::En,
                model: "live-test".to_owned(),
                consent: ConsentSnapshot::new(crate::consent::CURRENT_VERSION, false),
            })
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Request);
        assert_eq!(busy.actor.running.as_ref().unwrap().session_id, session_id);
        assert_eq!(
            busy.db
                .with_connection(|conn| Ok(repo::sessions::list(conn)?))
                .unwrap()
                .len(),
            1
        );
        busy.actor.finish_current(None).await.unwrap();

        let mut no_consent = fixture();
        let error = no_consent
            .actor
            .start(LiveStartParams {
                source: "mic:test".to_owned(),
                language: TranscribeLanguage::Auto,
                locale: None,
                ui_language: UiLanguage::En,
                model: "live-test".to_owned(),
                consent: ConsentSnapshot::new(crate::consent::CURRENT_VERSION - 1, false),
            })
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Blocked);
        assert!(no_consent.capture.active_source().is_none());
    }

    #[tokio::test]
    async fn old_gateway_task_is_cancelled_and_drained_within_one_second() {
        let mut fixture = fixture();
        let session_id = SessionId::new();
        let transcript_id = TranscriptId::new();
        let (terminal_errors, terminal_rx) = watch::channel(None);
        drop(terminal_errors);
        let cancellation = CancellationToken::new();
        let gateway_task = tokio::spawn(std::future::pending());
        fixture.actor.running = Some(RunningSession {
            generation: 1,
            session_id,
            transcript_id,
            baseline_sample: 0,
            last_flush_sample: 0,
            pending: Vec::new(),
            sentence_buffer: String::new(),
            sentence_start: None,
            sentence_end: 0,
            recording: None,
            terminal_errors: terminal_rx,
            gateway_cancel: cancellation,
            gateway_task,
            connection: ConnectionState::Connecting,
            recording_state: RecordingState::Active,
            storage_error_reported: false,
        });
        let started = std::time::Instant::now();
        let result = fixture.actor.finish_current(None).await;
        assert!(started.elapsed() <= OLD_GENERATION_DRAIN + Duration::from_millis(200));
        assert!(
            result.is_err(),
            "the fixture intentionally has no DB session row"
        );
        assert!(fixture.actor.running.is_none());
    }

    #[tokio::test]
    async fn normal_stop_finishes_recording_without_error_while_gateway_drain_times_out() {
        let mut fixture = fixture();
        let (_session_id, _transcript_id) = add_recording(&mut fixture, 1);
        let terminal_errors = fixture
            .actor
            .running
            .as_ref()
            .unwrap()
            .terminal_errors
            .clone();
        let events = Arc::new(Mutex::new(Vec::new()));
        fixture
            .actor
            .register_subscriber(collect_channel(events.clone()));

        let previous = {
            let running = fixture.actor.running.as_mut().unwrap();
            running.gateway_cancel.cancel();
            std::mem::replace(
                &mut running.gateway_task,
                tokio::spawn(std::future::pending()),
            )
        };
        previous.abort();
        let _ = previous.await;

        let started = std::time::Instant::now();
        tokio::time::timeout(Duration::from_secs(2), fixture.actor.finish_current(None))
            .await
            .expect("gateway drain should be bounded")
            .unwrap();
        assert!(started.elapsed() >= OLD_GENERATION_DRAIN);
        assert_eq!(fixture.actor.snapshot.recording, RecordingState::Stopped);
        assert!(terminal_errors.borrow().is_none());
        assert!(!events
            .lock()
            .unwrap()
            .iter()
            .any(|event| matches!(event, LiveEvent::Error { .. })));
        assert!(events.lock().unwrap().iter().any(|event| matches!(
            event,
            LiveEvent::Recording {
                state: RecordingState::Stopped,
                ..
            }
        )));
        assert!(fixture.capture.active_source().is_none());
    }
}
