//! Process-wide LiveSession actor. It owns one live generation, its durable
//! transcript cursor and the seq stream shared by all IPC subscribers.

pub mod recording;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::ipc::Channel;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::audio::{CaptureController, OUTPUT_SAMPLE_RATE};
use crate::core::error::{AppError, Category, Code};
use crate::core::id::{SessionId, TagId, TranscriptId};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TranscriptionState {
    Active,
    SetupRejected,
    RecordingOnly,
    Stopped,
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
    pub transcription: TranscriptionState,
    pub error_category: Option<Category>,
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
    Transcription {
        #[specta(type = specta_typescript::Number)]
        seq: u64,
        state: TranscriptionState,
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
    pub tag_ids: Vec<TagId>,
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

    pub async fn stop(&self) -> Result<SessionId, AppError> {
        self.stop_inner(false).await
    }

    /// Stops capture and commits the durable Live data without waiting for a
    /// derived Proxy. Used by the application close coordinator.
    pub async fn stop_for_close(&self) -> Result<SessionId, AppError> {
        self.stop_inner(true).await
    }

    async fn stop_inner(&self, for_close: bool) -> Result<SessionId, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Stop { for_close, reply })
            .await
            .map_err(|_| actor_error())?;
        response.await.map_err(|_| actor_error())?
    }

    pub async fn is_running(&self) -> Result<bool, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::IsRunning { reply })
            .await
            .map_err(|_| actor_error())?;
        response.await.map_err(|_| actor_error())
    }

    /// Swaps the capture source of the running session. Serialized with
    /// start/stop by the actor; rejected when no session is running so that
    /// no capture is ever opened outside a session.
    pub async fn set_source(&self, source: String) -> Result<(), AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::SetSource { source, reply })
            .await
            .map_err(|_| actor_error())?;
        response.await.map_err(|_| actor_error())?
    }

    pub async fn continue_recording_only(&self) -> Result<(), AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::ContinueRecordingOnly { reply })
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

    /// Drops the subscriber registered with this Channel id (a no-op when it
    /// is already gone).
    pub async fn unsubscribe(&self, channel_id: u32) -> Result<(), AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Unsubscribe { channel_id, reply })
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
        for_close: bool,
        reply: oneshot::Sender<Result<SessionId, AppError>>,
    },
    IsRunning {
        reply: oneshot::Sender<bool>,
    },
    SetSource {
        source: String,
        reply: oneshot::Sender<Result<(), AppError>>,
    },
    ContinueRecordingOnly {
        reply: oneshot::Sender<Result<(), AppError>>,
    },
    Subscribe {
        channel: Channel<LiveEvent>,
        reply: oneshot::Sender<()>,
    },
    Unsubscribe {
        channel_id: u32,
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
    transcription: TranscriptionState,
    transcript_stopped_sample: Option<u64>,
    error_category: Option<Category>,
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
    pending_close_finalize: Option<(SessionId, f64)>,
    close_failed: bool,
    /// Sessions whose derived Proxy is being encoded off-actor (shared with
    /// boot recovery). Delete and rerun treat them as busy.
    recovering: Arc<Mutex<HashSet<SessionId>>>,
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
                transcription: TranscriptionState::Stopped,
                error_category: None,
                duration_sec: 0.0,
            },
            running: None,
            pending_close_finalize: None,
            close_failed: false,
            recovering: Arc::new(Mutex::new(HashSet::new())),
        },
    )
}

impl LiveSessionActor {
    /// Shares the app-wide busy set so a detached Proxy encode is visible to
    /// delete/rerun guards.
    pub fn with_recovering(mut self, recovering: Arc<Mutex<HashSet<SessionId>>>) -> Self {
        self.recovering = recovering;
        self
    }

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
                    Some(Command::Stop { for_close, reply }) => {
                        let result = self.finish_current(None, for_close).await;
                        let _ = reply.send(result);
                    }
                    Some(Command::IsRunning { reply }) => {
                        let _ = reply.send(
                            self.running.is_some()
                                || self.pending_close_finalize.is_some()
                                || self.close_failed,
                        );
                    }
                    Some(Command::SetSource { source, reply }) => {
                        let result = self.set_source(source).await;
                        let _ = reply.send(result);
                    }
                    Some(Command::ContinueRecordingOnly { reply }) => {
                        let result = self.continue_recording_only().await;
                        let _ = reply.send(result);
                    }
                    Some(Command::Subscribe { channel, reply }) => {
                        self.register_subscriber(channel);
                        let _ = reply.send(());
                    }
                    Some(Command::Unsubscribe { channel_id, reply }) => {
                        self.unregister_subscriber(channel_id);
                        let _ = reply.send(());
                    }
                    None => {
                        let _ = self.finish_current(None, false).await;
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
        tracing::debug!("Live start requested");
        if self.running.is_some() {
            return Err(AppError::new(
                Code::Request,
                "A Live session is already running",
            ));
        }
        if self.pending_close_finalize.is_some() || self.close_failed {
            return Err(AppError::new(
                Code::Request,
                "The previous Live session has not been saved yet",
            ));
        }
        if !params.consent.is_current() {
            tracing::warn!("Live start refused: Gemini consent is not current");
            return Err(AppError::new(
                Code::Blocked,
                "Gemini Live requires current consent",
            ));
        }
        if !self.key_pool.has_eligible_key().await? {
            tracing::warn!("Live start refused: no eligible Gemini credential");
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
        let tag_ids = params.tag_ids.clone();
        let recording_result = tokio::task::spawn_blocking(move || {
            recording::start_with_receiver_and_tags(
                capture,
                recording_audio,
                &db,
                &data_dir,
                ui_language,
                locale.as_deref(),
                &tag_ids,
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
                // Nothing was transcribed yet: stop both workers, then remove
                // the just-created row and its media in-process so no phantom
                // `recording` session outlives this failed start. If removal
                // fails too, boot recovery still owns the leftovers.
                let mut recording = recording;
                recording.begin_shutdown();
                let capture = self.capture.clone();
                let _ = tokio::task::spawn_blocking(move || capture.stop_capture()).await;
                let _ = tokio::task::spawn_blocking(move || recording.stop()).await;
                let db = self.db.clone();
                let data_dir = self.data_dir.clone();
                let cleanup = tokio::task::spawn_blocking(move || {
                    store::delete_session(&db, &data_dir, session_id)
                })
                .await;
                if !matches!(cleanup, Ok(Ok(()))) {
                    tracing::warn!(session_id = %session_id, "could not remove the Live session of a failed start; boot recovery will handle it");
                }
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
            transcription: TranscriptionState::Active,
            transcript_stopped_sample: None,
            error_category: None,
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
        self.apply_internal(internal, true).await;
    }

    /// `allow_finish = false` is used while `finish_current` drains queued
    /// events: a storage failure there is handled by the final flush itself.
    async fn apply_internal(&mut self, internal: Internal, allow_finish: bool) {
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
                                running.sentence_start = None;
                                if has_content(&text) {
                                    let draft =
                                        text_draft(start, end, running.baseline_sample, text);
                                    running.pending.push(draft.clone());
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
                            }
                        } else {
                            None
                        };
                        if let Some(segment) = completed {
                            self.emit(LiveEvent::Segment { seq: 0, segment });
                        }
                        self.emit(LiveEvent::Turn { seq: 0 });
                        if self.flush_active("recording").await.is_err() && allow_finish {
                            // Persistent transcript storage is terminal for
                            // this Live session: stop capture and keep the
                            // prior DB checkpoints plus durable WAV bounded.
                            let _ = Box::pin(self.finish_current(None, false)).await;
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
                match &result {
                    Ok(()) => tracing::info!("Live gateway ended normally"),
                    Err(crate::gemini::live::LiveFailure::Cancelled) => {
                        tracing::debug!("Live gateway cancelled")
                    }
                    Err(crate::gemini::live::LiveFailure::Gateway(error)) => {
                        tracing::warn!(error = %error, "Live gateway failed")
                    }
                    Err(crate::gemini::live::LiveFailure::SetupRejected) => {
                        tracing::warn!("Live gateway setup rejected by Gemini")
                    }
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
                            if let Some(running) = self.running.as_mut() {
                                running.transcription = TranscriptionState::Stopped;
                                running.transcript_stopped_sample =
                                    Some(self.capture.sample_clock());
                                running.error_category = Some(error.category);
                            }
                            self.snapshot = self.snapshot_for_running();
                            self.emit(LiveEvent::Transcription {
                                seq: 0,
                                state: TranscriptionState::Stopped,
                            });
                            self.emit(LiveEvent::Error { seq: 0, error });
                        }
                        crate::gemini::live::LiveFailure::SetupRejected => {
                            if let Some(running) = self.running.as_mut() {
                                running.transcription = TranscriptionState::SetupRejected;
                                running.transcript_stopped_sample =
                                    Some(self.capture.sample_clock());
                            }
                            self.snapshot = self.snapshot_for_running();
                            self.emit(LiveEvent::Transcription {
                                seq: 0,
                                state: TranscriptionState::SetupRejected,
                            });
                        }
                    }
                } else if let Some(running) = self.running.as_mut() {
                    if running.transcription == TranscriptionState::Active {
                        running.transcription = TranscriptionState::Stopped;
                        running.transcript_stopped_sample = Some(self.capture.sample_clock());
                        self.snapshot = self.snapshot_for_running();
                        self.emit(LiveEvent::Transcription {
                            seq: 0,
                            state: TranscriptionState::Stopped,
                        });
                    }
                }
            }
        }
    }

