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
use crate::core::id::{JobId, SessionId, TranscriptId};
use crate::core::paths;
use crate::db::repo::segments::{SegmentDraft, SegmentRow};
use crate::db::Db;
use crate::gemini::{CancellationToken, ConsentSnapshot, GeminiGateway, JobObserver};
use crate::library::store::{self, FileSessionDraft, SessionDraft, TranscriptDraft};
use crate::media::{self, Chunk, ChunkBudget, ChunkOptions, Chunker};
use crate::settings;
use crate::settings::TranscribeLanguage;
use crate::transcribe::adapter::{transcribe_chunk_observed, TranscribeFailure};
use crate::transcribe::merge::{splice_rerun, MergeBuilder};
use crate::transcribe::parser::ChunkTranscript;
use crate::transcribe::rerun::{decode_ranges_and_chunk, RerunRange};

use super::job::{CancelOutcome, JobEvent, JobKind, JobSnapshot, JobState};

const CHUNK_CHANNEL_CAPACITY: usize = 2;
const COMMAND_CHANNEL_CAPACITY: usize = 256;

/// Build the `ChunkOptions` a Job pipeline decodes with, from the
/// `chunkMinutes` captured on its params at start (spec Approach: "Job
/// transcribe file và Chạy lại chụp `chunkMinutes` ... lúc nhận Job"). The
/// byte/serialized-size budget itself is never configurable — only the
/// duration bound (spec Never: "Không đổi ngân sách inline/tự chia Chunk quá
/// lớn của `media::chunk`").
fn chunk_options_for(chunk_minutes: u32) -> ChunkOptions {
    ChunkOptions {
        max_duration_seconds: u64::from(chunk_minutes) * 60,
        budget: ChunkBudget::default(),
    }
}

fn actor_error() -> AppError {
    AppError::new(Code::Storage, "job registry actor unavailable")
}

fn cancelled_pipeline_error() -> AppError {
    AppError::new(Code::Blocked, "transcribe job was cancelled")
}

fn source_changed_error() -> AppError {
    AppError::new(
        Code::Storage,
        "the source file changed since it was selected",
    )
}

/// Hash lại `source_path` và so với `expected_hash` (spec 2.8 Always: "Job
/// ... hash lại nguồn trước probe/decode và hash lại lần nữa sau decode
/// trước commit; thiếu file hoặc hash khác → Job `error`, không commit,
/// không lưu Phiên"). Chạy trong `spawn_blocking` vì `sha256_file` là I/O
/// chặn; một file bị xoá tự nhiên rơi vào nhánh lỗi của `sha256_file` (category
/// `Storage`, giống `source_changed_error` — cùng category nên
/// `run_job_inner` không cần phân biệt hai nhánh này ở caller).
async fn verify_source_hash(source_path: &Path, expected_hash: &str) -> Result<(), AppError> {
    let path = source_path.to_path_buf();
    match tokio::task::spawn_blocking(move || media::sha256_file(&path)).await {
        Ok(Ok(actual)) if actual == expected_hash => Ok(()),
        Ok(Ok(_)) => Err(source_changed_error()),
        Ok(Err(err)) => Err(err),
        Err(_) => Err(actor_error()),
    }
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
    /// Chụp từ `settings::load` lúc nhận Job (spec Approach: "Job transcribe
    /// file và Chạy lại chụp `model` + ngôn ngữ ... lúc nhận Job"); đổi
    /// Settings giữa chừng không ảnh hưởng Job đang chạy/chờ.
    pub language: TranscribeLanguage,
    /// Chụp từ `settings.chunkMinutes` cùng lúc, cùng cách bất biến với
    /// `language` phía trên.
    pub chunk_minutes: u32,
    pub consent: ConsentSnapshot,
}

/// Đã giải quyết đầy đủ trước khi tạo Job Chạy lại (2.5) -- `ipc::` đã kiểm
/// Consent, tra Phiên/transcript thuộc Phiên, chặn `primary` của Phiên
/// `live`, giải `scope` thành `ranges`, kiểm Proxy tồn tại, và có key dùng
/// được (spec Always: "Rust tự tra range từ DB và kiểm transcript thuộc
/// `session_id` — không nhận range từ UI"). `transcript_id` là transcript
/// `primary` đang nhắm swap -- `expected_transcript_id` trong
/// `store::swap_transcript`.
#[derive(Debug, Clone)]
pub struct RerunParams {
    pub session_id: SessionId,
    pub transcript_id: TranscriptId,
    /// Vùng cần transcribe lại, tăng dần, không chồng lấp. Rỗng chỉ có thể
    /// xảy ra nếu caller gọi sai (ipc đã lọc `NothingToRerun` trước khi tới
    /// đây) -- pipeline coi rỗng là hoàn tất ngay, không gửi Chunk nào.
    pub ranges: Vec<RerunRange>,
    /// `true` cho scope `all`: bỏ hẳn Segment cũ thay vì giữ phần ngoài
    /// vùng (không có "ngoài vùng" khi vùng đã phủ hết Proxy).
    pub discard_old: bool,
    pub proxy_path: PathBuf,
    pub model: String,
    /// Cùng ngữ nghĩa chụp-lúc-nhận-Job với [`StartParams::language`].
    pub language: TranscribeLanguage,
    /// Cùng ngữ nghĩa chụp-lúc-nhận-Job với [`StartParams::chunk_minutes`].
    pub chunk_minutes: u32,
    pub consent: ConsentSnapshot,
}

/// Trả về từ `JobRegistryHandle::start_rerun` (spec Always: "Mỗi Phiên tối
/// đa một Job Chạy lại đang chờ/chạy: gọi lại trả Job hiện có").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RerunOutcome {
    Started { job_id: JobId },
    Existing { job_id: JobId },
}

