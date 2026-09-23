//! `JobRegistry` actor (story 2.4): một hàng đợi tuần tự trong bộ nhớ, không
//! `Arc<Mutex>` bọc state (spec Boundaries) — state chỉ đổi bên trong
//! [`JobRegistryActor::run`], mọi caller khác chỉ nói chuyện qua
//! [`JobRegistryHandle`] (`mpsc` + `oneshot`), đúng khuôn của
//! `gemini::keys::KeyPoolActor`.
//!
//! Một Job chạy sequential pipeline: Proxy staging (best-effort — lỗi chỉ
//! thành `proxy_error`) → decode/chunk (một `spawn_blocking`, đẩy `Chunk`
//! qua kênh bounded ≤ 2 sang vòng async) → Gemini tuần tự từng Chunk → gộp
//! (`merge::MergeBuilder`) → `library::store::commit_file_session`. Chỉ một
//! Job chạy tại một thời điểm — Job kế tiếp trong hàng chỉ bắt đầu khi Job
//! trước đó rời registry (spec Boundaries: "không chạy Job song song").

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use tauri::ipc::Channel;
use tokio::sync::{mpsc, oneshot};

use crate::core::error::{AppError, Category, Code};
use crate::core::id::{JobId, SessionId};
use crate::core::paths;
use crate::db::Db;
use crate::gemini::{CancellationToken, ConsentSnapshot, GeminiGateway, JobObserver};
use crate::library::store::{self, FileSessionDraft, SessionDraft, TranscriptDraft};
use crate::media::{self, Chunk, ChunkOptions, Chunker};
use crate::transcribe::adapter::{transcribe_chunk_observed, TranscribeFailure};
use crate::transcribe::merge::MergeBuilder;
use crate::transcribe::parser::ChunkTranscript;

use super::job::{CancelOutcome, JobEvent, JobSnapshot, JobState};

const CHUNK_CHANNEL_CAPACITY: usize = 2;
const COMMAND_CHANNEL_CAPACITY: usize = 256;
const DEFAULT_CHUNK_SECONDS: u64 = 300;

fn actor_error() -> AppError {
    AppError::new(Code::Storage, "job registry actor unavailable")
}

fn cancelled_pipeline_error() -> AppError {
    AppError::new(Code::Blocked, "transcribe job was cancelled")
}

/// Everything `transcribe_start` (ipc) already resolved before deciding to
/// create a Job — consent was checked, the file was hashed, and the hash was
/// confirmed not to match an existing session (spec Always: gate order).
#[derive(Debug, Clone)]
pub struct StartParams {
    pub source_path: PathBuf,
    pub source_hash: String,
    pub source_name: Option<String>,
    pub model: String,
    pub consent: ConsentSnapshot,
}

pub type TranscribeChunkFuture =
    Pin<Box<dyn Future<Output = Result<ChunkTranscript, TranscribeFailure>> + Send>>;

/// Seam so the registry pipeline can be driven by a deterministic fake in
/// tests instead of the real Gemini gateway (spec Tasks: "pipeline sau trait
/// `ChunkTranscriber` để test bằng fake").
pub trait ChunkTranscriber: Send + Sync + 'static {
    fn transcribe(
        &self,
        model: String,
        chunk: Chunk,
        consent: ConsentSnapshot,
        cancellation: CancellationToken,
        observer: Arc<dyn JobObserver>,
    ) -> TranscribeChunkFuture;
}

/// Production transcriber: the real Gemini adapter through the app's one
/// gateway.
pub struct GatewayTranscriber {
    gateway: Arc<GeminiGateway>,
}

impl GatewayTranscriber {
    pub fn new(gateway: Arc<GeminiGateway>) -> Self {
        Self { gateway }
    }
}

impl ChunkTranscriber for GatewayTranscriber {
    fn transcribe(
        &self,
        model: String,
        chunk: Chunk,
        consent: ConsentSnapshot,
        cancellation: CancellationToken,
        observer: Arc<dyn JobObserver>,
    ) -> TranscribeChunkFuture {
        let gateway = self.gateway.clone();
        Box::pin(async move {
            transcribe_chunk_observed(
                &gateway,
                &model,
                &chunk,
                consent,
                cancellation,
                Some(observer),
            )
            .await
        })
    }
}

#[derive(Clone)]
pub struct JobRegistryHandle {
    commands: mpsc::Sender<Command>,
}

impl JobRegistryHandle {
    /// Start a new Job. The registry pre-allocates both `JobId` and
    /// `SessionId` right away (spec Always: "Registry cấp trước `session_id`
    /// dự kiến") so the caller can already route to `/session/:id` before
    /// any work — let alone a commit — has happened.
    pub async fn start(&self, params: StartParams) -> Result<(JobId, SessionId), AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Start { params, reply })
            .await
            .map_err(|_| actor_error())?;
        response.await.map_err(|_| actor_error())
    }

    pub async fn cancel(&self, job_id: JobId) -> Result<CancelOutcome, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Cancel { job_id, reply })
            .await
            .map_err(|_| actor_error())?;
        response.await.map_err(|_| actor_error())
    }

    /// Register a Channel and get an immediate snapshot in the very same
    /// actor command (spec Always: "snapshot và đăng ký Channel trong cùng
    /// một lệnh actor") so no event fired between "read state" and
    /// "register" can ever be missed by the new subscriber.
    pub async fn subscribe(&self, channel: Channel<JobEvent>) -> Result<(), AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Subscribe { channel, reply })
            .await
            .map_err(|_| actor_error())?;
        response.await.map_err(|_| actor_error())
    }

    pub async fn is_busy(&self, session_id: SessionId) -> Result<bool, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::IsBusy { session_id, reply })
            .await
            .map_err(|_| actor_error())?;
        response.await.map_err(|_| actor_error())
    }

    pub async fn snapshot(&self) -> Result<Vec<JobSnapshot>, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Snapshot { reply })
            .await
            .map_err(|_| actor_error())?;
        response.await.map_err(|_| actor_error())
    }

    fn try_send_internal(&self, message: Internal) {
        let _ = self.commands.try_send(Command::Internal(message));
    }

    async fn send_internal(&self, message: Internal) {
        let _ = self.commands.send(Command::Internal(message)).await;
    }
}