    async fn set_source(&mut self, source: String) -> Result<(), AppError> {
        if self.running.is_none() {
            return Err(AppError::new(
                Code::Request,
                "Audio source can only be changed while a Live session is running",
            ));
        }
        let capture = self.capture.clone();
        tokio::task::spawn_blocking(move || capture.set_source(&source))
            .await
            .map_err(|err| AppError::new(Code::Storage, err.to_string()))?
    }

    async fn continue_recording_only(&mut self) -> Result<(), AppError> {
        let Some(running) = self.running.as_mut() else {
            return Err(AppError::new(
                Code::Request,
                "There is no Live session waiting for a recording-only decision",
            ));
        };
        if running.transcription != TranscriptionState::SetupRejected {
            return Err(AppError::new(
                Code::Request,
                "Live setup has not been rejected five times",
            ));
        }

        // SetupRejected is terminal in the gateway. Cancelling here also
        // closes the race with a still-unwinding gateway task and makes the
        // session-scoped recording-only choice explicit to the actor.
        running.gateway_cancel.cancel();
        running.transcription = TranscriptionState::RecordingOnly;
        self.snapshot = self.snapshot_for_running();
        self.emit(LiveEvent::Transcription {
            seq: 0,
            state: TranscriptionState::RecordingOnly,
        });
        Ok(())
    }