/// Trả về từ `JobRegistryHandle::start` (spec 2.8 Always: "Tạo Job trong
/// actor là nguyên tử: nếu cùng lúc đã có Job Transcribe chờ/chạy cùng
/// `source_hash` thì trả Job đó, không tạo Job thứ hai"). Việc kiểm tra và
/// tạo entry mới xảy ra trong cùng một lệnh actor (`handle_start`) nên hai
/// lời gọi `start` đồng thời cùng `source_hash` luôn thấy đúng một Job —
/// không có khoảng hở giữa "tra" và "tạo" (khác với
/// [`JobRegistryHandle::find_transcribe_by_hash`], vốn chỉ là một snapshot
/// đọc dùng để bỏ qua `probe` sớm ở gate `ipc::`, không phải nguồn thật của
/// tính nguyên tử).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartOutcome {
    Started {
        job_id: JobId,
        session_id: SessionId,
    },
    Existing {
        job_id: JobId,
        session_id: SessionId,
    },
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
        language: TranscribeLanguage,
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
        language: TranscribeLanguage,
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
                language,
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
    pub async fn start(&self, params: StartParams) -> Result<StartOutcome, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Start { params, reply })
            .await
            .map_err(|_| actor_error())?;
        response.await.map_err(|_| actor_error())
    }

    /// Tra xem đã có Job Transcribe nào đang chờ/chạy cùng `source_hash` hay
    /// chưa — một snapshot đọc dùng ở gate `ipc::decide_transcribe_start`
    /// (spec Always: "tra reservation JobRegistry (trùng → `ExistingJob`)")
    /// để bỏ qua `probe`/key cho file trùng. Không phải nguồn thật của tính
    /// nguyên tử — `start` tự kiểm lại bên trong cùng một lệnh actor trước
    /// khi tạo entry mới, nên một race giữa lần tra này và lúc gọi `start`
    /// vẫn không thể tạo ra hai Job cho cùng nội dung.
    pub async fn find_transcribe_by_hash(
        &self,
        source_hash: String,
    ) -> Result<Option<(JobId, SessionId)>, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::FindByHash { source_hash, reply })
            .await
            .map_err(|_| actor_error())?;
        response.await.map_err(|_| actor_error())
    }

    /// Chạy lại: bắt đầu (hoặc, nếu Phiên đã có một Job Chạy lại đang
    /// chờ/chạy, trả về Job đó -- spec Always) một Job loại `Rerun` trong
    /// đúng hàng đợi dùng chung với Job transcribe file.
    pub async fn start_rerun(&self, params: RerunParams) -> Result<RerunOutcome, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::StartRerun { params, reply })
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
        reply: oneshot::Sender<StartOutcome>,
    },
    FindByHash {
        source_hash: String,
        reply: oneshot::Sender<Option<(JobId, SessionId)>>,
    },
    StartRerun {
        params: RerunParams,
        reply: oneshot::Sender<RerunOutcome>,
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
    /// `Some(hash)` cho Job `JobKind::Transcribe` (dùng làm khoá reservation
    /// — spec Always: "JobRegistry giữ reservation theo `source_hash`");
    /// luôn `None` cho `JobKind::Rerun`, vốn không nhận file nguồn mới.
    source_hash: Option<String>,
    kind: JobKind,
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
            kind: self.kind,
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
    /// Same lifecycle as `pending_params`, for Job loại Chạy lại (2.5). A
    /// given `JobId` only ever appears in one of the two maps — `kind` on
    /// its `JobEntry` says which.
    pending_rerun_params: HashMap<JobId, RerunParams>,
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
        pending_rerun_params: HashMap::new(),
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
            Command::FindByHash { source_hash, reply } => {
                let _ = reply.send(self.find_transcribe_job_by_hash(&source_hash));
            }
            Command::StartRerun { params, reply } => self.handle_start_rerun(params, reply),
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

    /// Nguyên tử theo `source_hash` (spec 2.8 Always: "Tạo Job trong actor là
    /// nguyên tử: nếu cùng lúc đã có Job Transcribe chờ/chạy cùng
    /// `source_hash` thì trả Job đó, không tạo Job thứ hai"). Kiểm tra và tạo
    /// entry mới xảy ra trong cùng lệnh actor này (`&mut self`, không có
    /// `.await` ở giữa) nên hai lời gọi `start` đồng thời cùng nội dung luôn
    /// thấy đúng một Job dù `ipc::decide_transcribe_start` đã tra trước đó
    /// (`find_transcribe_by_hash`) không thấy trùng.
    fn handle_start(&mut self, params: StartParams, reply: oneshot::Sender<StartOutcome>) {
        if let Some((job_id, session_id)) = self.find_transcribe_job_by_hash(&params.source_hash) {
            let _ = reply.send(StartOutcome::Existing { job_id, session_id });
            return;
        }

        let job_id = JobId::new();
        let session_id = SessionId::new();
        let will_run_now = self.order.is_empty();
        let entry = JobEntry {
            session_id,
            source_name: params.source_name.clone(),
            source_hash: Some(params.source_hash.clone()),
            kind: JobKind::Transcribe,
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
        let _ = reply.send(StartOutcome::Started { job_id, session_id });
        self.broadcast_updated(job_id);

        if will_run_now {
            self.start_pipeline_for(job_id);
        }
    }

    /// Job `Transcribe` đang chờ/chạy (còn trong `self.jobs`) với cùng
    /// `source_hash` — dùng cả bởi `handle_start` (kiểm nguyên tử trước khi
    /// tạo entry mới) lẫn `Command::FindByHash` (snapshot đọc cho gate
    /// `ipc::`). Entry rời `self.jobs` ngay khi Job commit/huỷ/lỗi
    /// (`handle_finished`), nên reservation tự giải phóng theo đúng vòng đời
    /// đó — không có sổ sách riêng nào khác phải dọn.
    fn find_transcribe_job_by_hash(&self, source_hash: &str) -> Option<(JobId, SessionId)> {
        self.jobs.iter().find_map(|(job_id, entry)| {
            (entry.kind == JobKind::Transcribe && entry.source_hash.as_deref() == Some(source_hash))
                .then_some((*job_id, entry.session_id))
        })
    }

    /// Chạy lại (2.5): trả `Existing` nếu Phiên đã có một Job Chạy lại đang
    /// chờ/chạy (spec Always: "Mỗi Phiên tối đa một Job Chạy lại đang
    /// chờ/chạy"), ngược lại tạo Job mới trong cùng hàng đợi.
    fn handle_start_rerun(&mut self, params: RerunParams, reply: oneshot::Sender<RerunOutcome>) {
        let existing = self.jobs.iter().find_map(|(job_id, entry)| {
            (entry.session_id == params.session_id && entry.kind == JobKind::Rerun)
                .then_some(*job_id)
        });
        if let Some(job_id) = existing {
            let _ = reply.send(RerunOutcome::Existing { job_id });
            return;
        }

        let job_id = JobId::new();
        let session_id = params.session_id;
        let will_run_now = self.order.is_empty();
        let entry = JobEntry {
            session_id,
            source_name: None,
            source_hash: None,
            kind: JobKind::Rerun,
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
        self.pending_rerun_params.insert(job_id, params);
        let _ = reply.send(RerunOutcome::Started { job_id });
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
        self.pending_rerun_params.remove(&job_id);
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

    /// Spawn the pipeline task for `job_id`, consuming its stored params (a
    /// `StartParams` for `JobKind::Transcribe`, a `RerunParams` for
    /// `JobKind::Rerun` — never both). A no-op if the Job or its params are
    /// gone (already cancelled/removed between being queued and being
    /// promoted).
    fn start_pipeline_for(&mut self, job_id: JobId) {
        let Some(entry) = self.jobs.get(&job_id) else {
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

        if let Some(params) = self.pending_params.remove(&job_id) {
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
            return;
        }

        let Some(params) = self.pending_rerun_params.remove(&job_id) else {
            return;
        };
        tokio::spawn(async move {
            let outcome = run_rerun_job(
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

fn consent_revoked_error() -> AppError {
    // Content-free: never logs/echoes the consent values themselves, only
    // that the Job is stopping because of them.
    AppError::new(Code::Blocked, "consent is no longer current")
}

/// Re-reads Settings from `db` and checks whether consent is still current
/// (spec Boundaries Always P0 review: "Consent is re-read from settings
/// before every chunk send (file transcribe and rerun). If it is no longer
/// current, the Job stops with a `Code::Blocked` error and does not commit").
/// Only ever called from the registry's per-chunk loops -- never inside the
/// gateway retry loop (spec Never: "no re-check of consent inside the
/// gateway retry loop; per-chunk bound is accepted").
async fn consent_still_current(db: &Arc<Db>) -> Result<bool, AppError> {
    let db = db.clone();
    match tokio::task::spawn_blocking(move || settings::load(&db)).await {
        Ok(loaded) => Ok(
            ConsentSnapshot::new(loaded.consent_accepted_version, loaded.consent_declined)
                .is_current(),
        ),
        Err(_) => Err(actor_error()),
    }
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

/// Estimate the number of `chunk_minutes`-long chunks a file of `total_ms`
/// will produce. Only used for progress display — the real Chunker may emit
/// more (rare oversized-payload splitting), in which case the actor simply
/// raises this estimate to match (`Internal::Progress` handling).
fn estimate_chunk_count(total_ms: u64, chunk_minutes: u32) -> u32 {
    let chunk_ms = u64::from(chunk_minutes) * 60 * 1000;
    (total_ms.div_ceil(chunk_ms.max(1))).max(1) as u32
}

/// Run one Job's full pipeline. Never panics: every fallible step is turned
/// into a [`JobOutcome`] instead of propagating a `Result` up, since this
/// function's only caller is the `tokio::spawn`ed task in
/// `JobRegistryActor::start_pipeline_for`, which has nothing to `?` into.
#[allow(clippy::too_many_arguments)]
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

#[allow(clippy::too_many_arguments)]
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

    // Hash lại nguồn trước probe/decode (spec 2.8 Always: "Job khi bắt đầu
    // chạy (kể cả sau khi chờ trong hàng) hash lại nguồn trước probe/decode
    // ... thiếu file hoặc hash khác → Job `error`, không commit, không lưu
    // Phiên") — bắt được trường hợp nguồn bị xoá/sửa trong lúc Job trước đó
    // còn đang chạy.
    if let Err(err) = verify_source_hash(&params.source_path, &params.source_hash).await {
        return JobOutcome::Error(err);
    }

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
    let chunk_count = estimate_chunk_count(total_ms, params.chunk_minutes);
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
        db,
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

    // Hash lại lần nữa ngay trước commit (spec 2.8 Always) — nguồn có thể đã
    // đổi/mất trong lúc decode/transcribe chạy (có thể mất nhiều phút);
    // không bao giờ commit một Phiên với `source_hash` không còn đúng nữa.
    if let Err(err) = verify_source_hash(&params.source_path, &params.source_hash).await {
        return JobOutcome::Error(err);
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
            language: params.language.as_code().map(str::to_string),
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
#[allow(clippy::too_many_arguments)]
async fn decode_and_transcribe(
    job_id: JobId,
    params: &StartParams,
    total_ms: u64,
    initial_chunk_count: u32,
    db: &Arc<Db>,
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
    let chunk_options = chunk_options_for(params.chunk_minutes);
    let decode_handle = tokio::task::spawn_blocking(move || {
        let decode_result: Result<(), AppError> = (|| {
            let mut chunker = Chunker::new(chunk_options)?;
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

        // Re-read consent from Settings right before this chunk is sent
        // (spec Boundaries Always P0 review) -- a revocation that lands
        // between two chunk sends must stop the Job before any more audio
        // leaves the machine, never merely at the next Job start.
        match consent_still_current(db).await {
            Ok(true) => {}
            Ok(false) => {
                outcome = Some(JobOutcome::Error(consent_revoked_error()));
                break;
            }
            Err(err) => {
                outcome = Some(JobOutcome::Error(err));
                break;
            }
        }

        let observer: Arc<dyn JobObserver> = Arc::new(RegistryObserver {
            handle: handle.clone(),
            job_id,
        });
        let result = transcriber
            .transcribe(
                params.model.clone(),
                chunk.clone(),
                params.language,
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
    // A panic in the decode task (`JoinError`) must turn the Job into an
    // error, never a silent commit of whatever was merged so far (spec
    // Boundaries Always P0 review) -- but only when nothing has already
    // decided this Job's outcome (a cancellation or a fatal transcribe error
    // already found above takes precedence).
    if decode_handle.await.is_err() && outcome.is_none() {
        outcome = Some(JobOutcome::Error(actor_error()));
    }

    (merge.finish(), outcome)
}

fn segment_row_to_draft(row: SegmentRow) -> SegmentDraft {
    SegmentDraft {
        start_sec: row.start_sec,
        end_sec: row.end_sec,
        kind: row.kind,
        gap_reason: row.gap_reason,
        text: row.text,
        speaker: row.speaker,
    }
}

/// Run one Chạy lại Job's full pipeline (spec Approach). Same never-panics
/// contract as [`run_job`]: every fallible step becomes a [`JobOutcome`].
/// Chạy lại never creates its own staging directory (source audio is always
/// the already-published Proxy — spec Boundaries: "không cần file nguồn"),
/// but `discard_staging` is still called for symmetry with `run_job`; it is
/// a harmless no-op when nothing was ever staged under `job_id`.
#[allow(clippy::too_many_arguments)]
async fn run_rerun_job(
    job_id: JobId,
    session_id: SessionId,
    params: &RerunParams,
    db: &Arc<Db>,
    root: &Path,
    transcriber: &Arc<dyn ChunkTranscriber>,
    cancel: &CancellationToken,
    handle: &JobRegistryHandle,
) -> JobOutcome {
    let outcome =
        run_rerun_job_inner(job_id, session_id, params, db, transcriber, cancel, handle).await;
    if let Err(err) = store::discard_staging(root, job_id) {
        tracing::warn!(error = %err, "không dọn được staging sau khi Job Chạy lại kết thúc");
    }
    outcome
}

async fn run_rerun_job_inner(
    job_id: JobId,
    session_id: SessionId,
    params: &RerunParams,
    db: &Arc<Db>,
    transcriber: &Arc<dyn ChunkTranscriber>,
    cancel: &CancellationToken,
    handle: &JobRegistryHandle,
) -> JobOutcome {
    if cancel.is_cancelled() {
        return JobOutcome::Cancelled;
    }

    let total_ms: u64 = params.ranges.iter().map(RerunRange::duration_ms).sum();
    let chunk_count = estimate_chunk_count(total_ms.max(1), params.chunk_minutes);
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

    let (per_range_segments, decode_outcome) = decode_and_transcribe_ranges(
        job_id,
        &params.model,
        params.language,
        params.chunk_minutes,
        params.consent,
        &params.proxy_path,
        &params.ranges,
        db,
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

    let discard_old = params.discard_old;
    let expected_transcript_id = params.transcript_id;
    let read_db = db.clone();
    let old_drafts = tokio::task::spawn_blocking(move || -> Result<Vec<SegmentDraft>, AppError> {
        if discard_old {
            return Ok(Vec::new());
        }
        read_db.with_connection(|conn| {
            let rows =
                crate::db::repo::segments::list_for_transcript(conn, expected_transcript_id)?;
            Ok(rows.into_iter().map(segment_row_to_draft).collect())
        })
    })
    .await;
    let old_drafts = match old_drafts {
        Ok(Ok(drafts)) => drafts,
        Ok(Err(err)) => return JobOutcome::Error(err),
        Err(_) => return JobOutcome::Error(actor_error()),
    };

    // An open-ended tail range (`end_sample == u64::MAX`, spec Boundaries
    // Always) consumed real audio past its nominal `end_ms` -- its
    // seconds-range for `splice_rerun` must extend to true infinity too, or
    // `clamp_to_range`/`trim_outside_ranges` would clip new/old segments back
    // down to the nominal (possibly short) boundary and lose the tail again.
    let ranges_sec: Vec<(f64, f64)> = params
        .ranges
        .iter()
        .map(|range| {
            let end_sec = if range.end_sample == u64::MAX {
                f64::INFINITY
            } else {
                range.end_ms as f64 / 1000.0
            };
            (range.start_ms as f64 / 1000.0, end_sec)
        })
        .collect();
    let merged = splice_rerun(old_drafts, &ranges_sec, per_range_segments);

    let db = db.clone();
    let model = params.model.clone();
    let language = params.language.as_code().map(str::to_string);
    let commit = tokio::task::spawn_blocking(move || {
        store::swap_transcript(
            &db,
            session_id,
            expected_transcript_id,
            TranscriptDraft {
                model,
                language,
                segments: merged,
            },
        )
    })
    .await;
    match commit {
        Ok(Ok(Some(_new_transcript_id))) => JobOutcome::Committed(session_id),
        Ok(Ok(None)) => JobOutcome::Error(AppError::new(
            Code::Request,
            "Transcript đích đã bị thay trong lúc Chạy lại",
        )),
        Ok(Err(err)) => JobOutcome::Error(err),
        Err(_) => JobOutcome::Error(actor_error()),
    }
}

/// Decode Proxy một lần (qua `rerun::decode_ranges_and_chunk`), transcribe
/// tuần tự từng Chunk của từng vùng, gộp riêng theo vùng bằng một
/// `MergeBuilder` mỗi vùng. Trả về Segment đã gộp của từng vùng (theo đúng
/// thứ tự `ranges`) cộng `Some(outcome)` khi cả Job phải dừng ở đây (huỷ,
/// hoặc lỗi hệ thống).
#[allow(clippy::too_many_arguments)]
async fn decode_and_transcribe_ranges(
    job_id: JobId,
    model: &str,
    language: TranscribeLanguage,
    chunk_minutes: u32,
    consent: ConsentSnapshot,
    proxy_path: &Path,
    ranges: &[RerunRange],
    db: &Arc<Db>,
    transcriber: &Arc<dyn ChunkTranscriber>,
    cancel: &CancellationToken,
    handle: &JobRegistryHandle,
) -> (Vec<Vec<SegmentDraft>>, Option<JobOutcome>) {
    if ranges.is_empty() {
        return (Vec::new(), None);
    }

    let (chunk_tx, mut chunk_rx) =
        mpsc::channel::<Result<(usize, Chunk), AppError>>(CHUNK_CHANNEL_CAPACITY);
    let cancel_for_decode = cancel.clone();
    let decode_path = proxy_path.to_path_buf();
    let decode_ranges = ranges.to_vec();
    let chunk_options = chunk_options_for(chunk_minutes);
    let decode_handle = tokio::task::spawn_blocking(move || {
        let decode_result = decode_ranges_and_chunk(
            &decode_path,
            &decode_ranges,
            chunk_options,
            &cancel_for_decode,
            |range_index, chunk| {
                if cancel_for_decode.is_cancelled() {
                    return Err(cancelled_pipeline_error());
                }
                chunk_tx
                    .blocking_send(Ok((range_index, chunk)))
                    .map_err(|_| cancelled_pipeline_error())
            },
        );
        if let Err(err) = decode_result {
            if !cancel_for_decode.is_cancelled() {
                let _ = chunk_tx.blocking_send(Err(err));
            }
        }
    });

    let range_offsets_ms: Vec<u64> = ranges
        .iter()
        .scan(0u64, |acc, range| {
            let offset = *acc;
            *acc += range.duration_ms();
            Some(offset)
        })
        .collect();
    let total_ms: u64 = ranges.iter().map(RerunRange::duration_ms).sum();

    let mut builders: Vec<MergeBuilder> = ranges.iter().map(|_| MergeBuilder::new()).collect();
    let mut chunk_index: u32 = 0;
    let mut chunk_count = estimate_chunk_count(total_ms.max(1), chunk_minutes);
    let mut outcome: Option<JobOutcome> = None;

    while let Some(item) = chunk_rx.recv().await {
        if cancel.is_cancelled() {
            outcome = Some(JobOutcome::Cancelled);
            break;
        }
        let (range_index, mut chunk) = match item {
            Ok(pair) => pair,
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
        let range = ranges[range_index];
        // `chunk.start_ms`/`start_sample` are still range-relative here (the
        // per-range Chunker starts at 0 — spec Design Notes) so they double
        // as "ms vùng đã xử lý" for progress before being shifted absolute.
        let processed_ms = range_offsets_ms[range_index] + chunk.start_ms + chunk.duration_ms;
        handle
            .send_internal(Internal::Progress {
                job_id,
                processed_ms,
                total_ms,
                chunk_index,
                chunk_count,
            })
            .await;

        chunk.start_ms += range.start_ms;
        chunk.start_sample += range.start_sample;

        // Re-read consent from Settings right before this chunk is sent,
        // same as the file-transcribe pipeline (spec Boundaries Always P0
        // review).
        match consent_still_current(db).await {
            Ok(true) => {}
            Ok(false) => {
                outcome = Some(JobOutcome::Error(consent_revoked_error()));
                break;
            }
            Err(err) => {
                outcome = Some(JobOutcome::Error(err));
                break;
            }
        }

        let observer: Arc<dyn JobObserver> = Arc::new(RegistryObserver {
            handle: handle.clone(),
            job_id,
        });
        let result = transcriber
            .transcribe(
                model.to_string(),
                chunk.clone(),
                language,
                consent,
                cancel.clone(),
                observer,
            )
            .await;
        match result {
            Ok(transcript) => builders[range_index].push_success(transcript),
            Err(failure) => {
                if cancel.is_cancelled() {
                    outcome = Some(JobOutcome::Cancelled);
                    break;
                }
                if is_fatal(&failure.error) {
                    outcome = Some(JobOutcome::Error(failure.error));
                    break;
                }
                builders[range_index].push_failed_chunk(&chunk);
            }
        }
    }

    drop(chunk_rx);
    // Same JoinError-must-not-commit contract as `decode_and_transcribe`
    // (spec Boundaries Always P0 review).
    if decode_handle.await.is_err() && outcome.is_none() {
        outcome = Some(JobOutcome::Error(actor_error()));
    }

    let per_range = builders.into_iter().map(MergeBuilder::finish).collect();
    (per_range, outcome)
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
        /// Every Chunk this fake was asked to transcribe, in call order —
        /// used by the Chạy lại tests to assert `start_ms` was already
        /// shifted absolute before reaching the transcriber (spec Acceptance
        /// Criteria: "chỉ đúng hai vùng được gửi (start_ms tuyệt đối)").
        chunks: Mutex<Vec<Chunk>>,
        /// Every `language` this fake was called with, in call order — story
        /// 2.6 test: "ngôn ngữ chụp được truyền tới transcriber".
        languages: Mutex<Vec<TranscribeLanguage>>,
    }

    impl FakeTranscriber {
        fn new(behaviors: Vec<Behavior>) -> Arc<Self> {
            Arc::new(Self {
                behaviors: Mutex::new(behaviors.into()),
                calls: Mutex::new(0),
                chunks: Mutex::new(Vec::new()),
                languages: Mutex::new(Vec::new()),
            })
        }

        fn call_count(&self) -> u32 {
            *self.calls.lock().unwrap()
        }

        fn languages_seen(&self) -> Vec<TranscribeLanguage> {
            self.languages.lock().unwrap().clone()
        }

        fn chunk_start_ms(&self) -> Vec<u64> {
            self.chunks
                .lock()
                .unwrap()
                .iter()
                .map(|chunk| chunk.start_ms)
                .collect()
        }
    }

    impl ChunkTranscriber for FakeTranscriber {
        fn transcribe(
            &self,
            _model: String,
            chunk: Chunk,
            language: TranscribeLanguage,
            _consent: ConsentSnapshot,
            cancellation: CancellationToken,
            observer: Arc<dyn JobObserver>,
        ) -> TranscribeChunkFuture {
            *self.calls.lock().unwrap() += 1;
            self.chunks.lock().unwrap().push(chunk.clone());
            self.languages.lock().unwrap().push(language);
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
    ///
    /// Content is deterministically varied by `name` (folded into the sine
    /// phase) so two fixtures created with different names never hash the
    /// same (spec 2.8: `run_job_inner` now re-hashes the real source before
    /// probe/decode and before commit, so `StartParams::source_hash` must be
    /// the *real* content hash — tests that want two distinct Jobs need two
    /// genuinely distinct files, not just distinct labels).
    fn wav_fixture(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let seed = name.bytes().map(u32::from).sum::<u32>() as f32;
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for frame in 0..1_600_i32 {
            let sample = (((frame as f32 + seed) * 0.05).sin() * 5_000.0) as i16;
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
        path
    }

    /// Builds `StartParams` for `path`, hashing its *real* current content
    /// (spec 2.8 Always: Job hash lại nguồn và so với `source_hash` trước
    /// probe/decode và trước commit — một hash bịa sẽ luôn bị coi là "nguồn
    /// đã đổi"). `label` no longer feeds `source_hash` — it stays as a
    /// call-site mnemonic only, kept so every existing `start_params(source,
    /// "hash-...")` call site reads the same as before.
    fn start_params(path: PathBuf, label: &str) -> StartParams {
        let _ = label;
        let source_hash = media::sha256_file(&path).expect("fixture phải hash được");
        StartParams {
            source_path: path,
            source_hash,
            source_name: Some("fixture.wav".to_string()),
            model: "gemini-flash-lite-latest".to_string(),
            language: TranscribeLanguage::Auto,
            chunk_minutes: 5,
            consent: ConsentSnapshot::new(1, false),
        }
    }

    /// Hầu hết test chỉ quan tâm trường hợp "vừa tạo Job mới" — helper này
    /// unwrap thẳng `StartOutcome::Started` và panic với thông điệp rõ ràng
    /// nếu một hash lặp lại vô tình trả `Existing` (bug test, không phải
    /// hành vi mong đợi ở các test không kiểm dedup).
    async fn start_new(handle: &JobRegistryHandle, params: StartParams) -> (JobId, SessionId) {
        match handle.start(params).await.unwrap() {
            StartOutcome::Started { job_id, session_id } => (job_id, session_id),
            StartOutcome::Existing { .. } => {
                panic!(
                    "start_new: expected StartOutcome::Started, got Existing (trùng source_hash?)"
                )
            }
        }
    }

    /// The registry now re-reads consent from Settings before every chunk
    /// send (P0 review fix). Every pipeline test below passes
    /// `ConsentSnapshot::new(1, false)` straight into
    /// `StartParams`/`RerunParams` without itself touching Settings, so this
    /// persists a currently-accepted consent into the test `Db` first --
    /// exactly what the real `ipc::` start gate already guarantees before
    /// ever calling `start`/`start_rerun`. A test that specifically wants to
    /// exercise a mid-Job revocation calls `settings::save` again (same
    /// on-disk file, WAL-shared) after the first chunk.
    fn seed_current_consent(db: &Db) {
        settings::save(
            db,
            &settings::Settings {
                consent_accepted_version: crate::consent::CURRENT_VERSION,
                consent_declined: false,
                ..Default::default()
            },
        )
        .unwrap();
    }

    fn registry_for_test(root: &Path, transcriber: Arc<dyn ChunkTranscriber>) -> JobRegistryHandle {
        let db = Arc::new(Db::open(root).unwrap());
        seed_current_consent(&db);
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
        let expected_hash = media::sha256_file(&source).unwrap();
        let transcriber = FakeTranscriber::new(vec![Behavior::Success("xin chào".to_string())]);
        let handle = registry_for_test(root.path(), transcriber);
        let (channel, received) = test_channel();
        handle.subscribe(channel).await.unwrap();

        let (job_id, session_id) = start_new(&handle, start_params(source, "hash-a")).await;
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
            assert_eq!(row.source_hash.as_deref(), Some(expected_hash.as_str()));
            Ok(())
        })
        .unwrap();
        assert!(!paths::staging_dir(root.path(), job_id).exists());
    }

    // ---------------------------------------------------------------------
    // Story 2.8: reservation theo `source_hash` — nguyên tử qua
    // `StartOutcome`, tự giải phóng khi Job rời registry, và hash lại nguồn
    // trước khi chạy (spec Always, Acceptance Criteria hàng 1).
    // ---------------------------------------------------------------------

    /// Acceptance Criteria hàng 1: "Given hai lời gọi `transcribe_start`
    /// đồng thời cùng nội dung mới, when cả hai hoàn tất, then đúng một Job
    /// tồn tại và cả hai trả cùng `jobId`."
    #[tokio::test]
    async fn concurrent_starts_with_the_same_source_hash_resolve_to_exactly_one_job() {
        let root = tempdir().unwrap();
        let source = wav_fixture(root.path(), "a.wav");
        let transcriber = FakeTranscriber::new(vec![Behavior::BlockUntilCancelled]);
        let handle = registry_for_test(root.path(), transcriber);
        let params = start_params(source, "hash-concurrent");

        let (outcome_a, outcome_b) =
            tokio::join!(handle.start(params.clone()), handle.start(params.clone()),);

        fn ids(outcome: StartOutcome) -> (JobId, SessionId) {
            match outcome {
                StartOutcome::Started { job_id, session_id } => (job_id, session_id),
                StartOutcome::Existing { job_id, session_id } => (job_id, session_id),
            }
        }
        let (job_a, session_a) = ids(outcome_a.unwrap());
        let (job_b, session_b) = ids(outcome_b.unwrap());

        assert_eq!(job_a, job_b, "đúng một Job cho cùng source_hash");
        assert_eq!(session_a, session_b);

        let jobs = handle.snapshot().await.unwrap();
        assert_eq!(jobs.len(), 1, "chỉ một Job tồn tại trong registry");

        handle.cancel(job_a).await.unwrap();
        wait_until_empty(&handle, Duration::from_secs(2)).await;
    }

    /// Gọi `start` lần hai trong khi lần đầu còn đang chạy (không phải
    /// đồng thời thật sự, nhưng cùng đường: `handle_start` phải tìm thấy
    /// entry đã có trước khi tạo mới) → vẫn đúng một Job.
    #[tokio::test]
    async fn a_second_start_while_the_first_is_still_running_returns_the_existing_job() {
        let root = tempdir().unwrap();
        let source = wav_fixture(root.path(), "a.wav");
        let transcriber = FakeTranscriber::new(vec![Behavior::BlockUntilCancelled]);
        let handle = registry_for_test(root.path(), transcriber);
        let params = start_params(source, "hash-repeat");

        let (job_1, session_1) = start_new(&handle, params.clone()).await;
        wait_until_state(&handle, job_1, JobState::Running).await;

        match handle.start(params).await.unwrap() {
            StartOutcome::Existing { job_id, session_id } => {
                assert_eq!(job_id, job_1);
                assert_eq!(session_id, session_1);
            }
            StartOutcome::Started { .. } => panic!("phải trả Existing, không tạo Job thứ hai"),
        }

        let jobs = handle.snapshot().await.unwrap();
        assert_eq!(jobs.len(), 1);

        handle.cancel(job_1).await.unwrap();
        wait_until_empty(&handle, Duration::from_secs(2)).await;
    }

    #[tokio::test]
    async fn reservation_is_released_after_commit_so_a_later_start_with_the_same_hash_creates_a_new_job(
    ) {
        let root = tempdir().unwrap();
        let source = wav_fixture(root.path(), "a.wav");
        let transcriber = FakeTranscriber::new(vec![
            Behavior::Success("một".to_string()),
            Behavior::Success("hai".to_string()),
        ]);
        let handle = registry_for_test(root.path(), transcriber);

        let (job_1, _) =
            start_new(&handle, start_params(source.clone(), "hash-commit-reuse")).await;
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        let (job_2, _) = start_new(&handle, start_params(source, "hash-commit-reuse")).await;
        assert_ne!(
            job_1, job_2,
            "reservation phải giải phóng sau khi Job trước commit xong"
        );
        wait_until_empty(&handle, Duration::from_secs(5)).await;
    }

    #[tokio::test]
    async fn reservation_is_released_after_cancel_so_a_later_start_with_the_same_hash_creates_a_new_job(
    ) {
        let root = tempdir().unwrap();
        let source = wav_fixture(root.path(), "a.wav");
        let transcriber = FakeTranscriber::new(vec![Behavior::BlockUntilCancelled]);
        let handle = registry_for_test(root.path(), transcriber.clone());

        let (job_1, _) =
            start_new(&handle, start_params(source.clone(), "hash-cancel-reuse")).await;
        // Chờ tới khi pipeline thật sự tới `transcriber.transcribe()` (chứ
        // không chỉ `JobState::Running`, vốn được set ngay khi tạo entry,
        // trước cả `probe`/hash lại nguồn) trước khi huỷ — nếu không, huỷ có
        // thể trúng một trong các điểm kiểm `cancel.is_cancelled()` sớm hơn,
        // để lại `Behavior::BlockUntilCancelled` chưa tiêu thụ trong hàng đợi
        // dùng chung của `FakeTranscriber`, khiến `job_2` phía dưới (dùng lại
        // cùng transcriber) treo mãi thay vì thấy `Behavior::Success` mặc định.
        tokio::time::timeout(Duration::from_secs(2), async {
            while transcriber.call_count() < 1 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("pipeline phải gọi transcriber trong thời hạn");
        handle.cancel(job_1).await.unwrap();
        wait_until_empty(&handle, Duration::from_secs(2)).await;

        let (job_2, _) = start_new(&handle, start_params(source, "hash-cancel-reuse")).await;
        assert_ne!(
            job_1, job_2,
            "reservation phải giải phóng sau khi Job trước bị huỷ"
        );
        wait_until_empty(&handle, Duration::from_secs(5)).await;
    }

    #[tokio::test]
    async fn reservation_is_released_after_a_fatal_error_so_a_later_start_with_the_same_hash_creates_a_new_job(
    ) {
        let root = tempdir().unwrap();
        let source = wav_fixture(root.path(), "a.wav");
        let transcriber = FakeTranscriber::new(vec![Behavior::Fail {
            code: Code::Auth,
            retryable: false,
        }]);
        let handle = registry_for_test(root.path(), transcriber);

        let (job_1, _) = start_new(&handle, start_params(source.clone(), "hash-error-reuse")).await;
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        let (job_2, _) = start_new(&handle, start_params(source, "hash-error-reuse")).await;
        assert_ne!(
            job_1, job_2,
            "reservation phải giải phóng sau khi Job trước lỗi"
        );
        wait_until_empty(&handle, Duration::from_secs(5)).await;
    }

    /// spec Always: "Job khi bắt đầu chạy ... hash lại nguồn trước
    /// probe/decode ... thiếu file hoặc hash khác → Job `error`, không
    /// commit, không lưu Phiên." — nguồn còn đó nhưng nội dung đã đổi so với
    /// lúc `source_hash` được tính (mô phỏng bằng một `source_hash` không
    /// khớp thật).
    #[tokio::test]
    async fn a_source_hash_mismatch_at_job_start_errors_without_committing_or_leaving_staging() {
        let root = tempdir().unwrap();
        let source = wav_fixture(root.path(), "a.wav");
        let params = StartParams {
            source_hash: "not-the-real-hash".to_string(),
            ..start_params(source, "unused")
        };
        let transcriber = FakeTranscriber::new(vec![]);
        let handle = registry_for_test(root.path(), transcriber.clone());
        let (channel, received) = test_channel();
        handle.subscribe(channel).await.unwrap();

        let (job_id, session_id) = start_new(&handle, params).await;
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        assert_eq!(
            transcriber.call_count(),
            0,
            "hash sai phải chặn trước cả probe/decode, không gọi transcriber"
        );
        let events = received.lock().unwrap().clone();
        assert!(events.iter().any(|event| matches!(
            event,
            JobEvent::Error { job_id: id, .. } if *id == job_id
        )));

        let db = Db::open(root.path()).unwrap();
        db.with_connection(|conn| {
            assert!(repo::sessions::get(conn, session_id)?.is_none());
            Ok(())
        })
        .unwrap();
        assert!(!paths::staging_dir(root.path(), job_id).exists());
    }

    /// Cùng gate, nhánh "thiếu file": nguồn bị xoá trước khi Job kịp chạy.
    #[tokio::test]
    async fn a_missing_source_file_at_job_start_errors_without_committing_or_leaving_staging() {
        let root = tempdir().unwrap();
        let source = wav_fixture(root.path(), "a.wav");
        let params = start_params(source.clone(), "hash-missing");
        std::fs::remove_file(&source).unwrap();

        let transcriber = FakeTranscriber::new(vec![]);
        let handle = registry_for_test(root.path(), transcriber.clone());
        let (channel, received) = test_channel();
        handle.subscribe(channel).await.unwrap();

        let (job_id, session_id) = start_new(&handle, params).await;
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        assert_eq!(transcriber.call_count(), 0);
        let events = received.lock().unwrap().clone();
        assert!(events.iter().any(|event| matches!(
            event,
            JobEvent::Error { job_id: id, .. } if *id == job_id
        )));

        let db = Db::open(root.path()).unwrap();
        db.with_connection(|conn| {
            assert!(repo::sessions::get(conn, session_id)?.is_none());
            Ok(())
        })
        .unwrap();
        assert!(!paths::staging_dir(root.path(), job_id).exists());
    }

    /// `find_transcribe_by_hash` là snapshot đọc dùng ở gate `ipc::` để bỏ
    /// qua `probe` sớm cho file trùng Job (spec Code Map) — không tự nó tạo
    /// hay xoá gì trong registry.
    #[tokio::test]
    async fn find_transcribe_by_hash_reports_a_running_job_and_nothing_once_it_is_gone() {
        let root = tempdir().unwrap();
        let source = wav_fixture(root.path(), "a.wav");
        let transcriber = FakeTranscriber::new(vec![Behavior::BlockUntilCancelled]);
        let handle = registry_for_test(root.path(), transcriber);
        let params = start_params(source, "hash-findbyhash");
        let hash = params.source_hash.clone();

        assert_eq!(
            handle.find_transcribe_by_hash(hash.clone()).await.unwrap(),
            None,
            "chưa start thì chưa có gì để tìm"
        );

        let (job_id, session_id) = start_new(&handle, params).await;
        assert_eq!(
            handle.find_transcribe_by_hash(hash.clone()).await.unwrap(),
            Some((job_id, session_id))
        );

        handle.cancel(job_id).await.unwrap();
        wait_until_empty(&handle, Duration::from_secs(2)).await;

        assert_eq!(
            handle.find_transcribe_by_hash(hash).await.unwrap(),
            None,
            "reservation phải biến mất cùng lúc Job rời registry"
        );
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

        let (job_id, _) = start_new(&handle, start_params(source, "hash-waiting")).await;
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

        let (job_a, _) = start_new(&handle, start_params(source_a, "hash-a")).await;
        wait_until_state(&handle, job_a, JobState::Running).await;
        let (job_b, _) = start_new(&handle, start_params(source_b, "hash-b")).await;

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

        let (job_id, session_id) = start_new(&handle, start_params(source, "hash-cancel")).await;
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

        let (job_id, _) = start_new(&handle, start_params(source, "hash-done")).await;
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
        let (job_id, session_id) = start_new(&handle, start_params(source, "hash-gap")).await;
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

        let (job_id, session_id) = start_new(&handle, start_params(source, "hash-fatal")).await;
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

        let (job_id, _) = start_new(&handle, start_params(source, "hash-seq")).await;
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

        let (job_id, session_id) = start_new(&handle, start_params(source, "hash-busy")).await;
        wait_until_state(&handle, job_id, JobState::Running).await;

        assert!(handle.is_busy(session_id).await.unwrap());
        assert!(!handle.is_busy(SessionId::new()).await.unwrap());

        handle.cancel(job_id).await.unwrap();
        wait_until_empty(&handle, Duration::from_secs(2)).await;
        assert!(!handle.is_busy(session_id).await.unwrap());
    }

    #[test]
    fn estimate_chunk_count_rounds_up_and_never_zero() {
        assert_eq!(estimate_chunk_count(0, 5), 1);
        assert_eq!(estimate_chunk_count(1, 5), 1);
        assert_eq!(estimate_chunk_count(300_000, 5), 1);
        assert_eq!(estimate_chunk_count(300_001, 5), 2);
        assert_eq!(estimate_chunk_count(600_000, 5), 2);
    }

    #[test]
    fn estimate_chunk_count_uses_the_captured_chunk_minutes() {
        // Story 2.6 Acceptance Criteria: "chunkMinutes = 1 và file 150 s ->
        // transcriber nhận 3 Chunk" -- the estimate must match a 1-minute
        // chunk size, not the 5-minute default.
        assert_eq!(estimate_chunk_count(150_000, 1), 3);
        assert_eq!(estimate_chunk_count(60_000, 1), 1);
        assert_eq!(estimate_chunk_count(60_001, 1), 2);
    }

    // ---------------------------------------------------------------------
    // Story 2.6: `chunkMinutes`/`language` captured on `StartParams` actually
    // drive the pipeline (decode chunking + what reaches `ChunkTranscriber` +
    // `transcripts.language`), and a Job already running/queued keeps its own
    // captured values independent of whatever a later `start` call captures.
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn chunk_minutes_of_one_splits_a_150_second_file_into_three_chunks_at_expected_starts() {
        // Acceptance Criteria: "Given Settings có `chunkMinutes = 1` và file
        // 150 s, when transcribe, then transcriber nhận 3 Chunk với
        // `start_ms` 0/60000/120000."
        let root = tempdir().unwrap();
        let source = wav_fixture_seconds(root.path(), "a.wav", 150);
        let transcriber = FakeTranscriber::new(vec![
            Behavior::Success("một".to_string()),
            Behavior::Success("hai".to_string()),
            Behavior::Success("ba".to_string()),
        ]);
        let handle = registry_for_test(root.path(), transcriber.clone());

        let params = StartParams {
            chunk_minutes: 1,
            ..start_params(source, "hash-chunk-minutes-one")
        };
        start_new(&handle, params).await;
        wait_until_empty(&handle, Duration::from_secs(10)).await;

        assert_eq!(transcriber.call_count(), 3);
        assert_eq!(transcriber.chunk_start_ms(), vec![0, 60_000, 120_000]);
    }

    /// A transcriber whose first call revokes consent in `db` (as a real
    /// Settings write, not just an in-memory flag) as a side effect, before
    /// returning that first chunk's successful transcript -- lets a test
    /// exercise the registry's per-chunk consent re-check deterministically
    /// without racing a real clock or a second process.
    struct RevokeConsentAfterFirstChunk {
        db: Arc<Db>,
        calls: Mutex<u32>,
    }

    impl ChunkTranscriber for RevokeConsentAfterFirstChunk {
        fn transcribe(
            &self,
            _model: String,
            chunk: Chunk,
            _language: TranscribeLanguage,
            _consent: ConsentSnapshot,
            _cancellation: CancellationToken,
            _observer: Arc<dyn JobObserver>,
        ) -> TranscribeChunkFuture {
            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            if *calls == 1 {
                settings::save(
                    &self.db,
                    &settings::Settings {
                        consent_accepted_version: 0,
                        consent_declined: true,
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            drop(calls);
            Box::pin(async move {
                Ok(ChunkTranscript {
                    segments: vec![Segment {
                        start: chunk.start_ms as f64 / 1000.0,
                        end: (chunk.start_ms + chunk.duration_ms) as f64 / 1000.0,
                        text: "ok".to_string(),
                        speaker: None,
                    }],
                    unresolved: vec![],
                    confirmed_silence: false,
                })
            })
        }
    }

    #[tokio::test]
    async fn consent_revoked_after_the_first_chunk_stops_the_job_before_the_second_chunk_and_does_not_commit(
    ) {
        // spec Acceptance Criteria: "Given a running Job, when consent is
        // revoked in settings, then no further chunk request is issued and
        // no session is committed." I/O Matrix "Consent revoked mid-job":
        // "chunk 2 never sent; Job Error Blocked".
        let root = tempdir().unwrap();
        let db = Arc::new(Db::open(root.path()).unwrap());
        seed_current_consent(&db);
        // 150 s at chunkMinutes=1 would normally be 3 Chunks (see the test
        // above) -- consent revoked on the first must stop before a second
        // is ever requested.
        let source = wav_fixture_seconds(root.path(), "a.wav", 150);
        let transcriber = Arc::new(RevokeConsentAfterFirstChunk {
            db: db.clone(),
            calls: Mutex::new(0),
        });
        let (handle, actor) = channel(
            db.clone(),
            root.path().to_path_buf(),
            transcriber.clone() as Arc<dyn ChunkTranscriber>,
        );
        tokio::spawn(actor.run());
        let (channel, received) = test_channel();
        handle.subscribe(channel).await.unwrap();

        let params = StartParams {
            chunk_minutes: 1,
            ..start_params(source, "hash-consent-revoked")
        };
        let (job_id, session_id) = start_new(&handle, params).await;
        wait_until_empty(&handle, Duration::from_secs(10)).await;

        assert_eq!(
            *transcriber.calls.lock().unwrap(),
            1,
            "chunk thứ hai không bao giờ được gửi sau khi consent bị revoke"
        );
        let events = received.lock().unwrap().clone();
        assert!(events.iter().any(
            |event| matches!(event, JobEvent::Error { job_id: id, error, .. } if *id == job_id && error.category == Category::Blocked)
        ));
        db.with_connection(|conn| {
            assert!(
                repo::sessions::get(conn, session_id)?.is_none(),
                "consent bị revoke giữa chừng không được commit Phiên"
            );
            Ok(())
        })
        .unwrap();
    }

    #[tokio::test]
    async fn captured_language_reaches_the_transcriber_and_is_saved_on_the_transcript() {
        let root = tempdir().unwrap();
        let db = Arc::new(Db::open(root.path()).unwrap());
        seed_current_consent(&db);
        let source = wav_fixture(root.path(), "a.wav");
        let transcriber = FakeTranscriber::new(vec![Behavior::Success("こんにちは".to_string())]);
        let (handle, actor) = channel(db.clone(), root.path().to_path_buf(), transcriber.clone());
        tokio::spawn(actor.run());

        let params = StartParams {
            language: TranscribeLanguage::Ja,
            ..start_params(source, "hash-lang-ja")
        };
        let (_job_id, session_id) = start_new(&handle, params).await;
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        assert_eq!(transcriber.languages_seen(), vec![TranscribeLanguage::Ja]);

        let transcript_id = db
            .with_connection(|conn| Ok(repo::transcripts::primary_for_session(conn, session_id)?))
            .unwrap()
            .unwrap();
        let transcript = db
            .with_connection(|conn| Ok(repo::transcripts::get(conn, transcript_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(transcript.language.as_deref(), Some("ja"));
    }

    #[tokio::test]
    async fn auto_language_leaves_the_transcript_language_null() {
        let root = tempdir().unwrap();
        let db = Arc::new(Db::open(root.path()).unwrap());
        seed_current_consent(&db);
        let source = wav_fixture(root.path(), "a.wav");
        let transcriber = FakeTranscriber::new(vec![Behavior::Success("hello".to_string())]);
        let (handle, actor) = channel(db.clone(), root.path().to_path_buf(), transcriber);
        tokio::spawn(actor.run());

        let (_job_id, session_id) =
            start_new(&handle, start_params(source, "hash-lang-auto")).await;
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        let transcript_id = db
            .with_connection(|conn| Ok(repo::transcripts::primary_for_session(conn, session_id)?))
            .unwrap()
            .unwrap();
        let transcript = db
            .with_connection(|conn| Ok(repo::transcripts::get(conn, transcript_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(transcript.language, None);
    }

    #[tokio::test]
    async fn a_queued_job_keeps_its_own_captured_config_independent_of_a_later_started_job() {
        // Acceptance Criteria: "Job đang chạy với cấu hình cũ ... Job đó
        // hoàn tất với cấu hình cũ và Job start sau đó dùng cấu hình mới."
        // The registry never re-reads Settings mid-pipeline — each Job only
        // ever sees the `StartParams` it was handed at `start()` — so two
        // Jobs queued back to back with different captured values must reach
        // the transcriber with exactly those values, in order.
        let root = tempdir().unwrap();
        let source_a = wav_fixture(root.path(), "a.wav");
        let source_b = wav_fixture(root.path(), "b.wav");
        let transcriber = FakeTranscriber::new(vec![
            Behavior::Success("a".to_string()),
            Behavior::Success("b".to_string()),
        ]);
        let handle = registry_for_test(root.path(), transcriber.clone());

        let params_a = StartParams {
            language: TranscribeLanguage::Auto,
            chunk_minutes: 5,
            ..start_params(source_a, "hash-cfg-a")
        };
        let params_b = StartParams {
            language: TranscribeLanguage::Vi,
            chunk_minutes: 2,
            ..start_params(source_b, "hash-cfg-b")
        };

        start_new(&handle, params_a).await;
        start_new(&handle, params_b).await;
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        assert_eq!(
            transcriber.languages_seen(),
            vec![TranscribeLanguage::Auto, TranscribeLanguage::Vi],
            "Job đầu giữ cấu hình captured lúc start của chính nó, không bị Job sau ghi đè"
        );
    }

    // ---------------------------------------------------------------------
    // Story 2.5: `JobKind::Rerun` end-to-end through the real registry actor
    // (`FakeTranscriber`, per spec Tasks: "test registry với `FakeTranscriber`"
    // and Acceptance Criteria's four `Given`/`when`/`then` rows).
    // ---------------------------------------------------------------------

    use crate::db::repo::segments::{GapReason, SegmentDraft, SegmentKind};
    use crate::db::repo::transcripts::Variant;
    use crate::library::store::TranscriptDraft;
    use crate::transcribe::rerun::{self, RerunScope};

    /// A mono 16 kHz WAV of exactly `seconds` seconds — long enough to carve
    /// deterministic gap ranges out of, short enough to decode instantly.
    fn wav_fixture_seconds(dir: &Path, name: &str, seconds: u32) -> PathBuf {
        let path = dir.join(name);
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for frame in 0..(16_000 * seconds) {
            let sample = ((frame as f32 * 0.05).sin() * 5_000.0) as i16;
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
        path
    }

    fn rerun_text_draft(start: f64, end: f64, text: &str) -> SegmentDraft {
        SegmentDraft {
            start_sec: start,
            end_sec: end,
            kind: SegmentKind::Text,
            gap_reason: None,
            text: text.to_string(),
            speaker: None,
        }
    }

    fn rerun_gap_draft(start: f64, end: f64) -> SegmentDraft {
        SegmentDraft {
            start_sec: start,
            end_sec: end,
            kind: SegmentKind::Gap,
            gap_reason: Some(GapReason::ChunkFailed),
            text: String::new(),
            speaker: None,
        }
    }

    /// Directly seeds a "committed partial file Phiên" (bypassing
    /// `commit_file_session` -- that story's own tests already cover
    /// publish/commit; here we only need a realistic starting point for a
    /// Chạy lại Job): a `sessions` row, a `primary` transcript with the given
    /// segments, and a real WAV published as its Proxy.
    fn seed_partial_session(
        root: &Path,
        db: &Db,
        session_id: SessionId,
        transcript_id: TranscriptId,
        duration_sec: f64,
        segments: &[SegmentDraft],
    ) -> PathBuf {
        let proxy_path = paths::proxy_path(root, session_id, "wav");
        std::fs::create_dir_all(proxy_path.parent().unwrap()).unwrap();
        wav_fixture_seconds(
            proxy_path.parent().unwrap(),
            "proxy.wav",
            duration_sec as u32,
        );

        db.with_connection(|conn| {
            repo::sessions::insert(
                conn,
                repo::sessions::NewSession {
                    id: session_id,
                    kind: "file",
                    title: "cuộc họp dở",
                    source_hash: None,
                    source_name: Some("meeting.wav"),
                    status: "complete",
                    recovered: false,
                    duration_sec,
                    proxy_ext: Some("wav"),
                    created_at: 0,
                    updated_at: 0,
                },
            )?;
            repo::transcripts::insert_with_segments(
                conn,
                transcript_id,
                session_id,
                Variant::Primary,
                "gemini-flash-lite-latest",
                None,
                segments,
                0,
            )?;
            Ok(())
        })
        .unwrap();

        proxy_path
    }

    fn resolved_missing_ranges(
        db: &Db,
        transcript_id: TranscriptId,
        total_duration_ms: u64,
    ) -> Vec<rerun::RerunRange> {
        let rows = db
            .with_connection(|conn| Ok(repo::segments::list_for_transcript(conn, transcript_id)?))
            .unwrap();
        rerun::resolve_ranges(RerunScope::Missing, &rows, total_duration_ms).unwrap()
    }

    #[tokio::test]
    async fn rerun_missing_sends_only_the_gap_ranges_with_absolute_start_ms_and_swaps_to_complete()
    {
        let root = tempdir().unwrap();
        let db = Arc::new(Db::open(root.path()).unwrap());
        seed_current_consent(&db);
        let session_id = SessionId::new();
        let old_transcript_id = TranscriptId::new();
        let proxy_path = seed_partial_session(
            root.path(),
            &db,
            session_id,
            old_transcript_id,
            4.0,
            &[
                rerun_text_draft(0.0, 1.0, "hello"),
                rerun_gap_draft(1.0, 2.0),
                rerun_text_draft(2.0, 3.0, "hello"),
                rerun_gap_draft(3.0, 4.0),
            ],
        );
        let ranges = resolved_missing_ranges(&db, old_transcript_id, 4_000);
        assert_eq!(ranges.len(), 2, "hai gap phải giải ra đúng hai vùng");

        let transcriber = FakeTranscriber::new(vec![
            Behavior::Success("vá 1".to_string()),
            Behavior::Success("vá 2".to_string()),
        ]);
        let (handle, actor) = channel(db.clone(), root.path().to_path_buf(), transcriber.clone());
        tokio::spawn(actor.run());

        let outcome = handle
            .start_rerun(RerunParams {
                session_id,
                transcript_id: old_transcript_id,
                ranges,
                discard_old: false,
                proxy_path,
                model: "gemini-flash-lite-latest".to_string(),
                language: TranscribeLanguage::Auto,
                chunk_minutes: 5,
                consent: ConsentSnapshot::new(1, false),
            })
            .await
            .unwrap();
        assert!(matches!(outcome, RerunOutcome::Started { .. }));
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        assert_eq!(transcriber.call_count(), 2, "chỉ đúng hai vùng được gửi");
        assert_eq!(
            transcriber.chunk_start_ms(),
            vec![1_000, 3_000],
            "start_ms phải tuyệt đối, không phải tương đối trong vùng"
        );

        let new_transcript_id = db
            .with_connection(|conn| Ok(repo::transcripts::primary_for_session(conn, session_id)?))
            .unwrap()
            .unwrap();
        assert_ne!(new_transcript_id, old_transcript_id);
        assert!(db
            .with_connection(|conn| Ok(repo::transcripts::get(conn, old_transcript_id)?))
            .unwrap()
            .is_none());
        let new_transcript = db
            .with_connection(|conn| Ok(repo::transcripts::get(conn, new_transcript_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(new_transcript.status, repo::transcripts::Status::Complete);

        let segments = db
            .with_connection(|conn| {
                Ok(repo::segments::list_for_transcript(
                    conn,
                    new_transcript_id,
                )?)
            })
            .unwrap();
        assert_eq!(
            segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(),
            vec!["hello", "vá 1", "hello", "vá 2"],
            "Segment ngoài vùng giữ nguyên, vùng gap được vá đúng vị trí"
        );
        assert!(segments.iter().all(|s| s.kind == SegmentKind::Text));

        let session = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(session.id, session_id, "session_id giữ nguyên");
    }

    #[tokio::test]
    async fn rerun_consent_revoked_after_the_first_range_stops_before_the_second_and_does_not_swap(
    ) {
        // Same per-chunk consent re-check as file transcribe (spec
        // Boundaries Always P0 review: "before every chunk send (file
        // transcribe and rerun)"), exercised on the Chạy lại pipeline: two
        // gap ranges to retry, consent revoked as a side effect of the first
        // range's transcribe call -- the second range must never be sent and
        // the old transcript must survive untouched (no swap/commit).
        let root = tempdir().unwrap();
        let db = Arc::new(Db::open(root.path()).unwrap());
        seed_current_consent(&db);
        let session_id = SessionId::new();
        let old_transcript_id = TranscriptId::new();
        let proxy_path = seed_partial_session(
            root.path(),
            &db,
            session_id,
            old_transcript_id,
            4.0,
            &[
                rerun_text_draft(0.0, 1.0, "hello"),
                rerun_gap_draft(1.0, 2.0),
                rerun_text_draft(2.0, 3.0, "hello"),
                rerun_gap_draft(3.0, 4.0),
            ],
        );
        let ranges = resolved_missing_ranges(&db, old_transcript_id, 4_000);
        assert_eq!(ranges.len(), 2, "hai gap phải giải ra đúng hai vùng");

        let transcriber = Arc::new(RevokeConsentAfterFirstChunk {
            db: db.clone(),
            calls: Mutex::new(0),
        });
        let (handle, actor) = channel(
            db.clone(),
            root.path().to_path_buf(),
            transcriber.clone() as Arc<dyn ChunkTranscriber>,
        );
        tokio::spawn(actor.run());
        let (channel, received) = test_channel();
        handle.subscribe(channel).await.unwrap();

        let outcome = handle
            .start_rerun(RerunParams {
                session_id,
                transcript_id: old_transcript_id,
                ranges,
                discard_old: false,
                proxy_path,
                model: "gemini-flash-lite-latest".to_string(),
                language: TranscribeLanguage::Auto,
                chunk_minutes: 5,
                consent: ConsentSnapshot::new(1, false),
            })
            .await
            .unwrap();
        let job_id = match outcome {
            RerunOutcome::Started { job_id } => job_id,
            RerunOutcome::Existing { .. } => panic!("expected a new Chạy lại Job"),
        };
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        assert_eq!(
            *transcriber.calls.lock().unwrap(),
            1,
            "vùng thứ hai không bao giờ được gửi sau khi consent bị revoke"
        );
        let events = received.lock().unwrap().clone();
        assert!(events.iter().any(
            |event| matches!(event, JobEvent::Error { job_id: id, error, .. } if *id == job_id && error.category == Category::Blocked)
        ));

        // The old transcript is untouched: no swap happened.
        assert!(db
            .with_connection(|conn| Ok(repo::transcripts::get(conn, old_transcript_id)?))
            .unwrap()
            .is_some());
        let primary = db
            .with_connection(|conn| Ok(repo::transcripts::primary_for_session(conn, session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(primary, old_transcript_id, "primary vẫn là transcript cũ");
    }

    #[tokio::test]
    async fn rerun_missing_with_one_gap_still_failing_stays_partial_with_only_that_gap_left() {
        let root = tempdir().unwrap();
        let db = Arc::new(Db::open(root.path()).unwrap());
        seed_current_consent(&db);
        let session_id = SessionId::new();
        let old_transcript_id = TranscriptId::new();
        let proxy_path = seed_partial_session(
            root.path(),
            &db,
            session_id,
            old_transcript_id,
            4.0,
            &[
                rerun_text_draft(0.0, 1.0, "hello"),
                rerun_gap_draft(1.0, 2.0),
                rerun_text_draft(2.0, 3.0, "hello"),
                rerun_gap_draft(3.0, 4.0),
            ],
        );
        let ranges = resolved_missing_ranges(&db, old_transcript_id, 4_000);
        assert_eq!(ranges.len(), 2);

        // First gap [1,2) is repaired; second gap [3,4) fails again
        // (non-fatal) -- per spec I/O Matrix "Chạy lại `missing`": "Vẫn lỗi
        // -> vẫn `partial` với gap còn lại".
        let transcriber = FakeTranscriber::new(vec![
            Behavior::Success("vá 1".to_string()),
            Behavior::Fail {
                code: Code::Network,
                retryable: true,
            },
        ]);
        let (handle, actor) = channel(db.clone(), root.path().to_path_buf(), transcriber.clone());
        tokio::spawn(actor.run());

        let outcome = handle
            .start_rerun(RerunParams {
                session_id,
                transcript_id: old_transcript_id,
                ranges,
                discard_old: false,
                proxy_path,
                model: "gemini-flash-lite-latest".to_string(),
                language: TranscribeLanguage::Auto,
                chunk_minutes: 5,
                consent: ConsentSnapshot::new(1, false),
            })
            .await
            .unwrap();
        assert!(matches!(outcome, RerunOutcome::Started { .. }));
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        assert_eq!(
            transcriber.call_count(),
            2,
            "cả hai vùng phải được gửi dù một vùng vẫn lỗi"
        );

        let new_transcript_id = db
            .with_connection(|conn| Ok(repo::transcripts::primary_for_session(conn, session_id)?))
            .unwrap()
            .unwrap();
        assert_ne!(
            new_transcript_id, old_transcript_id,
            "Job vẫn commit (không error) dù còn gap -- Chạy lại vẫn swap"
        );
        let new_transcript = db
            .with_connection(|conn| Ok(repo::transcripts::get(conn, new_transcript_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(
            new_transcript.status,
            repo::transcripts::Status::Partial,
            "vẫn lỗi một vùng -> transcript mới vẫn partial"
        );

        let segments = db
            .with_connection(|conn| {
                Ok(repo::segments::list_for_transcript(
                    conn,
                    new_transcript_id,
                )?)
            })
            .unwrap();
        assert_eq!(segments.len(), 4);
        assert_eq!(segments[0].kind, SegmentKind::Text);
        assert_eq!(segments[0].text, "hello");
        assert_eq!(segments[1].kind, SegmentKind::Text);
        assert_eq!(segments[1].text, "vá 1", "gap đầu tiên phải được vá");
        assert_eq!(segments[2].kind, SegmentKind::Text);
        assert_eq!(segments[2].text, "hello", "segment ngoài vùng giữ nguyên");
        assert_eq!(
            segments[3].kind,
            SegmentKind::Gap,
            "gap thứ hai vẫn lỗi -> vẫn còn đúng một gap"
        );
        assert_eq!(segments[3].gap_reason, Some(GapReason::ChunkFailed));
        assert_eq!((segments[3].start_sec, segments[3].end_sec), (3.0, 4.0));
    }

    #[tokio::test]
    async fn rerun_fatal_transcriber_error_leaves_the_old_transcript_and_segments_untouched() {
        let root = tempdir().unwrap();
        let db = Arc::new(Db::open(root.path()).unwrap());
        seed_current_consent(&db);
        let session_id = SessionId::new();
        let old_transcript_id = TranscriptId::new();
        let original_segments = vec![
            rerun_text_draft(0.0, 1.0, "hello"),
            rerun_gap_draft(1.0, 2.0),
        ];
        let proxy_path = seed_partial_session(
            root.path(),
            &db,
            session_id,
            old_transcript_id,
            2.0,
            &original_segments,
        );
        let ranges = resolved_missing_ranges(&db, old_transcript_id, 2_000);

        let transcriber = FakeTranscriber::new(vec![Behavior::Fail {
            code: Code::Auth,
            retryable: false,
        }]);
        let (handle, actor) = channel(db.clone(), root.path().to_path_buf(), transcriber);
        tokio::spawn(actor.run());
        let (channel, received) = test_channel();
        handle.subscribe(channel).await.unwrap();

        let outcome = handle
            .start_rerun(RerunParams {
                session_id,
                transcript_id: old_transcript_id,
                ranges,
                discard_old: false,
                proxy_path,
                model: "gemini-flash-lite-latest".to_string(),
                language: TranscribeLanguage::Auto,
                chunk_minutes: 5,
                consent: ConsentSnapshot::new(1, false),
            })
            .await
            .unwrap();
        let job_id = match outcome {
            RerunOutcome::Started { job_id } => job_id,
            RerunOutcome::Existing { .. } => panic!("phải là Job mới"),
        };
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        let events = received.lock().unwrap().clone();
        assert!(events.iter().any(
            |event| matches!(event, JobEvent::Error { job_id: id, error, .. } if *id == job_id && error.category == Category::Auth)
        ));

        assert_eq!(
            db.with_connection(|conn| Ok(repo::transcripts::primary_for_session(
                conn, session_id
            )?))
            .unwrap(),
            Some(old_transcript_id),
            "lỗi fatal không được đụng transcript cũ"
        );
        let segments = db
            .with_connection(|conn| {
                Ok(repo::segments::list_for_transcript(
                    conn,
                    old_transcript_id,
                )?)
            })
            .unwrap();
        assert_eq!(segments.len(), original_segments.len());
        assert!(!paths::staging_dir(root.path(), job_id).exists());
    }

    #[tokio::test]
    async fn rerun_cancelled_leaves_the_old_transcript_untouched() {
        let root = tempdir().unwrap();
        let db = Arc::new(Db::open(root.path()).unwrap());
        seed_current_consent(&db);
        let session_id = SessionId::new();
        let old_transcript_id = TranscriptId::new();
        let proxy_path = seed_partial_session(
            root.path(),
            &db,
            session_id,
            old_transcript_id,
            2.0,
            &[
                rerun_text_draft(0.0, 1.0, "hello"),
                rerun_gap_draft(1.0, 2.0),
            ],
        );
        let ranges = resolved_missing_ranges(&db, old_transcript_id, 2_000);

        let transcriber = FakeTranscriber::new(vec![Behavior::BlockUntilCancelled]);
        let (handle, actor) = channel(db.clone(), root.path().to_path_buf(), transcriber);
        tokio::spawn(actor.run());

        let outcome = handle
            .start_rerun(RerunParams {
                session_id,
                transcript_id: old_transcript_id,
                ranges,
                discard_old: false,
                proxy_path,
                model: "gemini-flash-lite-latest".to_string(),
                language: TranscribeLanguage::Auto,
                chunk_minutes: 5,
                consent: ConsentSnapshot::new(1, false),
            })
            .await
            .unwrap();
        let job_id = match outcome {
            RerunOutcome::Started { job_id } => job_id,
            RerunOutcome::Existing { .. } => panic!("phải là Job mới"),
        };
        wait_until_state(&handle, job_id, JobState::Running).await;
        handle.cancel(job_id).await.unwrap();
        wait_until_empty(&handle, Duration::from_secs(2)).await;

        assert_eq!(
            db.with_connection(|conn| Ok(repo::transcripts::primary_for_session(
                conn, session_id
            )?))
            .unwrap(),
            Some(old_transcript_id),
            "huỷ không được đụng transcript cũ"
        );
        assert!(!paths::staging_dir(root.path(), job_id).exists());
    }

    #[tokio::test]
    async fn rerun_calling_again_for_the_same_session_returns_the_existing_job() {
        let root = tempdir().unwrap();
        let db = Arc::new(Db::open(root.path()).unwrap());
        seed_current_consent(&db);
        let session_id = SessionId::new();
        let old_transcript_id = TranscriptId::new();
        let proxy_path = seed_partial_session(
            root.path(),
            &db,
            session_id,
            old_transcript_id,
            2.0,
            &[
                rerun_text_draft(0.0, 1.0, "hello"),
                rerun_gap_draft(1.0, 2.0),
            ],
        );
        let ranges = resolved_missing_ranges(&db, old_transcript_id, 2_000);

        let transcriber = FakeTranscriber::new(vec![Behavior::BlockUntilCancelled]);
        let (handle, actor) = channel(db.clone(), root.path().to_path_buf(), transcriber);
        tokio::spawn(actor.run());

        let params = RerunParams {
            session_id,
            transcript_id: old_transcript_id,
            ranges,
            discard_old: false,
            proxy_path,
            model: "gemini-flash-lite-latest".to_string(),
            language: TranscribeLanguage::Auto,
            chunk_minutes: 5,
            consent: ConsentSnapshot::new(1, false),
        };
        let first = handle.start_rerun(params.clone()).await.unwrap();
        let job_id = match first {
            RerunOutcome::Started { job_id } => job_id,
            RerunOutcome::Existing { .. } => panic!("phải là Job mới lần đầu"),
        };
        wait_until_state(&handle, job_id, JobState::Running).await;

        let second = handle.start_rerun(params).await.unwrap();
        assert_eq!(second, RerunOutcome::Existing { job_id });

        let jobs = handle.snapshot().await.unwrap();
        assert_eq!(
            jobs.iter()
                .filter(|job| job.session_id == session_id)
                .count(),
            1,
            "registry vẫn chỉ có một Job Chạy lại cho Phiên này"
        );

        handle.cancel(job_id).await.unwrap();
        wait_until_empty(&handle, Duration::from_secs(2)).await;
    }

    #[tokio::test]
    async fn rerun_captures_its_own_language_independent_of_the_original_transcribe() {
        // Acceptance table "Ngôn ngữ `ja`": "Transcribe file / Chạy lại ->
        // Prompt thêm câu chỉ định tiếng Nhật; `transcripts.language = 'ja'`."
        let root = tempdir().unwrap();
        let db = Arc::new(Db::open(root.path()).unwrap());
        seed_current_consent(&db);
        let session_id = SessionId::new();
        let old_transcript_id = TranscriptId::new();
        let proxy_path = seed_partial_session(
            root.path(),
            &db,
            session_id,
            old_transcript_id,
            2.0,
            &[
                rerun_text_draft(0.0, 1.0, "hello"),
                rerun_gap_draft(1.0, 2.0),
            ],
        );
        let ranges = resolved_missing_ranges(&db, old_transcript_id, 2_000);

        let transcriber = FakeTranscriber::new(vec![Behavior::Success("vá".to_string())]);
        let (handle, actor) = channel(db.clone(), root.path().to_path_buf(), transcriber.clone());
        tokio::spawn(actor.run());

        let outcome = handle
            .start_rerun(RerunParams {
                session_id,
                transcript_id: old_transcript_id,
                ranges,
                discard_old: false,
                proxy_path,
                model: "gemini-flash-lite-latest".to_string(),
                language: TranscribeLanguage::Ja,
                chunk_minutes: 5,
                consent: ConsentSnapshot::new(1, false),
            })
            .await
            .unwrap();
        assert!(matches!(outcome, RerunOutcome::Started { .. }));
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        assert_eq!(transcriber.languages_seen(), vec![TranscribeLanguage::Ja]);

        let new_transcript_id = db
            .with_connection(|conn| Ok(repo::transcripts::primary_for_session(conn, session_id)?))
            .unwrap()
            .unwrap();
        let new_transcript = db
            .with_connection(|conn| Ok(repo::transcripts::get(conn, new_transcript_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(new_transcript.language.as_deref(), Some("ja"));
    }

    #[tokio::test]
    async fn rerun_commit_with_a_stale_expected_transcript_writes_nothing_and_errors() {
        let root = tempdir().unwrap();
        let db = Arc::new(Db::open(root.path()).unwrap());
        seed_current_consent(&db);
        let session_id = SessionId::new();
        let old_transcript_id = TranscriptId::new();
        let proxy_path = seed_partial_session(
            root.path(),
            &db,
            session_id,
            old_transcript_id,
            2.0,
            &[
                rerun_text_draft(0.0, 1.0, "hello"),
                rerun_gap_draft(1.0, 2.0),
            ],
        );
        let ranges = resolved_missing_ranges(&db, old_transcript_id, 2_000);

        // Simulate "kết quả tới muộn": another commit already swapped the
        // primary transcript out from under this Job's `expected_transcript_id`
        // before it reaches its own commit (spec Design Notes: swap so khớp
        // `expected_transcript_id`, không khoá).
        let interfering_id = crate::library::store::swap_transcript(
            &db,
            session_id,
            old_transcript_id,
            TranscriptDraft {
                model: "gemini-flash-lite-latest".to_string(),
                language: None,
                segments: vec![rerun_text_draft(0.0, 2.0, "đã đổi trước")],
            },
        )
        .unwrap()
        .unwrap();

        let transcriber = FakeTranscriber::new(vec![Behavior::Success("quá muộn".to_string())]);
        let (handle, actor) = channel(db.clone(), root.path().to_path_buf(), transcriber);
        tokio::spawn(actor.run());
        let (channel, received) = test_channel();
        handle.subscribe(channel).await.unwrap();

        let outcome = handle
            .start_rerun(RerunParams {
                session_id,
                transcript_id: old_transcript_id,
                ranges,
                discard_old: false,
                proxy_path,
                model: "gemini-flash-lite-latest".to_string(),
                language: TranscribeLanguage::Auto,
                chunk_minutes: 5,
                consent: ConsentSnapshot::new(1, false),
            })
            .await
            .unwrap();
        let job_id = match outcome {
            RerunOutcome::Started { job_id } => job_id,
            RerunOutcome::Existing { .. } => panic!("phải là Job mới"),
        };
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        let events = received.lock().unwrap().clone();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, JobEvent::Error { job_id: id, .. } if *id == job_id)),
            "commit với expected_transcript_id lỗi thời phải kết thúc Job bằng error, thực tế: {events:?}"
        );

        assert_eq!(
            db.with_connection(|conn| Ok(repo::transcripts::primary_for_session(
                conn, session_id
            )?))
            .unwrap(),
            Some(interfering_id),
            "Job Chạy lại không được ghi gì khi expected_transcript_id đã lỗi thời"
        );
    }

    // ---------------------------------------------------------------------
    // Story 2.5 Tasks: "test tích hợp `GatewayTranscriber` qua transport giả
    // cho 4 kịch bản AR-36" — the real `GatewayTranscriber` (adapter +
    // `GeminiGateway` + key pool), driven by the real `JobRegistry`, with
    // only the HTTP transport faked (`gemini::test_support`). This locks the
    // retry policy at the layer that actually owns it (`gemini/`) *and* that
    // the registry/merge pipeline reacts to it correctly end to end.
    // ---------------------------------------------------------------------

    use crate::gemini::test_support::{gateway_with, key, FakeTransport};
    use crate::gemini::{TransportError, TransportResponse};

    fn gateway_transcriber(gateway: crate::gemini::GeminiGateway) -> Arc<dyn ChunkTranscriber> {
        Arc::new(GatewayTranscriber::new(Arc::new(gateway)))
    }

    fn valid_response(
        status: u16,
        start: &str,
        end: &str,
        text: &str,
    ) -> Result<TransportResponse, TransportError> {
        Ok(TransportResponse {
            status,
            body: format!(
                r#"{{"candidates":[{{"finishReason":"STOP","content":{{"parts":[{{"text":"[{{\"start\":\"{start}\",\"end\":\"{end}\",\"text\":\"{text}\"}}]"}}]}}}}]}}"#
            ),
        })
    }

    fn error_response(status: u16) -> Result<TransportResponse, TransportError> {
        Ok(TransportResponse {
            status,
            body: "fake error".to_string(),
        })
    }

    /// Every segment (text or gap) of `session_id`'s primary transcript, in
    /// `idx` order — used to assert full, contiguous time coverage ("no
    /// Chunk silently disappears" — spec I/O Matrix "JSON cắt cụt").
    fn primary_segments(
        db: &Db,
        session_id: SessionId,
    ) -> Vec<crate::db::repo::segments::SegmentRow> {
        db.with_connection(|conn| {
            let transcript_id = repo::transcripts::primary_for_session(conn, session_id)?
                .expect("Phiên phải có transcript primary");
            Ok(repo::segments::list_for_transcript(conn, transcript_id)?)
        })
        .unwrap()
    }

    fn assert_full_coverage(segments: &[crate::db::repo::segments::SegmentRow], total_sec: f64) {
        assert!(!segments.is_empty(), "phải có ít nhất một segment");
        assert_eq!(segments[0].start_sec, 0.0, "phải phủ từ đầu file");
        for pair in segments.windows(2) {
            assert_eq!(
                pair[0].end_sec, pair[1].start_sec,
                "không được có khoảng trống giữa hai segment -- một Chunk biến mất âm thầm: {segments:?}"
            );
        }
        assert_eq!(
            segments.last().unwrap().end_sec,
            total_sec,
            "phải phủ tới hết file, không mất phần cuối"
        );
    }

    #[tokio::test]
    async fn gateway_integration_429_on_key_one_rotates_to_key_two_then_the_chunk_succeeds() {
        let root = tempdir().unwrap();
        let source = wav_fixture_seconds(root.path(), "a.wav", 2);
        let transport = FakeTransport::new(vec![
            error_response(429),
            valid_response(200, "00:00", "00:01", "xin chào"),
        ]);
        let gateway = gateway_with(
            vec![key("a", "AIzaA123456789"), key("b", "AQ.B123456789")],
            transport.clone(),
        )
        .await;
        let handle = registry_for_test(root.path(), gateway_transcriber(gateway));

        let (_job_id, session_id) = start_new(&handle, start_params(source, "hash-429")).await;
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        let db = Db::open(root.path()).unwrap();
        let session = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(session.status, "complete");
        let segments = primary_segments(&db, session_id);
        assert!(segments.iter().any(|s| s.text == "xin chào"));

        let requests = transport.requests();
        assert_eq!(requests.len(), 2, "429 rồi thành công chỉ 2 lần gửi");
        assert_eq!(requests[0].header("x-goog-api-key"), Some("AIzaA123456789"));
        assert_eq!(
            requests[1].header("x-goog-api-key"),
            Some("AQ.B123456789"),
            "429 phải xoay sang key 2"
        );
    }

    #[tokio::test]
    async fn gateway_integration_four_server_errors_become_a_gap_and_the_job_still_commits_partial()
    {
        let root = tempdir().unwrap();
        let source = wav_fixture_seconds(root.path(), "b.wav", 2);
        let transport = FakeTransport::new(vec![
            error_response(503),
            error_response(503),
            error_response(503),
            error_response(503),
        ]);
        let gateway = gateway_with(vec![key("a", "AIzaA123456789")], transport.clone()).await;
        let handle = registry_for_test(root.path(), gateway_transcriber(gateway));

        let (_job_id, session_id) = start_new(&handle, start_params(source, "hash-503x4")).await;
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        let db = Db::open(root.path()).unwrap();
        let session = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(
            session.status, "complete",
            "sessions.status không đổi theo gap (do transcripts.status suy ra)"
        );
        let transcript_status: String = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT status FROM transcripts WHERE session_id = ?1",
                    [session_id.to_string()],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(
            transcript_status, "partial",
            "Chunk lỗi hệ thống -> gap, Job vẫn commit partial"
        );

        let segments = primary_segments(&db, session_id);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].kind, SegmentKind::Gap);
        assert_eq!(segments[0].gap_reason, Some(GapReason::ChunkFailed));
        assert_full_coverage(&segments, 2.0);
        assert_eq!(
            transport.requests().len(),
            usize::from(crate::gemini::params::MAX_ATTEMPTS)
        );
    }

    #[tokio::test]
    async fn gateway_integration_truncated_json_keeps_the_valid_segment_and_the_remainder_becomes_a_gap(
    ) {
        let root = tempdir().unwrap();
        let source = wav_fixture_seconds(root.path(), "c.wav", 2);
        // A response whose embedded segment array is cut off mid-object
        // (missing the closing `]`) — `parse_general_text` falls back to its
        // brace-matching scan, keeps the one complete item, and the parser's
        // own `incomplete` handling turns the remainder into `unresolved`.
        let truncated_body = r#"{"candidates":[{"finishReason":"MAX_TOKENS","content":{"parts":[{"text":"[{\"start\":\"00:00\",\"end\":\"00:01\",\"text\":\"hello\"}"}]}}]}"#;
        let transport = FakeTransport::new(vec![Ok(TransportResponse {
            status: 200,
            body: truncated_body.to_string(),
        })]);
        let gateway = gateway_with(vec![key("a", "AIzaA123456789")], transport.clone()).await;
        let handle = registry_for_test(root.path(), gateway_transcriber(gateway));

        let (_job_id, session_id) =
            start_new(&handle, start_params(source, "hash-truncated")).await;
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        let db = Db::open(root.path()).unwrap();
        let segments = primary_segments(&db, session_id);
        assert_eq!(
            segments.len(),
            2,
            "Segment hợp lệ giữ lại, phần còn lại thành đúng một gap"
        );
        assert_eq!(segments[0].kind, SegmentKind::Text);
        assert_eq!(segments[0].text, "hello");
        assert_eq!(segments[1].kind, SegmentKind::Gap);
        assert_eq!(segments[1].gap_reason, Some(GapReason::ChunkFailed));
        assert_full_coverage(&segments, 2.0);
    }

    #[tokio::test]
    async fn gateway_integration_401_on_every_key_fails_the_job_with_no_db_row() {
        let root = tempdir().unwrap();
        let source = wav_fixture_seconds(root.path(), "d.wav", 2);
        let transport = FakeTransport::new(vec![error_response(401), error_response(401)]);
        let gateway = gateway_with(
            vec![key("a", "AIzaA123456789"), key("b", "AQ.B123456789")],
            transport.clone(),
        )
        .await;
        let handle = registry_for_test(root.path(), gateway_transcriber(gateway));
        let (channel, received) = test_channel();
        handle.subscribe(channel).await.unwrap();

        let (job_id, session_id) = start_new(&handle, start_params(source, "hash-401-all")).await;
        wait_until_empty(&handle, Duration::from_secs(5)).await;

        let events = received.lock().unwrap().clone();
        assert!(events.iter().any(
            |event| matches!(event, JobEvent::Error { job_id: id, error, .. } if *id == job_id && error.category == Category::Auth)
        ));

        let db = Db::open(root.path()).unwrap();
        db.with_connection(|conn| {
            assert!(
                repo::sessions::get(conn, session_id)?.is_none(),
                "mọi key 401 -> không commit"
            );
            Ok(())
        })
        .unwrap();
    }

    /// Silence encodes to a tiny FLAC frame regardless of duration, so a
    /// fixture longer than 5 minutes (needed to force the real `Chunker`'s
    /// 300 s boundary and get a genuine "Chunk 2") still decodes/encodes
    /// near instantly.
    fn silent_wav_fixture_seconds(dir: &Path, name: &str, seconds: u32) -> PathBuf {
        let path = dir.join(name);
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..(16_000 * seconds) {
            writer.write_sample(0_i16).unwrap();
        }
        writer.finalize().unwrap();
        path
    }

    #[tokio::test]
    async fn gateway_integration_401_mid_job_on_the_second_chunk_rotates_key_then_succeeds() {
        let root = tempdir().unwrap();
        // 301 s of silence -> the real 300 s `Chunker` boundary emits exactly
        // two Chunks (300 s, then 1 s), so this is a genuine "Chunk 2".
        let source = silent_wav_fixture_seconds(root.path(), "e.wav", 301);
        let transport = FakeTransport::new(vec![
            // Chunk 1 -> key a (round-robin cursor starts at 0), healthy.
            valid_response(200, "00:00", "05:00", "phần một"),
            // Chunk 2 -> cursor has moved on to key b; it is rejected mid-Job
            // (disabling it), then the retry rotates back to key a, which
            // still works.
            error_response(401),
            valid_response(200, "00:00", "00:01", "phần hai"),
        ]);
        let gateway = gateway_with(
            vec![key("a", "AIzaA123456789"), key("b", "AQ.B123456789")],
            transport.clone(),
        )
        .await;
        let handle = registry_for_test(root.path(), gateway_transcriber(gateway));

        let (_job_id, session_id) =
            start_new(&handle, start_params(source, "hash-401-mid-job")).await;
        wait_until_empty(&handle, Duration::from_secs(30)).await;

        let db = Db::open(root.path()).unwrap();
        let session = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(session.status, "complete");
        let segments = primary_segments(&db, session_id);
        assert_eq!(
            segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(),
            vec!["phần một", "phần hai"],
            "cả hai Chunk phải có mặt, đúng thứ tự"
        );

        let requests = transport.requests();
        assert_eq!(requests.len(), 3);
        assert_eq!(
            requests[0].header("x-goog-api-key"),
            Some("AIzaA123456789"),
            "Chunk 1 dùng key a"
        );
        assert_eq!(
            requests[1].header("x-goog-api-key"),
            Some("AQ.B123456789"),
            "Chunk 2 (con trỏ round-robin đã sang key b) bị 401"
        );
        assert_eq!(
            requests[2].header("x-goog-api-key"),
            Some("AIzaA123456789"),
            "401 ở Chunk 2 phải xoay lại key a, vẫn thành công"
        );
    }
}