enum Command {
    Start {
        params: StartParams,
        reply: oneshot::Sender<(JobId, SessionId)>,
    },
    Cancel {
        job_id: JobId,
        reply: oneshot::Sender<CancelOutcome>,
    },
    Subscribe {
        channel: Channel<JobEvent>,
        reply: oneshot::Sender<()>,
    },
    IsBusy {
        session_id: SessionId,
        reply: oneshot::Sender<bool>,
    },
    Snapshot {
        reply: oneshot::Sender<Vec<JobSnapshot>>,
    },
    Internal(Internal),
}

/// Messages the running pipeline task reports back to the actor. Delivered
/// through the same `mpsc` as every other command, so the actor remains the
/// single owner of all Job state (spec Boundaries).
enum Internal {
    Progress {
        job_id: JobId,
        processed_ms: u64,
        total_ms: u64,
        chunk_index: u32,
        chunk_count: u32,
    },
    WaitingQuota {
        job_id: JobId,
        waiting: bool,
    },
    KeyInUse {
        job_id: JobId,
        ordinal: u32,
    },
    Attempt {
        job_id: JobId,
        attempt: u32,
    },
    Finished {
        job_id: JobId,
        outcome: JobOutcome,
    },
}

enum JobOutcome {
    Committed(SessionId),
    Cancelled,
    Error(AppError),
}

struct JobEntry {
    session_id: SessionId,
    source_name: Option<String>,
    state: JobState,
    processed_ms: u64,
    total_ms: u64,
    chunk_index: u32,
    chunk_count: u32,
    key_ordinal: Option<u32>,
    attempt: Option<u32>,
    waiting_quota: bool,
    cancel: CancellationToken,
}

/// Milliseconds are kept as `u64` everywhere internally (source durations
/// come out of `f64` seconds) and only narrowed to `u32` at the
/// `JobSnapshot` boundary — specta-typescript forbids exporting BigInt-style
/// integers (see `ipc/spike_channel.rs`), and `u32` milliseconds already
/// cover about 49 days of audio.
fn to_snapshot_ms(value: u64) -> u32 {
    value.min(u32::MAX as u64) as u32
}

impl JobEntry {
    fn snapshot(&self, job_id: JobId) -> JobSnapshot {
        JobSnapshot {
            job_id,
            session_id: self.session_id,
            source_name: self.source_name.clone(),
            state: self.state,
            processed_ms: to_snapshot_ms(self.processed_ms),
            total_ms: to_snapshot_ms(self.total_ms),
            chunk_index: self.chunk_index,
            chunk_count: self.chunk_count,
            key_ordinal: self.key_ordinal,
            attempt: self.attempt,
            waiting_quota: self.waiting_quota,
        }
    }
}

pub struct JobRegistryActor {
    receiver: mpsc::Receiver<Command>,
    self_sender: mpsc::Sender<Command>,
    db: Arc<Db>,
    root: PathBuf,
    transcriber: Arc<dyn ChunkTranscriber>,
    seq: u32,
    order: VecDeque<JobId>,
    jobs: HashMap<JobId, JobEntry>,
    /// `StartParams` for a Job not yet running. Removed the moment its
    /// pipeline is spawned (immediately for the first Job, later for a
    /// promoted one) — a running Job never has an entry here.
    pending_params: HashMap<JobId, StartParams>,
    subscribers: Vec<Channel<JobEvent>>,
}

/// Build the connected handle/actor pair. `root` is the same app-data root
/// used by `library::store`/`core::paths` (staging + published media live
/// under it).
pub fn channel(
    db: Arc<Db>,
    root: PathBuf,
    transcriber: Arc<dyn ChunkTranscriber>,
) -> (JobRegistryHandle, JobRegistryActor) {
    let (commands, receiver) = mpsc::channel(COMMAND_CHANNEL_CAPACITY);
    let handle = JobRegistryHandle {
        commands: commands.clone(),
    };
    let actor = JobRegistryActor {
        receiver,
        self_sender: commands,
        db,
        root,
        transcriber,
        seq: 0,
        order: VecDeque::new(),
        jobs: HashMap::new(),
        pending_params: HashMap::new(),
        subscribers: Vec::new(),
    };
    (handle, actor)
}

impl JobRegistryActor {
    pub async fn run(mut self) {
        while let Some(command) = self.receiver.recv().await {
            self.handle(command);
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Start { params, reply } => self.handle_start(params, reply),
            Command::Cancel { job_id, reply } => self.handle_cancel(job_id, reply),
            Command::Subscribe { channel, reply } => self.handle_subscribe(channel, reply),
            Command::IsBusy { session_id, reply } => {
                let busy = self
                    .jobs
                    .values()
                    .any(|entry| entry.session_id == session_id);
                let _ = reply.send(busy);
            }
            Command::Snapshot { reply } => {
                let _ = reply.send(self.snapshot_vec());
            }
            Command::Internal(message) => self.handle_internal(message),
        }
    }