    async fn check_running_session(&mut self) {
        let writer_error = self
            .running
            .as_ref()
            .and_then(|running| running.terminal_errors.borrow().clone());
        if let Some(error) = writer_error {
            let _ = self.finish_current(Some(error), false).await;
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
                let _ = self.finish_current(None, false).await;
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
                    running.error_category = Some(error.category);
                    if !running.storage_error_reported {
                        running.storage_error_reported = true;
                        report = true;
                    }
                }
                if report {
                    self.snapshot = self.snapshot_for_running();
                    self.emit(LiveEvent::Error {
                        seq: 0,
                        error: error.clone(),
                    });
                }
                Err(error)
            }
        }
    }

    async fn finish_current(
        &mut self,
        writer_error: Option<AppError>,
        for_close: bool,
    ) -> Result<SessionId, AppError> {
        if self.running.is_none() {
            if for_close {
                if let Some((session_id, duration_sec)) = self.pending_close_finalize {
                    let db = self.db.clone();
                    let retry = tokio::task::spawn_blocking(move || {
                        store::finalize_live_session_minimal(&db, session_id, duration_sec)
                    })
                    .await
                    .map_err(|error| AppError::new(Code::Storage, error.to_string()))
                    .and_then(|outcome| outcome);
                    if let Err(error) = retry {
                        self.snapshot.recording = RecordingState::Failed;
                        self.snapshot.error_category = Some(error.category);
                        self.emit(LiveEvent::Error {
                            seq: 0,
                            error: error.clone(),
                        });
                        self.emit(LiveEvent::Recording {
                            seq: 0,
                            state: RecordingState::Failed,
                        });
                        return Err(error);
                    }
                    self.pending_close_finalize = None;
                    self.close_failed = false;
                    self.snapshot = LiveSnapshot {
                        session_id: Some(session_id),
                        transcript_id: self.snapshot.transcript_id,
                        recording: RecordingState::Stopped,
                        connection: ConnectionState::Stopped,
                        transcription: TranscriptionState::Stopped,
                        error_category: None,
                        duration_sec,
                    };
                    self.emit(LiveEvent::Recording {
                        seq: 0,
                        state: RecordingState::Stopped,
                    });
                    self.emit(LiveEvent::Done { seq: 0 });
                    self.emit(LiveEvent::Final {
                        seq: 0,
                        session_id,
                        transcript_id: self.snapshot.transcript_id.unwrap_or_default(),
                        duration_sec,
                    });
                    return Ok(session_id);
                }
            }
            if self.close_failed {
                return Err(AppError::new(
                    Code::Storage,
                    "Live storage failed; the app must stay open for data recovery",
                ));
            }
            return Err(AppError::new(
                Code::Request,
                "There is no Live session to stop",
            ));
        }
        // Deltas already queued by the gateway must be applied before the
        // final flush, otherwise Stop would silently drop the last sentences.
        while let Ok(internal) = self.internal_receiver.try_recv() {
            self.apply_internal(internal, false).await;
        }
        let final_segment = if let Some(running) = self.running.as_mut() {
            if !running.sentence_buffer.trim().is_empty() {
                let start = running.sentence_start.unwrap_or(running.sentence_end);
                let text = std::mem::take(&mut running.sentence_buffer);
                running.sentence_start = None;
                if has_content(&text) {
                    let draft = text_draft(
                        start,
                        running.sentence_end.max(start),
                        running.baseline_sample,
                        text,
                    );
                    running.pending.push(draft.clone());
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

        // A terminal transcription failure leaves WAV capture alive. Close
        // its missing transcript range on the sample clock when capture
        // actually stops, then persist it with the final transcript batch.
        let stop_sample = self.capture.sample_clock();
        if let Some(start_sample) = running.transcript_stopped_sample {
            if let Some(gap) = tail_gap_draft(start_sample, stop_sample, running.baseline_sample) {
                let start_sec = gap.start_sec;
                let end_sec = gap.end_sec;
                running.pending.push(gap);
                self.emit(LiveEvent::Gap {
                    seq: 0,
                    start_sec,
                    end_sec,
                    reason: "disconnected".to_owned(),
                });
            }
        }

        let sample_clock = self.capture.sample_clock();
        let duration_sec = relative_seconds(sample_clock, running.baseline_sample);
        let db = self.db.clone();
        let session_id = running.session_id;
        let transcript_id = running.transcript_id;
        let batch = std::mem::take(&mut running.pending);
        let retry_batch = batch.clone();
        let final_flush = tokio::task::spawn_blocking(move || {
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
        let final_flush = match final_flush {
            Ok(result) => result,
            Err(err) => Err(AppError::new(Code::Storage, err.to_string())),
        };

        // Persist the final transcript batch and enter `finalizing` before
        // closing the WAV writer. Even when that DB flush fails, still join
        // the writer below so its checkpointed data is left in a recoverable
        // state on disk.
        let recording_result = if let Some(recording) = running.recording.take() {
            match tokio::task::spawn_blocking(move || recording.stop()).await {
                Ok(outcome) => outcome,
                Err(err) => recording::RecordingStopOutcome {
                    wav_finalized: false,
                    error: Some(AppError::new(Code::Storage, err.to_string())),
                    written_samples: 0,
                },
            }
        } else {
            recording::RecordingStopOutcome {
                wav_finalized: true,
                error: None,
                written_samples: 0,
            }
        };
        match tokio::time::timeout(OLD_GENERATION_DRAIN, &mut running.gateway_task).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => {}
            Err(_) => running.gateway_task.abort(),
        }

        // The WAV length is the authoritative duration; the sample clock is
        // only a fallback when the writer reported nothing.
        let duration_sec =
            duration_from_written_samples(recording_result.written_samples).unwrap_or(duration_sec);

        if let Err(error) = final_flush {
            if recording_result.wav_finalized {
                // Keep the session so Stop can be retried with the same batch.
                running.pending = retry_batch;
                running.recording = None;
                running.recording_state = RecordingState::Failed;
                running.error_category = Some(error.category);
                // The old gateway task is already joined or aborted; a fresh
                // finished task keeps the retry from polling it twice.
                running.gateway_task = tokio::spawn(async {});
                self.running = Some(running);
            } else if for_close {
                self.close_failed = true;
            }
            self.emit(LiveEvent::Error {
                seq: 0,
                error: error.clone(),
            });
            self.snapshot = LiveSnapshot {
                session_id: Some(session_id),
                transcript_id: Some(transcript_id),
                recording: RecordingState::Failed,
                connection: ConnectionState::Stopped,
                transcription: TranscriptionState::Stopped,
                error_category: Some(error.category),
                duration_sec,
            };
            self.emit(LiveEvent::Recording {
                seq: 0,
                state: RecordingState::Failed,
            });
            return Err(error);
        }

        let terminal_error = writer_error.or(recording_result.error.clone());
        if !recording_result.wav_finalized {
            if for_close {
                self.close_failed = true;
            }
            let error = terminal_error.unwrap_or_else(|| {
                AppError::new(Code::Storage, "Live Recording WAV could not be finalized")
            });
            self.emit(LiveEvent::Error {
                seq: 0,
                error: error.clone(),
            });
            self.snapshot = LiveSnapshot {
                session_id: Some(session_id),
                transcript_id: Some(transcript_id),
                recording: RecordingState::Failed,
                connection: ConnectionState::Stopped,
                transcription: TranscriptionState::Stopped,
                error_category: Some(error.category),
                duration_sec,
            };
            self.emit(LiveEvent::Recording {
                seq: 0,
                state: RecordingState::Failed,
            });
            return Err(error);
        }

        if let Some(error) = terminal_error.as_ref() {
            self.emit(LiveEvent::Error {
                seq: 0,
                error: error.clone(),
            });
        }

        let db = self.db.clone();
        // Commit `complete` first: Stop must not wait for a Proxy encode.
        let outcome = tokio::task::spawn_blocking(move || {
            store::finalize_live_session_minimal(&db, session_id, duration_sec)
        })
        .await;
        let outcome = match outcome {
            Ok(result) => result,
            Err(err) => Err(AppError::new(Code::Storage, err.to_string())),
        };
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                if for_close {
                    self.pending_close_finalize = Some((session_id, duration_sec));
                }
                self.emit(LiveEvent::Error {
                    seq: 0,
                    error: error.clone(),
                });
                self.snapshot = LiveSnapshot {
                    session_id: Some(session_id),
                    transcript_id: Some(transcript_id),
                    recording: RecordingState::Failed,
                    connection: ConnectionState::Stopped,
                    transcription: TranscriptionState::Stopped,
                    error_category: Some(error.category),
                    duration_sec,
                };
                self.emit(LiveEvent::Recording {
                    seq: 0,
                    state: RecordingState::Failed,
                });
                return Err(error);
            }
        };

        if let Some(error) = outcome.proxy_error.as_ref() {
            self.emit(LiveEvent::Error {
                seq: 0,
                error: error.clone(),
            });
        }
        if !for_close {
            self.spawn_proxy_derivation(session_id);
        }
        self.snapshot = LiveSnapshot {
            session_id: Some(session_id),
            transcript_id: Some(transcript_id),
            recording: RecordingState::Stopped,
            connection: ConnectionState::Stopped,
            transcription: TranscriptionState::Stopped,
            error_category: terminal_error
                .as_ref()
                .map(|error| error.category)
                .or_else(|| outcome.proxy_error.as_ref().map(|error| error.category))
                .or(running.error_category),
            duration_sec,
        };
        self.emit(LiveEvent::Recording {
            seq: 0,
            state: RecordingState::Stopped,
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
        Ok(outcome.session_id)
    }

    /// Encodes the Proxy of a just-completed session off the actor. The
    /// session is claimed in the shared `recovering` set first, so delete and
    /// rerun see it as busy; a failure leaves the failed-repair marker and the
    /// WAV/transcript untouched.
    fn spawn_proxy_derivation(&self, session_id: SessionId) {
        let Some(claim) = store::ClaimGuard::try_claim(&self.recovering, session_id) else {
            return;
        };
        let db = self.db.clone();
        let root = self.data_dir.clone();
        tokio::task::spawn_blocking(move || {
            match store::derive_live_proxy(&db, &root, session_id, &claim) {
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(session_id = %session_id, error = %error, "Live Proxy could not be derived after Stop");
                }
            }
            drop(claim);
        });
    }

    fn snapshot_for_running(&self) -> LiveSnapshot {
        self.running.as_ref().map_or_else(
            || self.snapshot.clone(),
            |running| LiveSnapshot {
                session_id: Some(running.session_id),
                transcript_id: Some(running.transcript_id),
                recording: running.recording_state,
                connection: running.connection,
                transcription: running.transcription,
                error_category: running.error_category,
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

    fn unregister_subscriber(&mut self, channel_id: u32) {
        self.subscribers
            .retain(|channel| channel.id() != channel_id);
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
        LiveEvent::Transcription { state, .. } => LiveEvent::Transcription { seq, state },
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

fn duration_from_written_samples(written_samples: u64) -> Option<f64> {
    (written_samples > 0).then(|| written_samples as f64 / f64::from(OUTPUT_SAMPLE_RATE))
}

/// A segment worth persisting contains at least one letter or digit; a piece
/// of pure punctuation (for example a stray `.` between sentences) is dropped.
fn has_content(text: &str) -> bool {
    text.chars().any(char::is_alphanumeric)
}

fn tail_gap_draft(
    start_sample: u64,
    end_sample: u64,
    baseline_sample: u64,
) -> Option<SegmentDraft> {
    if end_sample <= start_sample {
        return None;
    }
    Some(SegmentDraft {
        start_sec: relative_seconds(start_sample, baseline_sample),
        end_sec: relative_seconds(end_sample, baseline_sample),
        kind: SegmentKind::Gap,
        gap_reason: Some(GapReason::Disconnected),
        text: String::new(),
        speaker: None,
    })
}

/// Abbreviations that end in a dot without ending the sentence when the next
/// word starts in lowercase.
const ABBREVIATIONS: &[&str] = &[
    "mr", "mrs", "ms", "dr", "prof", "sr", "jr", "vs", "etc", "vol", "fig", "inc", "ltd", "e.g",
    "i.e", "approx", "dept",
];

/// Whether the terminator at `chars[index]` closes a sentence. `chars` is the
/// whole buffered text, so a `.` at the very end is judged without knowing what
/// follows: it splits unless it could be part of a decimal or an abbreviation.
fn is_sentence_end(chars: &[(usize, char)], index: usize) -> bool {
    let ch = chars[index].1;
    if !matches!(ch, '.' | '!' | '?' | '。' | '！' | '？' | '\n') {
        return false;
    }
    if ch != '.' {
        return true;
    }
    let previous = index.checked_sub(1).map(|i| chars[i].1);
    let next = chars.get(index + 1).map(|(_, c)| *c);
    // `...` is an ellipsis, not a sentence end.
    if previous == Some('.') || next == Some('.') {
        return false;
    }
    // Decimal or version number such as `3.14`; a trailing digit-dot waits
    // for the next delta to disambiguate.
    if previous.is_some_and(|c| c.is_ascii_digit()) {
        match next {
            Some(c) if c.is_ascii_digit() => return false,
            None => return false,
            _ => {}
        }
    }
    // Abbreviation followed by a lowercase word (or by nothing yet).
    let word_end = index;
    let mut word_start = word_end;
    while word_start > 0 {
        let c = chars[word_start - 1].1;
        if c.is_alphabetic()
            || (c == '.' && word_start >= 2 && chars[word_start - 2].1.is_alphabetic())
        {
            word_start -= 1;
        } else {
            break;
        }
    }
    let word: String = chars[word_start..word_end]
        .iter()
        .map(|(_, c)| c.to_ascii_lowercase())
        .collect();
    let is_abbreviation = !word.is_empty()
        && (ABBREVIATIONS.contains(&word.as_str())
            || (word.chars().count() == 1 && word.chars().all(|c| c.is_alphabetic())));
    if is_abbreviation {
        let following = chars[index + 1..]
            .iter()
            .map(|(_, c)| *c)
            .find(|c| !c.is_whitespace());
        return match following {
            Some(c) => !c.is_lowercase(),
            None => false,
        };
    }
    true
}

fn split_complete_sentences(running: &mut RunningSession) -> Vec<LiveSegment> {
    let chars: Vec<(usize, char)> = running.sentence_buffer.char_indices().collect();
    let split_points: Vec<(usize, usize)> = chars
        .iter()
        .enumerate()
        .filter(|(char_idx, _)| is_sentence_end(&chars, *char_idx))
        .map(|(char_idx, (byte_idx, ch))| (byte_idx + ch.len_utf8(), char_idx + 1))
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
        if has_content(sentence) {
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
        CaptureBackend, InputBlock, InputCallback, InputErrorCallback, InputFormat, InputSide,
        LiveMicrophone, LiveSources, PreparedInput, PreparedSourceSet, PreparedStream, SourceInput,
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
    struct TestBackend {
        callbacks: Mutex<Vec<(u64, InputSide, InputCallback)>>,
    }

    impl TestBackend {
        fn emit_chunk(&self) {
            let callback = self.callbacks.lock().unwrap().last().cloned();
            if let Some((generation, side, callback)) = callback {
                callback(InputBlock {
                    generation,
                    side,
                    sample_rate: OUTPUT_SAMPLE_RATE,
                    channels: 1,
                    samples: vec![0.25; OUTPUT_CHUNK_SAMPLES],
                });
            }
        }
    }

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
            if let Some(input) = inputs.first() {
                self.callbacks
                    .lock()
                    .unwrap()
                    .push((generation, input.side, on_audio.clone()));
            }
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
        backend: Arc<TestBackend>,
        actor: LiveSessionActor,
    }

    fn fixture() -> Fixture {
        let root = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(root.path()).unwrap());
        let backend = Arc::new(TestBackend::default());
        let capture = Arc::new(CaptureController::new(backend.clone()));
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
            backend,
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
            transcription: TranscriptionState::Active,
            transcript_stopped_sample: None,
            error_category: None,
            recording_state: RecordingState::Active,
            storage_error_reported: false,
        });
        fixture.actor.next_generation = generation.saturating_add(1);
        fixture.actor.snapshot = fixture.actor.snapshot_for_running();
        (session_id, transcript_id)
    }

    /// The Proxy is derived off the actor after Stop returned; wait for it.
    async fn wait_for_proxy(db: &Db, session_id: SessionId) -> Option<String> {
        for _ in 0..400 {
            let ext = db
                .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
                .unwrap()
                .proxy_ext;
            if ext.is_some() {
                return ext;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        None
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
            transcription: TranscriptionState::Active,
            transcript_stopped_sample: None,
            error_category: None,
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
        fixture.actor.finish_current(None, false).await.unwrap();
        let row = fixture
            .db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(row.status, "complete");
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
        fixture.actor.finish_current(None, false).await.unwrap();
        let row = fixture
            .db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(row.status, "complete");
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

        fixture.actor.finish_current(None, false).await.unwrap();
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
        fixture.actor.finish_current(None, false).await.unwrap();
    }

    #[tokio::test]
    async fn setup_rejection_keeps_wav_active_and_recording_only_choice_persists_tail_gap() {
        let mut fixture = fixture();
        let (session_id, transcript_id) = add_recording(&mut fixture, 1);
        let events = Arc::new(Mutex::new(Vec::new()));
        fixture
            .actor
            .register_subscriber(collect_channel(events.clone()));

        fixture
            .actor
            .handle_internal(Internal::GatewayEnded {
                generation: 1,
                result: Err(crate::gemini::live::LiveFailure::SetupRejected),
            })
            .await;

        assert_eq!(
            fixture.actor.snapshot.transcription,
            TranscriptionState::SetupRejected
        );
        assert_eq!(fixture.actor.snapshot.recording, RecordingState::Active);
        assert!(fixture.capture.active_source().is_some());
        let baseline = fixture.actor.running.as_ref().unwrap().baseline_sample;
        let gap_start_sample = fixture
            .actor
            .running
            .as_ref()
            .unwrap()
            .transcript_stopped_sample
            .unwrap();
        fixture.actor.continue_recording_only().await.unwrap();
        assert_eq!(
            fixture.actor.snapshot.transcription,
            TranscriptionState::RecordingOnly
        );
        assert_eq!(fixture.actor.snapshot.recording, RecordingState::Active);
        assert!(fixture.capture.active_source().is_some());

        let remount_events = Arc::new(Mutex::new(Vec::new()));
        fixture
            .actor
            .register_subscriber(collect_channel(remount_events.clone()));
        assert!(matches!(
            remount_events.lock().unwrap().first(),
            Some(LiveEvent::Ready { snapshot, .. })
                if snapshot.transcription == TranscriptionState::RecordingOnly
                    && snapshot.recording == RecordingState::Active
        ));

        let sample_before = fixture.capture.sample_clock();
        fixture.backend.emit_chunk();
        tokio::time::timeout(Duration::from_secs(1), async {
            while fixture.capture.sample_clock() <= sample_before {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("capture sample clock should advance while recording-only");
        fixture.actor.finish_current(None, false).await.unwrap();
        assert_eq!(
            fixture.actor.snapshot.transcription,
            TranscriptionState::Stopped
        );
        assert_eq!(fixture.actor.snapshot.recording, RecordingState::Stopped);
        assert!(fixture.capture.active_source().is_none());

        let rows = fixture
            .db
            .with_connection(|conn| Ok(repo::segments::list_for_transcript(conn, transcript_id)?))
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, SegmentKind::Gap);
        assert_eq!(rows[0].gap_reason, Some(GapReason::Disconnected));
        let gap_end_sample = fixture.capture.sample_clock();
        let expected_start = relative_seconds(gap_start_sample, baseline);
        let expected_end = relative_seconds(gap_end_sample, baseline);
        assert!(expected_end > expected_start);
        assert_eq!(rows[0].start_sec, expected_start);
        assert_eq!(rows[0].end_sec, expected_end);
        let session = fixture
            .db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(session.status, "complete");
        assert!(events.lock().unwrap().iter().any(|event| matches!(
            event,
            LiveEvent::Gap { start_sec, end_sec, reason, .. }
                if *start_sec == expected_start
                    && *end_sec == expected_end
                    && reason == "disconnected"
        )));
    }

    #[tokio::test]
    async fn stop_flush_failure_keeps_the_session_so_stop_can_be_retried() {
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
                conn.execute_batch(
                    "CREATE TRIGGER fail_segments BEFORE INSERT ON segments \
                     BEGIN SELECT RAISE(FAIL, 'injected'); END;",
                )?;
                Ok(())
            })
            .unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        fixture
            .actor
            .register_subscriber(collect_channel(events.clone()));

        let error = fixture.actor.flush_active("recording").await.unwrap_err();
        assert_eq!(error.code, Code::Storage);
        assert_eq!(fixture.actor.running.as_ref().unwrap().pending.len(), 1);
        assert!(fixture.actor.finish_current(None, false).await.is_err());
        // Session is kept with its pending batch; no Done was announced.
        let running = fixture
            .actor
            .running
            .as_ref()
            .expect("Stop must stay retryable");
        assert_eq!(running.pending.len(), 1);
        assert_eq!(fixture.actor.snapshot.recording, RecordingState::Failed);
        assert!(!events
            .lock()
            .unwrap()
            .iter()
            .any(|event| matches!(event, LiveEvent::Done { .. })));

        fixture
            .db
            .with_connection(|conn| {
                conn.execute_batch("DROP TRIGGER fail_segments")?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            fixture.actor.finish_current(None, false).await.unwrap(),
            session_id
        );
        assert!(fixture.actor.running.is_none());
        let row = fixture
            .db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(row.status, "complete");
        let segments = fixture
            .db
            .with_connection(|conn| Ok(repo::segments::list_for_transcript(conn, transcript_id)?))
            .unwrap();
        assert!(segments.iter().any(|segment| segment.text == "Retain me."));
        assert!(events
            .lock()
            .unwrap()
            .iter()
            .any(|event| matches!(event, LiveEvent::Done { .. })));
    }

    #[tokio::test]
    async fn close_retry_failure_marks_recording_failed_without_announcing_done() {
        let mut fixture = fixture();
        // No session row is in `finalizing`, so the retried commit fails.
        fixture.actor.pending_close_finalize = Some((SessionId::new(), 1.0));
        let events = Arc::new(Mutex::new(Vec::new()));
        fixture
            .actor
            .register_subscriber(collect_channel(events.clone()));

        assert!(fixture.actor.finish_current(None, true).await.is_err());
        assert_eq!(fixture.actor.snapshot.recording, RecordingState::Failed);
        assert!(fixture.actor.pending_close_finalize.is_some());
        assert!(!events
            .lock()
            .unwrap()
            .iter()
            .any(|event| matches!(event, LiveEvent::Done { .. })));
    }

    #[tokio::test]
    async fn stop_persists_events_still_queued_from_the_gateway() {
        let mut fixture = fixture();
        let (session_id, transcript_id) = add_recording(&mut fixture, 1);
        let baseline = fixture.actor.running.as_ref().unwrap().baseline_sample;
        fixture
            .actor
            .internal_sender
            .send(Internal::GatewayEvent {
                generation: 1,
                event: GatewayEvent::InputTranscription {
                    text: crate::core::sensitive::Sensitive::new("Queued words".to_owned()),
                    sample_start: baseline + 800,
                    sample_end: baseline + 1_600,
                },
            })
            .await
            .unwrap();

        assert_eq!(
            fixture.actor.finish_current(None, false).await.unwrap(),
            session_id
        );
        let segments = fixture
            .db
            .with_connection(|conn| Ok(repo::segments::list_for_transcript(conn, transcript_id)?))
            .unwrap();
        assert!(segments
            .iter()
            .any(|segment| segment.text.contains("Queued words")));
    }

    #[tokio::test]
    async fn set_source_is_rejected_when_no_session_is_running_and_opens_no_capture() {
        let mut fixture = fixture();
        let error = fixture
            .actor
            .set_source("mic:test".to_owned())
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Request);
        assert!(fixture.capture.active_source().is_none());

        add_recording(&mut fixture, 1);
        fixture
            .actor
            .set_source("mic:test".to_owned())
            .await
            .unwrap();
        assert!(fixture.capture.active_source().is_some());
        fixture.actor.finish_current(None, false).await.unwrap();
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
                tag_ids: Vec::new(),
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
                tag_ids: Vec::new(),
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
        let stopped_id = fixture.actor.finish_current(None, false).await.unwrap();
        assert_eq!(stopped_id, session_id);

        let path = crate::core::paths::recording_path(fixture._root.path(), session_id);
        let mut wav = hound::WavReader::open(path).unwrap();
        assert!(wav.duration() >= OUTPUT_CHUNK_SAMPLES as u32);
        assert!(wav.samples::<i16>().any(|sample| sample.unwrap() != 0));
        let row = fixture
            .db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(row.status, "complete");
        assert_eq!(
            wait_for_proxy(&fixture.db, session_id).await.as_deref(),
            Some("flac")
        );
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
                tag_ids: Vec::new(),
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
        busy.actor.finish_current(None, false).await.unwrap();

        let mut no_consent = fixture();
        let error = no_consent
            .actor
            .start(LiveStartParams {
                source: "mic:test".to_owned(),
                language: TranscribeLanguage::Auto,
                tag_ids: Vec::new(),
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
            transcription: TranscriptionState::Active,
            transcript_stopped_sample: None,
            error_category: None,
            recording_state: RecordingState::Active,
            storage_error_reported: false,
        });
        let started = std::time::Instant::now();
        let result = fixture.actor.finish_current(None, false).await;
        assert!(started.elapsed() <= OLD_GENERATION_DRAIN + Duration::from_millis(200));
        assert!(
            result.is_err(),
            "the fixture intentionally has no DB session row"
        );
        // A failed final flush keeps the session so Stop can be retried.
        assert!(fixture.actor.running.is_some());
    }

    #[tokio::test]
    async fn normal_stop_finishes_recording_without_error_while_gateway_drain_times_out() {
        let mut fixture = fixture();
        let (_session_id, _transcript_id) = add_recording(&mut fixture, 1);
        let baseline = fixture.actor.running.as_ref().unwrap().baseline_sample;
        tokio::time::timeout(Duration::from_secs(2), async {
            while fixture.capture.sample_clock() <= baseline {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .expect("capture should publish audio before stopping");
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
        tokio::time::timeout(
            Duration::from_secs(2),
            fixture.actor.finish_current(None, false),
        )
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

    #[tokio::test]
    async fn close_stop_flushes_live_and_commits_without_waiting_for_a_proxy() {
        let mut fixture = fixture();
        let (session_id, _transcript_id) = add_recording(&mut fixture, 1);

        assert_eq!(
            fixture.actor.finish_current(None, true).await.unwrap(),
            session_id
        );

        let row = fixture
            .db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(row.status, "complete");
        assert_eq!(row.proxy_ext, None);
        assert!(crate::core::paths::recording_path(fixture._root.path(), session_id).is_file());
        assert!(fixture.actor.running.is_none());
    }

    #[tokio::test]
    async fn device_error_uses_the_save_pipeline_and_returns_the_completed_session_id() {
        let mut fixture = fixture();
        let (session_id, _) = add_recording(&mut fixture, 1);
        let baseline = fixture.actor.running.as_ref().unwrap().baseline_sample;
        tokio::time::timeout(Duration::from_secs(2), async {
            while fixture.capture.sample_clock() <= baseline {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .expect("capture should publish audio before the device failure");
        let events = Arc::new(Mutex::new(Vec::new()));
        fixture
            .actor
            .register_subscriber(collect_channel(events.clone()));
        let device_error = AppError::new(Code::Permission, "audio device disconnected");

        let stopped_id = fixture
            .actor
            .finish_current(Some(device_error.clone()), false)
            .await
            .unwrap();

        assert_eq!(stopped_id, session_id);
        let row = fixture
            .db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(row.status, "complete");
        assert_eq!(
            wait_for_proxy(&fixture.db, session_id).await.as_deref(),
            Some("flac")
        );
        assert_eq!(
            fixture.actor.snapshot.error_category,
            Some(Category::Permission)
        );
        let events = events.lock().unwrap();
        assert!(events.iter().any(|event| matches!(
            event,
            LiveEvent::Error { error, .. } if error.category == Category::Permission
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            LiveEvent::Final { session_id: final_id, .. } if *final_id == session_id
        )));
    }

    fn running_with_buffer(buffer: &str, span_samples: u64) -> RunningSession {
        let (terminal_errors, terminal_rx) = watch::channel(None);
        drop(terminal_errors);
        RunningSession {
            generation: 1,
            session_id: SessionId::new(),
            transcript_id: TranscriptId::new(),
            baseline_sample: 0,
            last_flush_sample: 0,
            pending: Vec::new(),
            sentence_buffer: buffer.to_owned(),
            sentence_start: Some(0),
            sentence_end: span_samples,
            recording: None,
            terminal_errors: terminal_rx,
            gateway_cancel: CancellationToken::new(),
            gateway_task: tokio::spawn(async {}),
            connection: ConnectionState::Connected,
            transcription: TranscriptionState::Active,
            transcript_stopped_sample: None,
            error_category: None,
            recording_state: RecordingState::Active,
            storage_error_reported: false,
        }
    }

    fn split_texts(buffer: &str) -> (Vec<String>, String) {
        let mut running = running_with_buffer(buffer, 16_000);
        let completed = split_complete_sentences(&mut running)
            .into_iter()
            .map(|segment| segment.text)
            .collect();
        (completed, running.sentence_buffer)
    }

    #[tokio::test]
    async fn sentences_are_not_split_inside_decimals_ellipses_or_abbreviations() {
        assert_eq!(
            split_texts("3.14 is out."),
            (vec!["3.14 is out.".to_owned()], String::new())
        );
        // A trailing `3.` waits: the next delta may continue the number.
        assert_eq!(split_texts("Pi is 3."), (Vec::new(), "Pi is 3.".to_owned()));
        assert_eq!(
            split_texts("Wait... what?"),
            (vec!["Wait... what?".to_owned()], String::new())
        );
        assert_eq!(
            split_texts("Ask Dr. smith about it. Then go."),
            (
                vec!["Ask Dr. smith about it.".to_owned(), "Then go.".to_owned()],
                String::new()
            )
        );
        // An abbreviation before a capital still ends the sentence.
        assert_eq!(
            split_texts("I met Dr. Smith. Fine."),
            (
                vec![
                    "I met Dr.".to_owned(),
                    "Smith.".to_owned(),
                    "Fine.".to_owned()
                ],
                String::new()
            )
        );
        // A plain sentence end and CJK terminators still split.
        assert_eq!(
            split_texts("Hello. 元気ですか？はい。"),
            (
                vec![
                    "Hello.".to_owned(),
                    "元気ですか？".to_owned(),
                    "はい。".to_owned()
                ],
                String::new()
            )
        );
    }

    #[tokio::test]
    async fn punctuation_only_pieces_are_never_persisted() {
        let mut running = running_with_buffer("Done. . ... ! Next.", 16_000);
        let completed = split_complete_sentences(&mut running);
        let texts = completed
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>();
        assert_eq!(texts, ["Done.", "Next."]);
        assert!(running.pending.iter().all(|draft| has_content(&draft.text)));
        assert!(!has_content("... !?"));
        assert!(has_content("v2"));
    }

    #[tokio::test]
    async fn streamed_decimal_is_reassembled_before_it_can_split() {
        let mut fixture = fixture();
        add_recording(&mut fixture, 1);
        let baseline = fixture.actor.running.as_ref().unwrap().baseline_sample;
        for (index, text) in ["Version 3.", "14 is out."].into_iter().enumerate() {
            fixture
                .actor
                .handle_internal(Internal::GatewayEvent {
                    generation: 1,
                    event: GatewayEvent::InputTranscription {
                        text: crate::core::sensitive::Sensitive::new(text.to_owned()),
                        sample_start: baseline + index as u64 * 800,
                        sample_end: baseline + (index as u64 + 1) * 800,
                    },
                })
                .await;
        }
        let running = fixture.actor.running.as_ref().unwrap();
        assert_eq!(running.pending.len(), 1);
        assert_eq!(running.pending[0].text, "Version 3.14 is out.");
        fixture.actor.finish_current(None, false).await.unwrap();
    }

    #[tokio::test]
    async fn turn_complete_flushes_the_open_sentence_emits_turn_and_persists() {
        let mut fixture = fixture();
        let (_session_id, transcript_id) = add_recording(&mut fixture, 1);
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
                    text: crate::core::sensitive::Sensitive::new("no terminator yet".to_owned()),
                    sample_start: baseline + 800,
                    sample_end: baseline + 2_400,
                },
            })
            .await;
        assert!(fixture.actor.running.as_ref().unwrap().pending.is_empty());

        fixture
            .actor
            .handle_internal(Internal::GatewayEvent {
                generation: 1,
                event: GatewayEvent::TurnComplete {
                    sample_start: baseline + 800,
                    sample_end: baseline + 3_200,
                },
            })
            .await;

        {
            let events = events.lock().unwrap();
            let segment_at = events
                .iter()
                .position(|event| {
                    matches!(event, LiveEvent::Segment { segment, .. }
                        if segment.text == "no terminator yet")
                })
                .expect("open sentence becomes a segment");
            let turn_at = events
                .iter()
                .position(|event| matches!(event, LiveEvent::Turn { .. }))
                .expect("turn is announced");
            assert!(segment_at < turn_at);
        }
        let running = fixture.actor.running.as_ref().unwrap();
        assert!(running.sentence_buffer.is_empty());
        assert!(running.sentence_start.is_none());
        // The turn boundary also checkpointed the batch.
        let persisted = fixture
            .db
            .with_connection(|conn| Ok(repo::segments::list_for_transcript(conn, transcript_id)?))
            .unwrap();
        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0].text, "no terminator yet");
        fixture.actor.finish_current(None, false).await.unwrap();
    }

    #[tokio::test]
    async fn turn_complete_with_only_punctuation_emits_no_segment() {
        let mut fixture = fixture();
        let (_session_id, transcript_id) = add_recording(&mut fixture, 1);
        let baseline = fixture.actor.running.as_ref().unwrap().baseline_sample;
        {
            let running = fixture.actor.running.as_mut().unwrap();
            running.sentence_buffer = " ... ".to_owned();
            running.sentence_start = Some(baseline);
        }
        fixture
            .actor
            .handle_internal(Internal::GatewayEvent {
                generation: 1,
                event: GatewayEvent::TurnComplete {
                    sample_start: baseline,
                    sample_end: baseline + 1_600,
                },
            })
            .await;
        let persisted = fixture
            .db
            .with_connection(|conn| Ok(repo::segments::list_for_transcript(conn, transcript_id)?))
            .unwrap();
        assert!(persisted.is_empty());
        fixture.actor.finish_current(None, false).await.unwrap();
    }

    #[tokio::test]
    async fn stop_persists_an_unterminated_sentence_buffer() {
        let mut fixture = fixture();
        let (_session_id, transcript_id) = add_recording(&mut fixture, 1);
        let baseline = fixture.actor.running.as_ref().unwrap().baseline_sample;
        fixture
            .actor
            .handle_internal(Internal::GatewayEvent {
                generation: 1,
                event: GatewayEvent::InputTranscription {
                    text: crate::core::sensitive::Sensitive::new("trailing words".to_owned()),
                    sample_start: baseline + 800,
                    sample_end: baseline + 2_400,
                },
            })
            .await;
        assert!(fixture.actor.running.as_ref().unwrap().pending.is_empty());

        fixture.actor.finish_current(None, false).await.unwrap();

        let persisted = fixture
            .db
            .with_connection(|conn| Ok(repo::segments::list_for_transcript(conn, transcript_id)?))
            .unwrap();
        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0].text, "trailing words");
    }

    #[tokio::test]
    async fn stop_duration_comes_from_the_written_wav_samples() {
        let mut fixture = fixture();
        let (session_id, _) = add_recording(&mut fixture, 1);
        let baseline = fixture.actor.running.as_ref().unwrap().baseline_sample;
        tokio::time::timeout(Duration::from_secs(2), async {
            while fixture.capture.sample_clock() <= baseline {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();

        fixture.actor.finish_current(None, false).await.unwrap();

        let wav = hound::WavReader::open(crate::core::paths::recording_path(
            fixture._root.path(),
            session_id,
        ))
        .unwrap();
        let expected = f64::from(wav.duration()) / f64::from(OUTPUT_SAMPLE_RATE);
        let row = fixture
            .db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(row.duration_sec, expected);
        assert_eq!(duration_from_written_samples(0), None);
        assert_eq!(duration_from_written_samples(32_000), Some(2.0));
    }

    #[tokio::test]
    async fn stop_commits_complete_before_the_proxy_and_releases_its_claim_afterwards() {
        let mut fixture = fixture();
        let busy = Arc::new(Mutex::new(HashSet::new()));
        fixture.actor.recovering = busy.clone();
        let (session_id, _) = add_recording(&mut fixture, 1);
        tokio::time::sleep(Duration::from_millis(150)).await;

        fixture.actor.finish_current(None, false).await.unwrap();

        // `complete` is committed by the time Stop returns; the Proxy is a
        // detached follow-up that eventually appears and frees its claim.
        let row = fixture
            .db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(row.status, "complete");
        assert_eq!(
            wait_for_proxy(&fixture.db, session_id).await.as_deref(),
            Some("flac")
        );
        tokio::time::timeout(Duration::from_secs(2), async {
            while !busy.lock().unwrap().is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the proxy claim must be released");
    }

    #[tokio::test]
    async fn a_session_claimed_elsewhere_is_not_encoded_by_stop() {
        let mut fixture = fixture();
        let busy = Arc::new(Mutex::new(HashSet::new()));
        fixture.actor.recovering = busy.clone();
        let (session_id, _) = add_recording(&mut fixture, 1);
        busy.lock().unwrap().insert(session_id);
        tokio::time::sleep(Duration::from_millis(150)).await;

        fixture.actor.finish_current(None, false).await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;

        let row = fixture
            .db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(row.status, "complete");
        assert_eq!(row.proxy_ext, None);
        assert!(busy.lock().unwrap().contains(&session_id));
    }

    fn start_params() -> LiveStartParams {
        LiveStartParams {
            source: "mic:test".to_owned(),
            language: TranscribeLanguage::Auto,
            tag_ids: Vec::new(),
            locale: None,
            ui_language: UiLanguage::En,
            model: "live-start-test".to_owned(),
            consent: ConsentSnapshot::new(crate::consent::CURRENT_VERSION, false),
        }
    }

    #[tokio::test]
    async fn start_is_refused_while_a_close_save_is_pending_or_failed() {
        let mut fixture = fixture();
        fixture.actor.pending_close_finalize = Some((SessionId::new(), 1.0));
        let error = fixture.actor.start(start_params()).await.unwrap_err();
        assert_eq!(error.code, Code::Request);
        assert!(fixture.capture.active_source().is_none());

        fixture.actor.pending_close_finalize = None;
        fixture.actor.close_failed = true;
        let error = fixture.actor.start(start_params()).await.unwrap_err();
        assert_eq!(error.code, Code::Request);
        assert!(fixture.capture.active_source().is_none());
    }

    #[tokio::test]
    async fn failed_transcript_creation_leaves_no_phantom_recording_row() {
        let mut fixture = fixture();
        let (pool, pool_actor) =
            KeyPoolHandle::channel(Arc::new(OneKeyProvider), Arc::new(SystemClock));
        tokio::spawn(pool_actor.run());
        pool.refresh().await.unwrap();
        fixture.actor.key_pool = pool.clone();
        fixture.actor.gateway = LiveGateway::new(Arc::new(PendingConnector), pool);
        fixture
            .db
            .with_connection(|conn| {
                conn.execute_batch(
                    "CREATE TRIGGER fail_transcripts BEFORE INSERT ON transcripts \
                     BEGIN SELECT RAISE(FAIL, 'injected'); END;",
                )?;
                Ok(())
            })
            .unwrap();

        let error = fixture.actor.start(start_params()).await.unwrap_err();

        assert_eq!(error.code, Code::Storage);
        assert!(fixture.actor.running.is_none());
        assert!(fixture.capture.active_source().is_none());
        let sessions = fixture
            .db
            .with_connection(|conn| Ok(repo::sessions::list(conn)?))
            .unwrap();
        assert!(sessions.is_empty(), "no phantom recording row");
        let media_left = std::fs::read_dir(crate::core::paths::media_root(fixture._root.path()))
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(media_left, 0);
    }

    #[tokio::test]
    async fn unsubscribe_drops_only_the_named_channel() {
        let mut fixture = fixture();
        let first_events = Arc::new(Mutex::new(Vec::new()));
        let second_events = Arc::new(Mutex::new(Vec::new()));
        let first = collect_channel(first_events.clone());
        let first_id = first.id();
        fixture.actor.register_subscriber(first);
        fixture
            .actor
            .register_subscriber(collect_channel(second_events.clone()));
        assert_eq!(fixture.actor.subscribers.len(), 2);

        fixture.actor.unregister_subscriber(first_id);
        assert_eq!(fixture.actor.subscribers.len(), 1);
        fixture.actor.emit(LiveEvent::Turn { seq: 0 });
        assert_eq!(
            first_events.lock().unwrap().len(),
            1,
            "only the Ready event"
        );
        assert_eq!(second_events.lock().unwrap().len(), 2);
        // Unknown ids are a no-op.
        fixture.actor.unregister_subscriber(first_id);
        assert_eq!(fixture.actor.subscribers.len(), 1);
    }
}