    fn snapshot_vec(&self) -> Vec<JobSnapshot> {
        self.order
            .iter()
            .filter_map(|job_id| self.jobs.get(job_id).map(|entry| entry.snapshot(*job_id)))
            .collect()
    }

    fn handle_start(&mut self, params: StartParams, reply: oneshot::Sender<(JobId, SessionId)>) {
        let job_id = JobId::new();
        let session_id = SessionId::new();
        let will_run_now = self.order.is_empty();
        let entry = JobEntry {
            session_id,
            source_name: params.source_name.clone(),
            state: if will_run_now {
                JobState::Running
            } else {
                JobState::Queued
            },
            processed_ms: 0,
            total_ms: 0,
            chunk_index: 0,
            chunk_count: 0,
            key_ordinal: None,
            attempt: None,
            waiting_quota: false,
            cancel: CancellationToken::new(),
        };
        self.order.push_back(job_id);
        self.jobs.insert(job_id, entry);
        self.pending_params.insert(job_id, params);
        let _ = reply.send((job_id, session_id));
        self.broadcast_updated(job_id);

        if will_run_now {
            self.start_pipeline_for(job_id);
        }
    }

    fn handle_cancel(&mut self, job_id: JobId, reply: oneshot::Sender<CancelOutcome>) {
        let Some(entry) = self.jobs.get(&job_id) else {
            let _ = reply.send(CancelOutcome::AlreadyFinished);
            return;
        };
        let is_running = self.order.front() == Some(&job_id);
        if is_running {
            entry.cancel.cancel();
            let _ = reply.send(CancelOutcome::Cancelling);
            return;
        }
        // Queued, not yet running: cancellation is immediate — drop it from
        // the queue right here (spec I/O Matrix "Nhiều Job": "Huỷ Job đang
        // chờ -> bỏ khỏi hàng, Job khác tiếp tục").
        self.order.retain(|id| *id != job_id);
        self.jobs.remove(&job_id);
        self.pending_params.remove(&job_id);
        let _ = reply.send(CancelOutcome::Cancelling);
        let seq = self.next_seq();
        self.broadcast(JobEvent::Cancelled { seq, job_id });
    }

    fn handle_subscribe(&mut self, channel: Channel<JobEvent>, reply: oneshot::Sender<()>) {
        let jobs = self.snapshot_vec();
        let _ = channel.send(JobEvent::Snapshot {
            seq: self.seq,
            jobs,
        });
        self.subscribers.push(channel);
        let _ = reply.send(());
    }

    fn handle_internal(&mut self, message: Internal) {
        match message {
            Internal::Progress {
                job_id,
                processed_ms,
                total_ms,
                chunk_index,
                chunk_count,
            } => {
                if let Some(entry) = self.jobs.get_mut(&job_id) {
                    entry.processed_ms = processed_ms;
                    entry.total_ms = total_ms;
                    entry.chunk_index = chunk_index;
                    entry.chunk_count = chunk_count.max(chunk_index);
                }
                self.broadcast_updated(job_id);
            }
            Internal::WaitingQuota { job_id, waiting } => {
                if let Some(entry) = self.jobs.get_mut(&job_id) {
                    entry.waiting_quota = waiting;
                }
                self.broadcast_updated(job_id);
            }
            Internal::KeyInUse { job_id, ordinal } => {
                if let Some(entry) = self.jobs.get_mut(&job_id) {
                    entry.key_ordinal = Some(ordinal);
                }
                self.broadcast_updated(job_id);
            }
            Internal::Attempt { job_id, attempt } => {
                if let Some(entry) = self.jobs.get_mut(&job_id) {
                    entry.attempt = Some(attempt);
                }
                self.broadcast_updated(job_id);
            }
            Internal::Finished { job_id, outcome } => self.handle_finished(job_id, outcome),
        }
    }

    fn handle_finished(&mut self, job_id: JobId, outcome: JobOutcome) {
        self.order.retain(|id| *id != job_id);
        self.jobs.remove(&job_id);

        let seq = self.next_seq();
        let event = match outcome {
            JobOutcome::Committed(session_id) => JobEvent::Result {
                seq,
                job_id,
                session_id,
            },
            JobOutcome::Cancelled => JobEvent::Cancelled { seq, job_id },
            JobOutcome::Error(error) => JobEvent::Error { seq, job_id, error },
        };
        self.broadcast(event);

        self.promote_next();
    }

    /// Promote the next queued Job (if any) to running — only ever one Job
    /// runs at a time (spec Boundaries: "không chạy Job song song").
    fn promote_next(&mut self) {
        let Some(&job_id) = self.order.front() else {
            return;
        };
        let Some(entry) = self.jobs.get_mut(&job_id) else {
            return;
        };
        entry.state = JobState::Running;
        self.broadcast_updated(job_id);
        self.start_pipeline_for(job_id);
    }

    /// Spawn the pipeline task for `job_id`, consuming its stored
    /// `StartParams`. A no-op if the Job or its params are gone (already
    /// cancelled/removed between being queued and being promoted).
    fn start_pipeline_for(&mut self, job_id: JobId) {
        let Some(entry) = self.jobs.get(&job_id) else {
            return;
        };
        let Some(params) = self.pending_params.remove(&job_id) else {
            return;
        };
        let session_id = entry.session_id;
        let cancel = entry.cancel.clone();
        let handle = JobRegistryHandle {
            commands: self.self_sender.clone(),
        };
        let db = self.db.clone();
        let root = self.root.clone();
        let transcriber = self.transcriber.clone();
        tokio::spawn(async move {
            let outcome = run_job(
                job_id,
                session_id,
                &params,
                &db,
                &root,
                &transcriber,
                &cancel,
                &handle,
            )
            .await;
            handle
                .send_internal(Internal::Finished { job_id, outcome })
                .await;
        });
    }

    fn broadcast_updated(&mut self, job_id: JobId) {
        let Some(entry) = self.jobs.get(&job_id) else {
            return;
        };
        let job = entry.snapshot(job_id);
        let seq = self.next_seq();
        self.broadcast(JobEvent::Updated { seq, job });
    }

    fn broadcast(&mut self, event: JobEvent) {
        self.subscribers
            .retain(|channel| channel.send(event.clone()).is_ok());
    }

    fn next_seq(&mut self) -> u32 {
        self.seq = self.seq.wrapping_add(1);
        self.seq
    }
}

/// Fatal per spec I/O Matrix: "Chunk lỗi ... Auth/Model/Consent/đọc
/// nguồn/DB -> `error`, không commit". Category `Blocked` is what
/// `gemini::require_consent`/cancellation both use, so a genuine consent
/// failure and a cancellation share a category — callers must check
/// cancellation first (see `run_job_inner`) before treating a `Blocked`
/// error as fatal.
fn is_fatal(error: &AppError) -> bool {
    matches!(
        error.category,
        Category::Auth | Category::Model | Category::Blocked
    )
}

/// Forwards Gemini progress signals for one running Job back to the actor.
/// Fire-and-forget (`try_send`): a dropped progress tick under backpressure
/// is harmless — the next one carries the current truth regardless.
struct RegistryObserver {
    handle: JobRegistryHandle,
    job_id: JobId,
}

impl JobObserver for RegistryObserver {
    fn waiting_quota(&self, waiting: bool) {
        self.handle.try_send_internal(Internal::WaitingQuota {
            job_id: self.job_id,
            waiting,
        });
    }

    fn key_in_use(&self, ordinal: u32) {
        self.handle.try_send_internal(Internal::KeyInUse {
            job_id: self.job_id,
            ordinal,
        });
    }

    fn attempt(&self, attempt: u32) {
        self.handle.try_send_internal(Internal::Attempt {
            job_id: self.job_id,
            attempt,
        });
    }
}

/// Estimate the number of 5-minute chunks a file of `total_ms` will produce.
/// Only used for progress display — the real Chunker may emit more (rare
/// oversized-payload splitting), in which case the actor simply raises this
/// estimate to match (`Internal::Progress` handling).
fn estimate_chunk_count(total_ms: u64) -> u32 {
    let chunk_ms = DEFAULT_CHUNK_SECONDS * 1000;
    (total_ms.div_ceil(chunk_ms)).max(1) as u32
}

/// Run one Job's full pipeline. Never panics: every fallible step is turned
/// into a [`JobOutcome`] instead of propagating a `Result` up, since this
/// function's only caller is the `tokio::spawn`ed task in
/// `JobRegistryActor::start_pipeline_for`, which has nothing to `?` into.
async fn run_job(
    job_id: JobId,
    session_id: SessionId,
    params: &StartParams,
    db: &Arc<Db>,
    root: &Path,
    transcriber: &Arc<dyn ChunkTranscriber>,
    cancel: &CancellationToken,
    handle: &JobRegistryHandle,
) -> JobOutcome {
    let outcome = run_job_inner(
        job_id,
        session_id,
        params,
        db,
        root,
        transcriber,
        cancel,
        handle,
    )
    .await;
    if !matches!(outcome, JobOutcome::Committed(_)) {
        // Committed already handed staging cleanup to `commit_file_session`
        // (spec Design Notes). Every other exit (cancelled, or a fatal error
        // before a commit was even attempted) must not leave staging behind
        // (spec I/O Matrix "Huỷ": "staging bị xoá").
        if let Err(err) = store::discard_staging(root, job_id) {
            tracing::warn!(error = %err, "không dọn được staging sau khi Job kết thúc không commit");
        }
    }
    outcome
}

async fn run_job_inner(
    job_id: JobId,
    session_id: SessionId,
    params: &StartParams,
    db: &Arc<Db>,
    root: &Path,
    transcriber: &Arc<dyn ChunkTranscriber>,
    cancel: &CancellationToken,
    handle: &JobRegistryHandle,
) -> JobOutcome {
    if cancel.is_cancelled() {
        return JobOutcome::Cancelled;
    }

    // Probe first: total duration drives every progress event, independent
    // of whether Proxy staging below succeeds (spec Code Map: `probe`
    // "tổng thời lượng").
    let probe_path = params.source_path.clone();
    let probe = match tokio::task::spawn_blocking(move || media::probe(&probe_path)).await {
        Ok(Ok(info)) => info,
        Ok(Err(err)) => return JobOutcome::Error(err),
        Err(_) => return JobOutcome::Error(actor_error()),
    };
    let total_ms = (probe.duration_seconds * 1000.0).round().max(0.0) as u64;
    let chunk_count = estimate_chunk_count(total_ms);
    handle
        .send_internal(Internal::Progress {
            job_id,
            processed_ms: 0,
            total_ms,
            chunk_index: 0,
            chunk_count,
        })
        .await;

    if cancel.is_cancelled() {
        return JobOutcome::Cancelled;
    }

    // Proxy staging is best-effort (spec Design Notes: "lỗi Proxy chỉ thành
    // `proxy_error`") — its own failure never aborts the Job.
    let staging_dir = paths::staging_dir(root, job_id);
    let stage_source = params.source_path.clone();
    let staged_proxy: Result<PathBuf, AppError> =
        match tokio::task::spawn_blocking(move || media::create_proxy(&staging_dir, &stage_source))
            .await
        {
            Ok(Ok(info)) => Ok(info.path),
            Ok(Err(err)) => Err(err),
            Err(_) => Err(actor_error()),
        };

    if cancel.is_cancelled() {
        return JobOutcome::Cancelled;
    }

    let (segments, decode_outcome) = decode_and_transcribe(
        job_id,
        params,
        total_ms,
        chunk_count,
        transcriber,
        cancel,
        handle,
    )
    .await;
    if let Some(outcome) = decode_outcome {
        return outcome;
    }

    if cancel.is_cancelled() {
        return JobOutcome::Cancelled;
    }

    let draft = FileSessionDraft {
        session: SessionDraft {
            title: params
                .source_name
                .clone()
                .unwrap_or_else(|| "Untitled".to_string()),
            source_hash: Some(params.source_hash.clone()),
            source_name: params.source_name.clone(),
            duration_sec: probe.duration_seconds,
        },
        transcript: TranscriptDraft {
            model: params.model.clone(),
            language: None,
            segments,
        },
    };

    let db = db.clone();
    let root = root.to_path_buf();
    let commit = tokio::task::spawn_blocking(move || {
        store::commit_file_session(&db, &root, job_id, session_id, draft, staged_proxy)
    })
    .await;
    match commit {
        Ok(Ok(outcome)) => {
            if let Some(err) = outcome.proxy_error {
                tracing::warn!(error = %err, "Proxy lỗi, Transcript vẫn commit bình thường");
            }
            JobOutcome::Committed(outcome.session_id)
        }
        Ok(Err(err)) => JobOutcome::Error(err),
        Err(_) => JobOutcome::Error(actor_error()),
    }
}

/// Decode + chunk the source in one `spawn_blocking` task (pushing `Chunk`s
/// through a bounded channel to this async loop, per Code Map), then
/// transcribe each Chunk in order through `transcriber`, merging results.
/// Returns the merged segments plus `Some(outcome)` when the whole Job must
/// stop here instead of proceeding to commit (cancelled, or a fatal error).
async fn decode_and_transcribe(
    job_id: JobId,
    params: &StartParams,
    total_ms: u64,
    initial_chunk_count: u32,
    transcriber: &Arc<dyn ChunkTranscriber>,
    cancel: &CancellationToken,
    handle: &JobRegistryHandle,
) -> (
    Vec<crate::db::repo::segments::SegmentDraft>,
    Option<JobOutcome>,
) {
    let (chunk_tx, mut chunk_rx) = mpsc::channel::<Result<Chunk, AppError>>(CHUNK_CHANNEL_CAPACITY);
    let cancel_for_decode = cancel.clone();
    let decode_path = params.source_path.clone();
    let decode_handle = tokio::task::spawn_blocking(move || {
        let decode_result: Result<(), AppError> = (|| {
            let mut chunker = Chunker::new(ChunkOptions::default())?;
            let mut push_chunk = |chunk: Chunk| -> Result<(), AppError> {
                if cancel_for_decode.is_cancelled() {
                    return Err(cancelled_pipeline_error());
                }
                chunk_tx
                    .blocking_send(Ok(chunk))
                    .map_err(|_| cancelled_pipeline_error())
            };
            media::decode_mono_16khz(&decode_path, |samples| {
                if cancel_for_decode.is_cancelled() {
                    return Err(cancelled_pipeline_error());
                }
                chunker.push(samples, &mut push_chunk)
            })?;
            chunker.finish(&mut push_chunk)
        })();
        if let Err(err) = decode_result {
            if !cancel_for_decode.is_cancelled() {
                let _ = chunk_tx.blocking_send(Err(err));
            }
        }
    });

    let mut merge = MergeBuilder::new();
    let mut chunk_index: u32 = 0;
    let mut chunk_count = initial_chunk_count;
    let mut outcome: Option<JobOutcome> = None;

    while let Some(item) = chunk_rx.recv().await {
        if cancel.is_cancelled() {
            outcome = Some(JobOutcome::Cancelled);
            break;
        }
        let chunk = match item {
            Ok(chunk) => chunk,
            Err(err) => {
                outcome = Some(if cancel.is_cancelled() {
                    JobOutcome::Cancelled
                } else {
                    JobOutcome::Error(err)
                });
                break;
            }
        };
        chunk_index += 1;
        chunk_count = chunk_count.max(chunk_index);
        handle
            .send_internal(Internal::Progress {
                job_id,
                processed_ms: chunk.start_ms,
                total_ms,
                chunk_index,
                chunk_count,
            })
            .await;

        let observer: Arc<dyn JobObserver> = Arc::new(RegistryObserver {
            handle: handle.clone(),
            job_id,
        });
        let result = transcriber
            .transcribe(
                params.model.clone(),
                chunk.clone(),
                params.consent,
                cancel.clone(),
                observer,
            )
            .await;
        match result {
            Ok(transcript) => merge.push_success(transcript),
            Err(failure) => {
                if cancel.is_cancelled() {
                    outcome = Some(JobOutcome::Cancelled);
                    break;
                }
                if is_fatal(&failure.error) {
                    outcome = Some(JobOutcome::Error(failure.error));
                    break;
                }
                merge.push_failed_chunk(&chunk);
            }
        }

        handle
            .send_internal(Internal::Progress {
                job_id,
                processed_ms: chunk.start_ms + chunk.duration_ms,
                total_ms,
                chunk_index,
                chunk_count,
            })
            .await;
    }

    // Close the receiver before waiting for the decode task so a blocking
    // producer stuck on a full buffer (because this loop broke out early)
    // unblocks immediately instead of waiting on a timeout (spec Always:
    // "Huỷ: ngừng gửi Chunk mới ≤ 2 s").
    drop(chunk_rx);
    let _ = decode_handle.await;

    (merge.finish(), outcome)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use tauri::ipc::InvokeResponseBody;
    use tempfile::tempdir;

    use crate::core::error::Code;
    use crate::db::repo;
    use crate::transcribe::parser::{ChunkTranscript, Segment};

    use super::*;

    #[derive(Clone)]
    enum Behavior {
        Success(String),
        Fail {
            code: Code,
            retryable: bool,
        },
        BlockUntilCancelled,
        /// Signals `waiting_quota(true)` on the observer (as the real
        /// `GeminiGateway` does while awaiting a key — see
        /// `gemini::mod::post_job_observed`) before succeeding, so the
        /// registry's forwarding of that signal into a `JobSnapshot` can be
        /// tested end to end.
        SignalWaitingQuotaThenSucceed(String),
    }

    struct FakeTranscriber {
        behaviors: Mutex<VecDeque<Behavior>>,
        calls: Mutex<u32>,
    }

    impl FakeTranscriber {
        fn new(behaviors: Vec<Behavior>) -> Arc<Self> {
            Arc::new(Self {
                behaviors: Mutex::new(behaviors.into()),
                calls: Mutex::new(0),
            })
        }

        fn call_count(&self) -> u32 {
            *self.calls.lock().unwrap()
        }
    }

    impl ChunkTranscriber for FakeTranscriber {
        fn transcribe(
            &self,
            _model: String,
            chunk: Chunk,
            _consent: ConsentSnapshot,
            cancellation: CancellationToken,
            observer: Arc<dyn JobObserver>,
        ) -> TranscribeChunkFuture {
            *self.calls.lock().unwrap() += 1;
            let behavior = self
                .behaviors
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Behavior::Success("silence".to_string()));
            Box::pin(async move {
                match behavior {
                    Behavior::Success(text) => Ok(ChunkTranscript {
                        segments: vec![Segment {
                            start: chunk.start_ms as f64 / 1000.0,
                            end: (chunk.start_ms + chunk.duration_ms) as f64 / 1000.0,
                            text,
                            speaker: None,
                        }],
                        unresolved: vec![],
                        confirmed_silence: false,
                    }),
                    Behavior::Fail { code, retryable } => Err(TranscribeFailure {
                        error: AppError::new(code, "fake transcriber failure"),
                        retryable,
                    }),
                    Behavior::SignalWaitingQuotaThenSucceed(text) => {
                        observer.waiting_quota(true);
                        Ok(ChunkTranscript {
                            segments: vec![Segment {
                                start: chunk.start_ms as f64 / 1000.0,
                                end: (chunk.start_ms + chunk.duration_ms) as f64 / 1000.0,
                                text,
                                speaker: None,
                            }],
                            unresolved: vec![],
                            confirmed_silence: false,
                        })
                    }
                    Behavior::BlockUntilCancelled => {
                        cancellation.cancelled().await;
                        Err(TranscribeFailure {
                            error: AppError::new(Code::Blocked, "cancelled"),
                            retryable: false,
                        })
                    }
                }
            })
        }
    }

    /// A tiny mono WAV fixture — big enough to produce exactly one 5-minute
    /// Chunk, small enough that hash/probe/decode/staging are effectively
    /// instantaneous in every test (spec Code Map: "fixture media nhỏ").
    fn wav_fixture(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for frame in 0..1_600_i32 {
            let sample = ((frame as f32 * 0.05).sin() * 5_000.0) as i16;
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
        path
    }

    fn start_params(path: PathBuf, hash: &str) -> StartParams {
        StartParams {
            source_path: path,
            source_hash: hash.to_string(),
            source_name: Some("fixture.wav".to_string()),
            model: "gemini-flash-lite-latest".to_string(),
            consent: ConsentSnapshot::new(1, false),
        }
    }

    fn registry_for_test(root: &Path, transcriber: Arc<dyn ChunkTranscriber>) -> JobRegistryHandle {
        let db = Arc::new(Db::open(root).unwrap());
        let (handle, actor) = channel(db, root.to_path_buf(), transcriber);
        tokio::spawn(actor.run());
        handle
    }

    fn test_channel() -> (Channel<JobEvent>, Arc<Mutex<Vec<JobEvent>>>) {
        let received: Arc<Mutex<Vec<JobEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let received_for_cb = received.clone();
        let channel = Channel::new(move |body| {
            let event: JobEvent = match body {
                InvokeResponseBody::Json(json) => {
                    serde_json::from_str(&json).expect("JobEvent phải giải mã được từ JSON")
                }
                InvokeResponseBody::Raw(_) => panic!("JobEvent phải serialize thành JSON"),
            };
            received_for_cb.lock().unwrap().push(event);
            Ok(())
        });
        (channel, received)
    }

    async fn wait_until_empty(handle: &JobRegistryHandle, within: Duration) {
        tokio::time::timeout(within, async {
            loop {
                if handle.snapshot().await.unwrap().is_empty() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("registry phải rỗng trong thời hạn");
    }

    async fn wait_until_state(handle: &JobRegistryHandle, job_id: JobId, state: JobState) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let jobs = handle.snapshot().await.unwrap();
                if jobs
                    .iter()
                    .any(|job| job.job_id == job_id && job.state == state)
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("Job phải đạt trạng thái mong đợi trong thời hạn");
    }

    #[tokio::test]
    async fn happy_path_commits_and_emits_a_result_event() {
        let root = tempdir().unwrap();
        let source = wav_fixture(root.path(), "a.wav");
        let transcriber = FakeTranscriber::new(vec![Behavior::Success("xin chào".to_string())]);
        let handle = registry_for_test(root.path(), transcriber);
        let (channel, received) = test_channel();
        handle.subscribe(channel).await.unwrap();

        let (job_id, session_id) = handle.start(start_params(source, "hash-a")).await.unwrap();
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        let events = received.lock().unwrap().clone();
        let result = events.iter().find_map(|event| match event {
            JobEvent::Result {
                job_id: id,
                session_id: sid,
                ..
            } if *id == job_id => Some(*sid),
            _ => None,
        });
        assert_eq!(result, Some(session_id));

        // A committed row really exists, with the merged text segment.
        let db = Db::open(root.path()).unwrap();
        db.with_connection(|conn| {
            let row = repo::sessions::get(conn, session_id)?.unwrap();
            assert_eq!(row.status, "complete");
            assert_eq!(row.source_hash.as_deref(), Some("hash-a"));
            Ok(())
        })
        .unwrap();
        assert!(!paths::staging_dir(root.path(), job_id).exists());
    }

    #[tokio::test]
    async fn a_waiting_quota_signal_from_the_transcriber_is_surfaced_on_the_job_snapshot() {
        let root = tempdir().unwrap();
        let source = wav_fixture(root.path(), "a.wav");
        let transcriber = FakeTranscriber::new(vec![Behavior::SignalWaitingQuotaThenSucceed(
            "xin chào".to_string(),
        )]);
        let handle = registry_for_test(root.path(), transcriber);
        let (channel, received) = test_channel();
        handle.subscribe(channel).await.unwrap();

        let (job_id, _) = handle
            .start(start_params(source, "hash-waiting"))
            .await
            .unwrap();
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        let events = received.lock().unwrap().clone();
        let saw_waiting_quota = events.iter().any(|event| {
            matches!(
                event,
                JobEvent::Updated { job, .. } if job.job_id == job_id && job.waiting_quota
            )
        });
        assert!(
            saw_waiting_quota,
            "phải có ít nhất một JobEvent::Updated với waitingQuota=true, thực tế: {events:?}"
        );
    }

    #[tokio::test]
    async fn a_second_start_is_queued_until_the_first_job_leaves_the_registry() {
        let root = tempdir().unwrap();
        let source_a = wav_fixture(root.path(), "a.wav");
        let source_b = wav_fixture(root.path(), "b.wav");
        let transcriber = FakeTranscriber::new(vec![Behavior::BlockUntilCancelled]);
        let handle = registry_for_test(root.path(), transcriber);

        let (job_a, _) = handle
            .start(start_params(source_a, "hash-a"))
            .await
            .unwrap();
        wait_until_state(&handle, job_a, JobState::Running).await;
        let (job_b, _) = handle
            .start(start_params(source_b, "hash-b"))
            .await
            .unwrap();

        let jobs = handle.snapshot().await.unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].job_id, job_a);
        assert_eq!(jobs[0].state, JobState::Running);
        assert_eq!(jobs[1].job_id, job_b);
        assert_eq!(jobs[1].state, JobState::Queued);

        // Cancelling the queued Job removes it from the queue immediately,
        // the running Job is unaffected (spec I/O Matrix "Nhiều Job").
        assert_eq!(
            handle.cancel(job_b).await.unwrap(),
            CancelOutcome::Cancelling
        );
        let jobs = handle.snapshot().await.unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].job_id, job_a);

        handle.cancel(job_a).await.unwrap();
        wait_until_empty(&handle, Duration::from_secs(2)).await;
    }

    #[tokio::test]
    async fn cancelling_a_running_job_stops_within_two_seconds_with_no_row_and_no_staging() {
        let root = tempdir().unwrap();
        let source = wav_fixture(root.path(), "a.wav");
        let transcriber = FakeTranscriber::new(vec![Behavior::BlockUntilCancelled]);
        let handle = registry_for_test(root.path(), transcriber);
        let (channel, received) = test_channel();
        handle.subscribe(channel).await.unwrap();

        let (job_id, session_id) = handle
            .start(start_params(source, "hash-cancel"))
            .await
            .unwrap();
        wait_until_state(&handle, job_id, JobState::Running).await;

        let start = std::time::Instant::now();
        assert_eq!(
            handle.cancel(job_id).await.unwrap(),
            CancelOutcome::Cancelling
        );
        wait_until_empty(&handle, Duration::from_secs(2)).await;
        assert!(
            start.elapsed() <= Duration::from_secs(2),
            "huỷ phải có hiệu lực trong vòng 2 giây"
        );

        let events = received.lock().unwrap().clone();
        assert!(events
            .iter()
            .any(|event| matches!(event, JobEvent::Cancelled { job_id: id, .. } if *id == job_id)));

        let db = Db::open(root.path()).unwrap();
        db.with_connection(|conn| {
            assert!(repo::sessions::get(conn, session_id)?.is_none());
            Ok(())
        })
        .unwrap();
        assert!(!paths::staging_dir(root.path(), job_id).exists());
    }

    #[tokio::test]
    async fn cancel_after_the_job_already_finished_reports_already_finished() {
        let root = tempdir().unwrap();
        let source = wav_fixture(root.path(), "a.wav");
        let transcriber = FakeTranscriber::new(vec![Behavior::Success("done".to_string())]);
        let handle = registry_for_test(root.path(), transcriber);

        let (job_id, _) = handle
            .start(start_params(source, "hash-done"))
            .await
            .unwrap();
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        assert_eq!(
            handle.cancel(job_id).await.unwrap(),
            CancelOutcome::AlreadyFinished
        );
        assert_eq!(
            handle.cancel(JobId::new()).await.unwrap(),
            CancelOutcome::AlreadyFinished
        );
    }

    #[tokio::test]
    async fn non_fatal_chunk_failure_becomes_a_gap_and_still_commits_partial() {
        let root = tempdir().unwrap();
        let source = wav_fixture(root.path(), "a.wav");
        let transcriber = FakeTranscriber::new(vec![Behavior::Fail {
            code: Code::Network,
            retryable: true,
        }]);
        let handle = registry_for_test(root.path(), transcriber);
        let (job_id, session_id) = handle
            .start(start_params(source, "hash-gap"))
            .await
            .unwrap();
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        let db = Db::open(root.path()).unwrap();
        db.with_connection(|conn| {
            let status: String = conn.query_row(
                "SELECT status FROM transcripts WHERE session_id = ?1",
                [session_id.to_string()],
                |row| row.get(0),
            )?;
            assert_eq!(status, "partial");
            let gap_reason: Option<String> = conn.query_row(
                "SELECT gap_reason FROM segments s JOIN transcripts t ON s.transcript_id = t.id \
                 WHERE t.session_id = ?1",
                [session_id.to_string()],
                |row| row.get(0),
            )?;
            assert_eq!(gap_reason.as_deref(), Some("chunk_failed"));
            Ok(())
        })
        .unwrap();
        assert!(!paths::staging_dir(root.path(), job_id).exists());
    }

    #[tokio::test]
    async fn a_fatal_chunk_error_stops_the_job_without_committing() {
        let root = tempdir().unwrap();
        let source = wav_fixture(root.path(), "a.wav");
        let transcriber = FakeTranscriber::new(vec![Behavior::Fail {
            code: Code::Auth,
            retryable: false,
        }]);
        let handle = registry_for_test(root.path(), transcriber.clone());
        let (channel, received) = test_channel();
        handle.subscribe(channel).await.unwrap();

        let (job_id, session_id) = handle
            .start(start_params(source, "hash-fatal"))
            .await
            .unwrap();
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        assert_eq!(
            transcriber.call_count(),
            1,
            "một Chunk lỗi hệ thống phải dừng Job ngay, không thử Chunk khác"
        );
        let events = received.lock().unwrap().clone();
        assert!(events.iter().any(
            |event| matches!(event, JobEvent::Error { job_id: id, error, .. } if *id == job_id && error.category == Category::Auth)
        ));

        let db = Db::open(root.path()).unwrap();
        db.with_connection(|conn| {
            assert!(repo::sessions::get(conn, session_id)?.is_none());
            Ok(())
        })
        .unwrap();
        assert!(!paths::staging_dir(root.path(), job_id).exists());
    }

    #[tokio::test]
    async fn subscribing_twice_each_gets_a_snapshot_then_contiguous_increasing_seq() {
        let root = tempdir().unwrap();
        let source = wav_fixture(root.path(), "a.wav");
        let transcriber = FakeTranscriber::new(vec![Behavior::BlockUntilCancelled]);
        let handle = registry_for_test(root.path(), transcriber);

        let (job_id, _) = handle
            .start(start_params(source, "hash-seq"))
            .await
            .unwrap();
        wait_until_state(&handle, job_id, JobState::Running).await;

        let (channel_a, received_a) = test_channel();
        handle.subscribe(channel_a).await.unwrap();
        let (channel_b, received_b) = test_channel();
        handle.subscribe(channel_b).await.unwrap();

        handle.cancel(job_id).await.unwrap();
        wait_until_empty(&handle, Duration::from_secs(2)).await;

        for received in [received_a, received_b] {
            let events = received.lock().unwrap().clone();
            assert!(matches!(events.first(), Some(JobEvent::Snapshot { .. })));
            let seqs: Vec<u32> = events.iter().map(JobEvent::seq).collect();
            for pair in seqs.windows(2) {
                assert!(
                    pair[1] > pair[0],
                    "seq phải tăng đơn điệu, thực tế: {seqs:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn is_busy_reflects_the_pre_allocated_session_id_of_a_running_job() {
        let root = tempdir().unwrap();
        let source = wav_fixture(root.path(), "a.wav");
        let transcriber = FakeTranscriber::new(vec![Behavior::BlockUntilCancelled]);
        let handle = registry_for_test(root.path(), transcriber);

        let (job_id, session_id) = handle
            .start(start_params(source, "hash-busy"))
            .await
            .unwrap();
        wait_until_state(&handle, job_id, JobState::Running).await;

        assert!(handle.is_busy(session_id).await.unwrap());
        assert!(!handle.is_busy(SessionId::new()).await.unwrap());

        handle.cancel(job_id).await.unwrap();
        wait_until_empty(&handle, Duration::from_secs(2)).await;
        assert!(!handle.is_busy(session_id).await.unwrap());
    }

    #[test]
    fn estimate_chunk_count_rounds_up_and_never_zero() {
        assert_eq!(estimate_chunk_count(0), 1);
        assert_eq!(estimate_chunk_count(1), 1);
        assert_eq!(estimate_chunk_count(300_000), 1);
        assert_eq!(estimate_chunk_count(300_001), 2);
        assert_eq!(estimate_chunk_count(600_000), 2);
    }
}
