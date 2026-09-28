//! Publish/commit một Phiên file bền vững (story 2.3, ADR
//! `adr-2-3-publish-commit.md`): staging → publish Proxy → một transaction DB
//! (session + transcript + segments), cộng reconcile idempotent lúc boot và
//! swap `primary` nguyên tử cho Chạy lại. Đây là chủ ghi duy nhất cho các
//! bảng đó ngoài `db/repo/*` — không nơi nào khác trong `library`/`ipc`
//! chèn/sửa dòng `sessions`/`transcripts`/`segments` trực tiếp.
//!
//! Thứ tự publish **trước** commit (Design Notes): cửa sổ crash giữa hai
//! bước chỉ để lại một thư mục `media/<sid>/` không có dòng DB nào tham
//! chiếu — [`reconcile`] xoá được bằng so khớp id, không cần biết gì thêm về
//! trạng thái. Ngược lại (commit trước publish) sẽ tạo ra một dòng DB tham
//! chiếu file chưa tồn tại, khó phân biệt với "Proxy lỗi" hợp lệ.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::core::error::{AppError, Code};
use crate::core::id::{JobId, SessionId, TranscriptId};
use crate::core::paths;
use crate::db::repo::{self, segments::SegmentDraft, transcripts::Variant};
use crate::db::Db;
use crate::media;

fn storage_error(detail: &str) -> AppError {
    AppError::new(Code::Storage, detail)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// Metadata cấp Phiên cho một commit mới — không gồm `id`/`status`/`kind`:
/// `commit_file_session` tự sinh `SessionId`, luôn `kind = "file"`,
/// `status = "complete"` (spec Never: story này không chuyển Phiên live).
#[derive(Debug, Clone, PartialEq)]
pub struct SessionDraft {
    pub title: String,
    pub source_hash: Option<String>,
    pub source_name: Option<String>,
    pub duration_sec: f64,
}

/// Metadata + nội dung cho một transcript mới (đã chuẩn hoá — spec Code Map:
/// "story này chỉ nhận draft đã chuẩn hoá").
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptDraft {
    pub model: String,
    pub language: Option<String>,
    pub segments: Vec<SegmentDraft>,
}

/// Draft đầy đủ cho một commit file mới.
#[derive(Debug, Clone, PartialEq)]
pub struct FileSessionDraft {
    pub session: SessionDraft,
    pub transcript: TranscriptDraft,
}

/// Kết quả một commit thành công. `proxy_error` khác `None` khi Proxy thiếu
/// (staging lỗi hoặc publish lỗi) — Transcript vẫn commit bình thường, lỗi
/// audio báo riêng cho caller quyết định hiển thị (spec I/O Matrix "Proxy
/// lỗi").
#[derive(Debug)]
pub struct CommitOutcome {
    pub session_id: SessionId,
    pub proxy_error: Option<AppError>,
}

/// Atomically inserts the initial durable metadata for a Live session.
/// `live::recording` creates and checkpoints `recording.wav` first, then calls
/// this function; on an insert error its caller removes the just-created
/// session directory so no header-only recording is left behind.
pub fn create_live_session(db: &Db, id: SessionId, title: &str) -> Result<(), AppError> {
    create_live_session_with_tags(db, id, title, &[])
}

/// Insert a Live session and its initially selected tags in one transaction.
/// A failed tag validation or link insert rolls the session row back too.
pub fn create_live_session_with_tags(
    db: &Db,
    id: SessionId,
    title: &str,
    tag_ids: &[crate::core::id::TagId],
) -> Result<(), AppError> {
    let unique_tag_ids = tag_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    if unique_tag_ids.len() > crate::library::tags::MAX_TAGS_PER_SESSION as usize {
        return Err(AppError::new(
            crate::core::error::Code::Request,
            "A Live session can have at most 20 tags",
        ));
    }
    db.with_connection(|conn| {
        let tx = conn.transaction()?;
        repo::sessions::insert(
            &tx,
            repo::sessions::NewSession {
                id,
                kind: "live",
                title,
                source_hash: None,
                source_name: None,
                status: "recording",
                recovered: false,
                duration_sec: 0.0,
                proxy_ext: None,
                created_at: now_ms(),
                updated_at: now_ms(),
            },
        )?;
        for tag_id in unique_tag_ids {
            let tag_exists = repo::tags::exists(&tx, tag_id)?;
            if !tag_exists {
                return Err(AppError::new(
                    crate::core::error::Code::Request,
                    "A selected tag no longer exists",
                ));
            }
            repo::tags::attach(&tx, id, tag_id)?;
        }
        tx.commit()?;
        Ok(())
    })
}

/// Creates the durable empty primary transcript for a Live session after its
/// Recording worker has started. If this fails, the caller stops capture and
/// finalizes the WAV while retaining the session row and recording file for
/// recovery.
pub fn create_live_transcript(
    db: &Db,
    session_id: SessionId,
    model: &str,
    language: Option<&str>,
) -> Result<TranscriptId, AppError> {
    let transcript_id = TranscriptId::new();
    db.with_connection(|conn| {
        let tx = conn.transaction()?;
        repo::transcripts::insert_with_segments(
            &tx,
            transcript_id,
            session_id,
            Variant::Primary,
            model,
            language,
            &[],
            now_ms(),
        )?;
        tx.commit()?;
        Ok(transcript_id)
    })
}

/// Commits one ordered Live segment batch and its sample-clock duration in a
/// single transaction. Empty batches still checkpoint session duration.
pub fn append_live_batch(
    db: &Db,
    session_id: SessionId,
    transcript_id: TranscriptId,
    segments: &[SegmentDraft],
    duration_sec: f64,
    status: &str,
) -> Result<(), AppError> {
    db.with_connection(|conn| {
        let tx = conn.transaction()?;
        let transcript = repo::transcripts::get(&tx, transcript_id)?;
        if !transcript
            .is_some_and(|row| row.session_id == session_id && row.variant == Variant::Primary)
        {
            return Err(AppError::new(
                Code::Storage,
                "Live transcript no longer belongs to its session",
            ));
        }
        repo::transcripts::append_ordered_batch(&tx, transcript_id, segments)?;
        if !repo::sessions::update_live_progress(&tx, session_id, duration_sec, status, now_ms())? {
            return Err(AppError::new(Code::Storage, "Live session row is missing"));
        }
        tx.commit()?;
        Ok(())
    })
}

/// Finalizes the durable metadata for a stopped Live recording. Proxy
/// generation and publication are best-effort: a failure commits the session
/// with `proxy_ext = NULL`, preserving the WAV and transcript. The final
/// session update is one transaction and only changes a `finalizing` Live row
/// to `complete`; if it fails, the published files and recovery row remain.
#[derive(Debug)]
pub struct LiveFinalizeOutcome {
    pub session_id: SessionId,
    pub proxy_error: Option<AppError>,
}

/// Finalizes the durable part of a close request. A Live close must not wait
/// for a potentially slow Proxy encode; boot recovery can still derive a
/// Proxy from the finalized WAV later.
pub fn finalize_live_session_minimal(
    db: &Db,
    session_id: SessionId,
    duration_sec: f64,
) -> Result<LiveFinalizeOutcome, AppError> {
    db.with_connection(|conn| {
        let tx = conn.transaction()?;
        if !repo::sessions::finalize_live(&tx, session_id, duration_sec, None, now_ms())? {
            return Err(storage_error("Live session is not in a finalizing state"));
        }
        #[cfg(test)]
        if fault::should_fail(fault::Point::Commit) {
            return Err(storage_error("injected: commit failure"));
        }
        tx.commit()?;
        Ok(LiveFinalizeOutcome {
            session_id,
            proxy_error: None,
        })
    })
}

/// Returns the interrupted Live rows captured at boot. The revision is kept
/// with each candidate so a delayed encoder cannot publish into a renamed,
/// deleted, or otherwise changed session.
pub fn live_recovery_candidates(
    db: &Db,
) -> Result<Vec<repo::sessions::LiveRecoveryCandidate>, AppError> {
    db.with_connection(|conn| Ok(repo::sessions::list_live_recovery_candidates(conn)?))
}

pub fn live_proxy_candidates(db: &Db) -> Result<Vec<repo::sessions::LiveProxyCandidate>, AppError> {
    db.with_connection(|conn| Ok(repo::sessions::list_live_proxy_candidates(conn)?))
}

/// Recover one interrupted Live session without ever rewriting its transcript
/// or notes. The WAV's checkpointed data length is authoritative: uncheckpointed
/// bytes are truncated, then a Proxy is attempted and the row is conditionally
/// committed under an immediate transaction.
pub fn recover_live_session(
    db: &Db,
    root: &Path,
    candidate: repo::sessions::LiveRecoveryCandidate,
) -> Result<bool, AppError> {
    let still_current = db.with_connection(|conn| {
        let Some(current) = repo::sessions::get(conn, candidate.id)? else {
            return Ok(false);
        };
        Ok(current.kind == "live"
            && matches!(current.status.as_str(), "recording" | "finalizing")
            && current.updated_at == candidate.updated_at
            && current.proxy_ext == candidate.proxy_ext)
    })?;
    if !still_current {
        return Ok(false);
    }

    let recording_path = paths::recording_path(root, candidate.id);
    let duration_sec = finalize_checkpointed_wav(&recording_path)?;

    let job_id = JobId::new();
    let staging_dir = paths::staging_dir(root, job_id);
    let current_proxy_exists = candidate
        .proxy_ext
        .as_deref()
        .is_some_and(|ext| paths::proxy_path(root, candidate.id, ext).is_file());
    let (staged_proxy, proxy_error) = if current_proxy_exists {
        (None, None)
    } else {
        match media::create_proxy(&staging_dir, &recording_path) {
            Ok(proxy) => (Some(proxy.path), None),
            Err(error) => (None, Some(error)),
        }
    };

    let outcome = db.with_connection(|conn| {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let Some(current) = repo::sessions::get(&tx, candidate.id)? else {
            return Ok(false);
        };
        if current.kind != "live"
            || !matches!(current.status.as_str(), "recording" | "finalizing")
            || current.updated_at != candidate.updated_at
            || current.proxy_ext != candidate.proxy_ext
        {
            return Ok(false);
        }

        let existing_proxy = current.proxy_ext.as_deref().filter(|ext| {
            paths::proxy_path(root, candidate.id, ext).is_file()
        });
        let (proxy_ext, published_path) = if let Some(ext) = existing_proxy {
            (Some(ext.to_owned()), None)
        } else if let Some(staged_path) = staged_proxy.as_deref() {
            match publish_proxy(root, candidate.id, staged_path) {
                Ok(ext) => (Some(ext.clone()), Some(paths::proxy_path(root, candidate.id, &ext))),
                Err(error) => {
                    remove_media_dir_if_empty(root, candidate.id);
                    tracing::warn!(session_id = %candidate.id, error = %error, "Live recovery kept the WAV without a Proxy");
                    (None, None)
                }
            }
        } else {
            (None, None)
        };

        if !repo::sessions::finalize_recovered_live(
            &tx,
            candidate.id,
            candidate.updated_at,
            duration_sec,
            proxy_ext.as_deref(),
            now_ms(),
        )? {
            if let Some(path) = published_path.as_deref() {
                let _ = fs::remove_file(path);
            }
            return Ok(false);
        }
        if let Err(error) = tx.commit() {
            if let Some(path) = published_path.as_deref() {
                let _ = fs::remove_file(path);
            }
            return Err(error.into());
        }
        Ok(true)
    });

    if let Err(error) = discard_staging(root, job_id) {
        tracing::warn!(session_id = %candidate.id, error = %error, "could not clean Live recovery staging");
    }
    if let Some(error) = proxy_error {
        tracing::warn!(session_id = %candidate.id, error = %error, "Live recovery kept the WAV without a Proxy");
    }
    outcome
}

/// Retries only the derived Proxy for a completed Live session. The row and
/// revision are checked under an immediate transaction before the staged
/// Proxy is published, so deletion during encoding cannot resurrect files.
pub fn repair_live_proxy(
    db: &Db,
    root: &Path,
    candidate: repo::sessions::LiveProxyCandidate,
) -> Result<bool, AppError> {
    let recording_path = paths::recording_path(root, candidate.id);
    finalize_checkpointed_wav(&recording_path)?;
    let job_id = JobId::new();
    let staging_dir = paths::staging_dir(root, job_id);
    let staged_proxy = match media::create_proxy(&staging_dir, &recording_path) {
        Ok(proxy) => proxy.path,
        Err(error) => {
            if let Err(cleanup_error) = discard_staging(root, job_id) {
                tracing::warn!(session_id = %candidate.id, error = %cleanup_error, "could not clean failed Live Proxy repair staging");
            }
            tracing::warn!(session_id = %candidate.id, error = %error, "Live Proxy repair will retry at a later boot");
            return Ok(false);
        }
    };

    let outcome = db.with_connection(|conn| {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let Some(current) = repo::sessions::get(&tx, candidate.id)? else {
            return Ok(false);
        };
        if current.kind != "live"
            || current.status != "complete"
            || current.proxy_ext.is_some()
            || current.updated_at != candidate.updated_at
        {
            return Ok(false);
        }
        let ext = match publish_proxy(root, candidate.id, &staged_proxy) {
            Ok(ext) => ext,
            Err(error) => {
                remove_media_dir_if_empty(root, candidate.id);
                tracing::warn!(session_id = %candidate.id, error = %error, "Live Proxy repair could not publish the Proxy");
                return Ok(false);
            }
        };
        if repo::sessions::set_proxy_ext(&tx, candidate.id, Some(&ext), now_ms()).is_err() {
            let _ = fs::remove_file(paths::proxy_path(root, candidate.id, &ext));
            return Err(storage_error("could not save repaired Live Proxy metadata"));
        }
        tx.commit()?;
        Ok(true)
    });

    if let Err(error) = discard_staging(root, job_id) {
        tracing::warn!(session_id = %candidate.id, error = %error, "could not clean Live Proxy repair staging");
    }
    outcome
}

fn finalize_checkpointed_wav(recording_path: &Path) -> Result<f64, AppError> {
    let reader = hound::WavReader::open(recording_path)
        .map_err(|error| storage_error(&format!("invalid checkpointed Live WAV: {error}")))?;
    let spec = reader.spec();
    if !(1..=2).contains(&spec.channels)
        || spec.sample_rate != crate::audio::OUTPUT_SAMPLE_RATE
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err(storage_error(
            "checkpointed Live WAV has an unsupported format",
        ));
    }
    let samples = u64::from(reader.duration());
    drop(reader);
    let data_bytes = samples
        .checked_mul(u64::from(spec.channels))
        .and_then(|frames| frames.checked_mul(u64::from(spec.bits_per_sample / 8)))
        .ok_or_else(|| storage_error("checkpointed Live WAV is too large"))?;
    let checkpointed_len = 44_u64
        .checked_add(data_bytes)
        .ok_or_else(|| storage_error("checkpointed Live WAV is too large"))?;
    let file = OpenOptions::new().write(true).open(recording_path)?;
    let actual_len = file.metadata()?.len();
    if actual_len < checkpointed_len {
        return Err(storage_error("checkpointed Live WAV data is incomplete"));
    }
    file.set_len(checkpointed_len)?;
    file.sync_all()?;
    Ok(samples as f64 / f64::from(spec.sample_rate))
}

pub fn finalize_live_session(
    db: &Db,
    root: &Path,
    session_id: SessionId,
    recording_path: &Path,
    duration_sec: f64,
) -> Result<LiveFinalizeOutcome, AppError> {
    let job_id = JobId::new();
    let staging_dir = paths::staging_dir(root, job_id);
    let (proxy_ext, proxy_error) = match media::create_proxy(&staging_dir, recording_path) {
        Ok(proxy) => match publish_proxy(root, session_id, &proxy.path) {
            Ok(ext) => (Some(ext), None),
            Err(error) => {
                remove_media_dir_if_empty(root, session_id);
                (None, Some(error))
            }
        },
        Err(error) => (None, Some(error)),
    };

    if let Err(error) = discard_staging(root, job_id) {
        tracing::warn!(session_id = %session_id, error = %error, "could not clean Live proxy staging");
    }

    db.with_connection(|conn| {
        let tx = conn.transaction()?;
        if !repo::sessions::finalize_live(
            &tx,
            session_id,
            duration_sec,
            proxy_ext.as_deref(),
            now_ms(),
        )? {
            return Err(storage_error("Live session is not in a finalizing state"));
        }

        #[cfg(test)]
        if fault::should_fail(fault::Point::Commit) {
            return Err(storage_error("injected: commit failure"));
        }

        tx.commit()?;
        Ok(())
    })?;

    Ok(LiveFinalizeOutcome {
        session_id,
        proxy_error,
    })
}

/// Compensates a newly inserted Live row if the dedicated Recording worker
/// cannot be started. Existing sessions and file sessions are not affected.
pub fn delete_live_session(db: &Db, id: SessionId) -> Result<(), AppError> {
    db.with_connection(|conn| {
        let tx = conn.transaction()?;
        repo::sessions::delete_live(&tx, id)?;
        tx.commit()?;
        Ok(())
    })
}

// ---------------------------------------------------------------------
// Điểm tiêm lỗi cho test (spec Tasks: "điểm lỗi tiêm được (cfg(test)) tại
// ghi file/rename/commit/dọn"). `thread_local` vì test chạy song song trên
// nhiều thread — mỗi test bật/tắt injection của riêng nó, không ảnh hưởng
// test khác.
// ---------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod fault {
    use std::cell::Cell;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Point {
        PublishWrite,
        PublishRename,
        Commit,
        Discard,
        /// Story 3.1: fail the `media/<sid>/` removal step of
        /// [`super::delete_session`] after its DB transaction already
        /// committed -- proves the DB row is gone and the caller still gets
        /// back a `storage` error (spec I/O Matrix "Xoá lỗi FS").
        DeleteMediaDir,
        /// Story 3.4: fail the `media/` removal step of [`super::wipe_all`]
        /// after its DB transaction already committed -- proves `sessions`/
        /// `tags` are gone and a retry can still finish the FS cleanup (spec
        /// I/O Matrix "Lỗi FS giữa chừng").
        WipeMediaDir,
    }

    thread_local! {
        static INJECT: Cell<Option<Point>> = const { Cell::new(None) };
        // Separate from `INJECT`/`Point` because `relink_proxy`'s DB write
        // needs a *count* (fail the first N calls, e.g. "fails once, retry
        // recovers" vs. "fails twice, retry also fails") rather than an
        // on/off toggle.
        static SET_PROXY_EXT_FAILURES: Cell<u32> = const { Cell::new(0) };
    }

    pub fn set(point: Option<Point>) {
        INJECT.with(|cell| cell.set(point));
    }

    pub fn should_fail(point: Point) -> bool {
        INJECT.with(|cell| cell.get() == Some(point))
    }

    pub fn set_proxy_ext_failures(count: u32) {
        SET_PROXY_EXT_FAILURES.with(|cell| cell.set(count));
    }

    /// Consumes one remaining failure, if any -- `true` means the caller
    /// should fail this attempt.
    pub fn consume_set_proxy_ext_failure() -> bool {
        SET_PROXY_EXT_FAILURES.with(|cell| {
            let remaining = cell.get();
            if remaining > 0 {
                cell.set(remaining - 1);
                true
            } else {
                false
            }
        })
    }
}

#[cfg(unix)]
fn fsync_dir_best_effort(dir: &Path) {
    // Best-effort trên Unix: fsync thư mục cha để đảm bảo entry `rename` mới
    // bền vững qua crash (không có API std tương đương an toàn trên Windows —
    // NTFS coi rename gần-atomic hơn nên bỏ qua ở đó là chấp nhận được).
    if let Ok(dir_file) = File::open(dir) {
        let _ = dir_file.sync_all();
    }
}

#[cfg(not(unix))]
fn fsync_dir_best_effort(_dir: &Path) {}

/// Publish Proxy đã staging vào `media/<sid>/proxy.<ext>` cố định (AD-5):
/// tạo thư mục đích, fsync nội dung file staging, rồi `rename` vào chỗ cuối
/// cùng (ADR: "staging → fsync → rename"). Lỗi ở đây không phải lỗi commit —
/// caller biến nó thành `proxy_error`, vẫn tiếp tục ghi Transcript (spec I/O
/// Matrix "Proxy lỗi").
fn publish_proxy(
    root: &Path,
    session_id: SessionId,
    staged_path: &Path,
) -> Result<String, AppError> {
    let dest_dir = paths::media_dir(root, session_id);
    fs::create_dir_all(&dest_dir).map_err(|err| storage_error(&err.to_string()))?;

    #[cfg(test)]
    if fault::should_fail(fault::Point::PublishWrite) {
        return Err(storage_error("injected: publish write failure"));
    }

    let ext = staged_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("flac")
        .to_string();

    {
        let file = File::open(staged_path).map_err(|err| storage_error(&err.to_string()))?;
        file.sync_all()
            .map_err(|err| storage_error(&err.to_string()))?;
    }

    #[cfg(test)]
    if fault::should_fail(fault::Point::PublishRename) {
        return Err(storage_error("injected: publish rename failure"));
    }

    let dest = paths::proxy_path(root, session_id, &ext);
    fs::rename(staged_path, &dest).map_err(|err| storage_error(&err.to_string()))?;
    fsync_dir_best_effort(&dest_dir);

    Ok(ext)
}

/// Best-effort cleanup for a `media/<sid>/` directory left behind by a
/// failed [`publish_proxy`] (it creates the directory up front, before the
/// write/fsync/rename that can fail). Only removes it when empty, so a
/// filesystem race that left real content is never silently deleted; any
/// failure is logged with the session id only, never a path (spec Always:
/// "no path content beyond the session ID").
fn remove_media_dir_if_empty(root: &Path, session_id: SessionId) {
    let dir = paths::media_dir(root, session_id);
    match fs::read_dir(&dir) {
        Ok(mut entries) => {
            if entries.next().is_none() {
                if let Err(err) = fs::remove_dir(&dir) {
                    tracing::warn!(
                        session_id = %session_id,
                        error = %err,
                        "không dọn được thư mục media mồ côi sau khi publish Proxy lỗi"
                    );
                }
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            tracing::warn!(
                session_id = %session_id,
                error = %err,
                "không đọc được thư mục media để dọn mồ côi sau khi publish Proxy lỗi"
            );
        }
    }
}

/// Commit một Phiên file mới: publish Proxy (nếu có) rồi ghi
/// session+transcript+segments trong một transaction (spec I/O Matrix "Commit
/// mới"). `staged_proxy` là kết quả `media::create_proxy` trong staging —
/// `Err` (staging không tạo được Proxy) được xử lý giống một lỗi publish:
/// commit vẫn tiếp tục với `proxy_ext = NULL`.
///
/// `session_id` do caller cấp (story 2.4: `JobRegistry` cấp trước
/// `session_id` dự kiến ngay lúc nhận Job, dùng cho tiến độ/route
/// `/session/:id` trước khi commit thật sự xảy ra) — hàm này không tự sinh
/// id nữa, chỉ dùng đúng id được truyền vào (spec Tasks: "`commit_file_session`
/// nhận `session_id` từ caller").
///
/// DB lỗi (kể cả `source_hash` trùng — spec I/O Matrix "Trùng hash") gỡ luôn
/// Proxy vừa publish và trả `Err`, không còn dòng DB nửa vời nào (transaction
/// tự rollback qua `Drop` khi closure trả `Err` trước `commit()`). Thành công
/// thì dọn staging của job — lỗi dọn chỉ log, không đổi kết quả (spec I/O
/// Matrix "Huỷ/lỗi trước commit": "Lỗi dọn chỉ log").
pub fn commit_file_session(
    db: &Db,
    root: &Path,
    job_id: JobId,
    session_id: SessionId,
    draft: FileSessionDraft,
    staged_proxy: Result<std::path::PathBuf, AppError>,
) -> Result<CommitOutcome, AppError> {
    let transcript_id = TranscriptId::new();
    let now = now_ms();

    let (proxy_ext, proxy_error) = match staged_proxy {
        Ok(staged_path) => match publish_proxy(root, session_id, &staged_path) {
            Ok(ext) => (Some(ext), None),
            Err(err) => {
                // `publish_proxy` creates `media/<sid>/` before it can fail
                // (dest dir, then write/fsync/rename) -- a failure this side
                // of a successful rename leaves that directory empty, and it
                // is never reconciled otherwise: commit still proceeds with
                // `proxy_ext = NULL`, so `reconcile` sees a *known* session
                // with no `proxy_ext` and no reason to touch its (now
                // nonexistent, once cleaned) media dir (spec Always: "remove
                // that directory if it is empty, best-effort, logged").
                remove_media_dir_if_empty(root, session_id);
                (None, Some(err))
            }
        },
        Err(err) => (None, Some(err)),
    };

    let commit_result: Result<(), AppError> = db.with_connection(|conn| {
        let tx = conn.transaction()?;
        repo::sessions::insert(
            &tx,
            repo::sessions::NewSession {
                id: session_id,
                kind: "file",
                title: &draft.session.title,
                source_hash: draft.session.source_hash.as_deref(),
                source_name: draft.session.source_name.as_deref(),
                status: "complete",
                recovered: false,
                duration_sec: draft.session.duration_sec,
                proxy_ext: proxy_ext.as_deref(),
                created_at: now,
                updated_at: now,
            },
        )?;
        repo::transcripts::insert_with_segments(
            &tx,
            transcript_id,
            session_id,
            Variant::Primary,
            &draft.transcript.model,
            draft.transcript.language.as_deref(),
            &draft.transcript.segments,
            now,
        )?;

        #[cfg(test)]
        if fault::should_fail(fault::Point::Commit) {
            return Err(storage_error("injected: commit failure"));
        }

        tx.commit()?;
        Ok(())
    });

    if let Err(err) = commit_result {
        // DB không commit -> Proxy vừa publish (nếu có) là mồ côi, gỡ ngay
        // thay vì chờ reconcile lần boot sau (spec I/O Matrix: "DB lỗi -> gỡ
        // thư mục đã publish, không còn dòng"; "Trùng hash -> Rollback, gỡ
        // Proxy vừa publish").
        if proxy_ext.is_some() {
            if let Err(remove_err) = fs::remove_dir_all(paths::media_dir(root, session_id)) {
                tracing::warn!(
                    session_id = %session_id,
                    error = %remove_err,
                    "không gỡ được Proxy mồ côi sau khi DB commit lỗi"
                );
            }
        }
        return Err(err);
    }

    if let Err(err) = discard_staging(root, job_id) {
        tracing::warn!(error = %err, "không dọn được staging sau khi commit thành công");
    }

    Ok(CommitOutcome {
        session_id,
        proxy_error,
    })
}

/// Tóm tắt một Phiên đã lưu — dùng bởi route `/session/:id` (story 2.4) khi
/// `id` không khớp Job nào đang chạy trong `JobRegistry`: đủ để hiển thị tên
/// Phiên, không phải toàn bộ `SessionRow`. `partial`/`primary_transcript_id`
/// (story 2.5) phản ánh transcript `primary` hiện tại — `None` chỉ khi Phiên
/// chưa có `primary` nào (không xảy ra với một Phiên đã commit qua
/// `commit_file_session`, nhưng giữ `Option` để không giả định điều đó ở
/// đây).
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSummary {
    pub session_id: SessionId,
    pub title: String,
    pub duration_sec: f64,
    pub status: String,
    pub partial: bool,
    pub primary_transcript_id: Option<TranscriptId>,
}

/// Đọc tóm tắt một Phiên theo id, `None` nếu không còn tồn tại (spec I/O
/// Matrix "`/session/:id`": "Phiên đã lưu / không còn").
pub fn get(db: &Db, session_id: SessionId) -> Result<Option<SessionSummary>, AppError> {
    db.with_connection(|conn| {
        let Some(row) = repo::sessions::get(conn, session_id)? else {
            return Ok(None);
        };
        let primary_transcript_id = repo::transcripts::primary_for_session(conn, session_id)?;
        let partial = match primary_transcript_id {
            Some(id) => repo::transcripts::get(conn, id)?
                .is_some_and(|transcript| transcript.status == repo::transcripts::Status::Partial),
            None => false,
        };
        Ok(Some(SessionSummary {
            session_id: row.id,
            title: row.title,
            duration_sec: row.duration_sec,
            status: row.status,
            partial,
            primary_transcript_id,
        }))
    })
}

/// Một đoạn (Segment) đã chuẩn hoá cho `/session/:id` (story 2.7) — không có
/// `speaker` (spec Boundaries: "Speaker không hiển thị"). `kind`/`gap_reason`
/// giữ nguyên chuỗi thô khớp `segments::SegmentKind`/`GapReason` (cùng quy
/// ước với `SessionLookup::Session.status` — chuỗi thô thay vì một enum
/// specta riêng cho giá trị chỉ dùng để hiển thị).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SegmentDetail {
    /// `i32` không phải `i64` — specta-typescript cấm xuất kiểu BigInt (xem
    /// `transcribe::rerun::RerunScope::Gap::gap_id`, cùng lý do: không
    /// transcript nào tới gần `i32::MAX` segment). Đây cũng chính là
    /// `gap_id` mà `transcribe_rerun` mong đợi cho dòng gap này.
    pub idx: i32,
    pub start_sec: f64,
    pub end_sec: f64,
    pub kind: String,
    pub gap_reason: Option<String>,
    pub text: String,
}

/// Transcript `primary` (hoặc `retranscribe`, giữ tổng quát) của một Phiên
/// cho `/session/:id` (story 2.7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptDetail {
    pub id: TranscriptId,
    pub variant: String,
    pub status: String,
    pub model: String,
    pub language: Option<String>,
    pub segments: Vec<SegmentDetail>,
}

/// Transcript record for export. Unlike [`TranscriptDetail`], this keeps
/// speaker labels because the detail UI intentionally does not display them.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptExportData {
    pub transcript: repo::transcripts::TranscriptRow,
    pub segments: Vec<repo::segments::SegmentRow>,
}

/// Chi tiết đầy đủ một Phiên cho `/session/:id` (story 2.7, Task:
/// "`SessionDetail { session_id, kind, title, created_at, duration_sec,
/// recovered, source_name, proxy_path, transcript }`"). `proxy_path` là
/// `Some` chỉ khi `proxy_ext` có giá trị **và** file thật còn tồn tại trên
/// đĩa — Proxy lỗi hoặc thiếu (spec I/O Matrix "Proxy thiếu") luôn là `None`,
/// không bao giờ một đường dẫn trỏ tới file không tồn tại. Giữ dạng `String`
/// (đường dẫn tuyệt đối) thay vì `PathBuf` để khớp quy ước sẵn có của mọi
/// đường dẫn khác qua IPC trong codebase này (`transcribe_start(path:
/// String)`). `source_hash` cố ý không nằm trong struct này — so khớp hash
/// chỉ diễn ra phía Rust ở [`relink_proxy`], frontend không bao giờ thấy
/// hash. `created_at` là mili-giây kể từ Unix epoch (UTC), giữ dạng `f64`
/// (không phải `i64`) vì specta-typescript cấm xuất kiểu BigInt (cùng lý do
/// `SegmentDetail::idx` dùng `i32`) — số nguyên tới 2^53 vẫn chính xác tuyệt
/// đối trong `f64`, xa hơn nhiều so với bất kỳ mốc thời gian thật nào.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    pub session_id: SessionId,
    pub kind: String,
    pub title: String,
    pub created_at: f64,
    pub duration_sec: f64,
    pub recovered: bool,
    pub source_name: Option<String>,
    pub proxy_path: Option<String>,
    pub transcript: Option<TranscriptDetail>,
    /// Tag đang gắn với Phiên này (story 3.2) — tên tăng dần
    /// (`library::tags::list_for_session`). Không phụ thuộc
    /// tên/transcript: đổi tên hay Chạy lại không đụng tới danh sách này
    /// (spec Boundaries Always).
    pub tags: Vec<crate::library::tags::TagSummary>,
    /// `true` khi Phiên có ít nhất một memo đã sinh (story 3.7, spec
    /// Boundaries Always: "Badge `memo` trong meta `SessionHeader` khi Phiên
    /// có ít nhất một memo") — nguồn cho `SessionHeader`'s badge `memo`.
    pub has_memo: bool,
}

fn segment_row_to_detail(row: repo::segments::SegmentRow) -> SegmentDetail {
    SegmentDetail {
        idx: row.idx as i32,
        start_sec: row.start_sec,
        end_sec: row.end_sec,
        kind: row.kind.as_str().to_string(),
        gap_reason: row.gap_reason.map(|reason| reason.as_str().to_string()),
        text: row.text,
    }
}

/// Đọc chi tiết đầy đủ một Phiên cho `/session/:id` (story 2.7): meta +
/// transcript `primary` hiện tại (nếu có) + đường dẫn Proxy đã kiểm tồn tại
/// trên đĩa. `None` khi Phiên không còn tồn tại (giữ đúng quy ước của
/// [`get`] ở trên). Không chạm registry Job — caller (`ipc::`) chỉ gọi hàm
/// này sau khi `library_session_get`/`SessionLookup` đã xác định `id` là một
/// Phiên đã lưu, không phải Job đang chạy (spec Design Notes: "detail tải
/// sau khi biết là Phiên").
pub fn get_detail(
    db: &Db,
    root: &Path,
    session_id: SessionId,
) -> Result<Option<SessionDetail>, AppError> {
    let Some(session) = db.with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?))?
    else {
        return Ok(None);
    };

    let proxy_path = match session.proxy_ext.as_deref() {
        Some(ext) => {
            let path = paths::proxy_path(root, session_id, ext);
            if path.is_file() {
                Some(path.to_string_lossy().into_owned())
            } else {
                None
            }
        }
        None => None,
    };

    let transcript = db.with_connection(|conn| {
        let Some(transcript_id) = repo::transcripts::primary_for_session(conn, session_id)? else {
            return Ok(None);
        };
        let Some(transcript_row) = repo::transcripts::get(conn, transcript_id)? else {
            return Ok(None);
        };
        let segments = repo::segments::list_for_transcript(conn, transcript_id)?
            .into_iter()
            .map(segment_row_to_detail)
            .collect();
        Ok(Some(TranscriptDetail {
            id: transcript_row.id,
            variant: transcript_row.variant.as_str().to_string(),
            status: transcript_row.status.as_str().to_string(),
            model: transcript_row.model,
            language: transcript_row.language,
            segments,
        }))
    })?;

    let tags = crate::library::tags::list_for_session(db, session_id)?;
    let has_memo =
        db.with_connection(|conn| Ok(repo::memos::exists_for_session(conn, session_id)?))?;

    Ok(Some(SessionDetail {
        session_id: session.id,
        kind: session.kind,
        title: session.title,
        created_at: session.created_at as f64,
        duration_sec: session.duration_sec,
        recovered: session.recovered,
        source_name: session.source_name,
        proxy_path,
        transcript,
        tags,
        has_memo,
    }))
}

/// Read one transcript and all of its segments for export. The caller passes
/// the selected transcript's opaque ID *and* the session it believes owns it
/// -- `session_id` must match the transcript's actual `session_id` or this
/// returns `None`, exactly like the transcript not existing at all (spec
/// Boundaries Always P0 review: "`get_export_data` returns `None` unless the
/// transcript's `session_id` matches"). Without this check, any signed-in
/// caller who could guess/enumerate a bare `transcript_id` could export a
/// transcript belonging to a different session (IDOR) -- `ipc::` passes the
/// currently open session's own ID, never one taken from the request alone.
pub fn get_export_data(
    db: &Db,
    session_id: SessionId,
    transcript_id: TranscriptId,
) -> Result<Option<TranscriptExportData>, AppError> {
    db.with_connection(|conn| {
        let Some(transcript) = repo::transcripts::get(conn, transcript_id)? else {
            return Ok(None);
        };
        if transcript.session_id != session_id {
            return Ok(None);
        }
        let segments = repo::segments::list_for_transcript(conn, transcript_id)?;
        Ok(Some(TranscriptExportData {
            transcript,
            segments,
        }))
    })
}

/// Kết quả [`relink_proxy`] (spec I/O Matrix "Chọn lại khớp/sai/Phiên live").
/// `NotFileSession` gộp cả "Phiên live" lẫn "Phiên file không có `source_hash`
/// để so khớp" — cả hai đều không có gì hợp lệ để khớp hash, caller
/// (`ipc::`) ánh xạ biến thể này sang `ProxyRelinkOutcome::LiveUnsupported`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelinkOutcome {
    Relinked,
    HashMismatch,
    NotFileSession,
}

/// Chọn lại file nguồn (story 2.7, FR-15): hash `picked_path`, chỉ chấp nhận
/// khi khớp đúng `source_hash` của Phiên, dựng Proxy mới trong staging (một
/// `JobId` dùng-một-lần, dọn ngay sau khi xong — không phải staging của một
/// Job thật trong `JobRegistry`) rồi publish thay Proxy cũ và cập nhật
/// `proxy_ext`. Lỗi ở bất kỳ bước nào trước khi `publish_proxy` đổi tên file
/// tại chỗ (staging tạo Proxy lỗi, hay chính `publish_proxy` lỗi ở bước ghi/
/// fsync/rename) đều giữ nguyên Proxy cũ tại `media/<sid>/proxy.<ext>` —
/// `publish_proxy` chỉ đổi tên vào đúng chỗ đó ở bước cuối cùng (xem doc của
/// nó). Staging luôn được dọn ở mọi nhánh thoát, kể cả lỗi (spec Tasks: "lỗi
/// giữa chừng không để lại staging và giữ Proxy cũ").
pub fn relink_proxy(
    db: &Db,
    root: &Path,
    session_id: SessionId,
    picked_path: &Path,
) -> Result<RelinkOutcome, AppError> {
    let session = db
        .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?))?
        .ok_or_else(|| storage_error("Phiên không còn tồn tại"))?;

    if session.kind != "file" {
        return Ok(RelinkOutcome::NotFileSession);
    }
    let Some(expected_hash) = session.source_hash.as_deref() else {
        return Ok(RelinkOutcome::NotFileSession);
    };

    let picked_hash = media::sha256_file(picked_path)?;
    if picked_hash != expected_hash {
        return Ok(RelinkOutcome::HashMismatch);
    }

    let job_id = JobId::new();
    let staging_dir = paths::staging_dir(root, job_id);

    let proxy_info = match media::create_proxy(&staging_dir, picked_path) {
        Ok(info) => info,
        Err(err) => {
            let _ = discard_staging(root, job_id);
            return Err(err);
        }
    };

    let ext = match publish_proxy(root, session_id, &proxy_info.path) {
        Ok(ext) => ext,
        Err(err) => {
            let _ = discard_staging(root, job_id);
            return Err(err);
        }
    };

    // The file is already renamed into place at this point -- a failure to
    // record it is a DB write, not a publish failure, so it gets one retry
    // before giving up (spec Always: "if `set_proxy_ext` fails after the
    // file was published, retry the DB write once").
    let write_proxy_ext = || {
        #[cfg(test)]
        if fault::consume_set_proxy_ext_failure() {
            return Err(storage_error("injected: set_proxy_ext failure"));
        }
        db.with_connection(|conn| {
            Ok(repo::sessions::set_proxy_ext(
                conn,
                session_id,
                Some(&ext),
                now_ms(),
            )?)
        })
    };
    if write_proxy_ext().is_err() {
        if let Err(err) = write_proxy_ext() {
            if let Err(discard_err) = discard_staging(root, job_id) {
                tracing::warn!(
                    session_id = %session_id,
                    error = %discard_err,
                    "không dọn được staging sau khi ghi proxy_ext lỗi"
                );
            }
            tracing::warn!(
                session_id = %session_id,
                error = %err,
                "Proxy đã publish nhưng ghi proxy_ext vào DB thất bại cả hai lần"
            );
            // The published file stays on disk -- the next `reconcile` sees
            // a proxy file with no recorded `proxy_ext` and repairs it
            // (spec Always: "reconcile must fix the mismatch").
            return Err(AppError::new(
                Code::Storage,
                "Proxy đã được publish nhưng chưa ghi được vào DB",
            ));
        }
    }

    if let Err(err) = discard_staging(root, job_id) {
        tracing::warn!(error = %err, "không dọn được staging sau khi chọn lại file nguồn thành công");
    }

    Ok(RelinkOutcome::Relinked)
}

/// Chạy lại (2.5): swap nguyên tử một transcript của một Phiên có sẵn trong
/// một transaction — chỉ ghi khi `expected_transcript_id` vẫn tồn tại đúng
/// Phiên này lúc bắt đầu (spec Always: "chỉ khi transcript đích vẫn tồn tại
/// với đúng id lúc bắt đầu, xoá nó và chèn bản mới cùng `variant`"; spec
/// Design Notes: "swap so khớp `expected_transcript_id` trong transaction
/// thay vì khoá"). Bản mới giữ đúng `variant` của bản cũ (`repo::transcripts::
/// swap` tự đọc lại) — caller quyết định việc chặn Chạy lại vào `primary`
/// của Phiên `live` trước khi gọi tới đây. Trả `Ok(None)` (không ghi gì) khi
/// transcript đích đã bị thay/xoá giữa chừng (spec I/O Matrix "Kết quả tới
/// muộn"); không chạm media/Proxy — Chạy lại chỉ transcribe lại, không tạo
/// Proxy mới.
pub fn swap_transcript(
    db: &Db,
    session_id: SessionId,
    expected_transcript_id: TranscriptId,
    draft: TranscriptDraft,
) -> Result<Option<TranscriptId>, AppError> {
    let new_id = TranscriptId::new();
    let now = now_ms();

    let swapped: bool = db.with_connection(|conn| {
        let tx = conn.transaction()?;
        let result = repo::transcripts::swap(
            &tx,
            session_id,
            expected_transcript_id,
            new_id,
            &draft.model,
            draft.language.as_deref(),
            &draft.segments,
            now,
        )?;
        if result.is_none() {
            // Không khớp `expected_transcript_id` -- không ghi gì, để `tx`
            // rollback qua Drop (không gọi `commit()`), giống nhánh lỗi ở
            // `commit_file_session`/`replace_primary_transcript`.
            return Ok(false);
        }

        #[cfg(test)]
        if fault::should_fail(fault::Point::Commit) {
            return Err(storage_error("injected: commit failure"));
        }

        tx.commit()?;
        Ok(true)
    })?;

    Ok(swapped.then_some(new_id))
}

/// Chạy lại: thay transcript `primary` của một Phiên có sẵn trong một
/// transaction (spec I/O Matrix "Chạy lại"). Không chạm media/Proxy — Chạy
/// lại chỉ transcribe lại, không tạo Proxy mới. Huỷ/lỗi giữa chừng giữ
/// nguyên bản cũ vì transaction tự rollback (spec: "Huỷ/lỗi -> bản cũ nguyên
/// vẹn").
pub fn replace_primary_transcript(
    db: &Db,
    session_id: SessionId,
    draft: TranscriptDraft,
) -> Result<TranscriptId, AppError> {
    let transcript_id = TranscriptId::new();
    let now = now_ms();

    db.with_connection(|conn| {
        let tx = conn.transaction()?;
        repo::transcripts::replace_primary(
            &tx,
            session_id,
            transcript_id,
            &draft.model,
            draft.language.as_deref(),
            &draft.segments,
            now,
        )?;

        #[cfg(test)]
        if fault::should_fail(fault::Point::Commit) {
            return Err(storage_error("injected: commit failure"));
        }

        tx.commit()?;
        Ok(())
    })?;

    Ok(transcript_id)
}

/// Giới hạn tên Phiên đếm theo Unicode scalar (spec Boundaries Always: "> 200
/// ký tự (đếm theo Unicode scalar) bị chặn ở cả UI (`maxlength`) lẫn Rust").
const MAX_TITLE_SCALARS: usize = 200;

/// Đổi tên một Phiên (story 3.1, spec Approach "đổi tên inline"): trim rồi từ
/// chối rỗng/quá dài trước khi ghi (spec I/O Matrix "Tên rỗng/khoảng trắng",
/// "Tên 201 ký tự": cả hai trả `Code::Request` khi Rust bị gọi trực tiếp,
/// không phải UI đã chặn trước). Không đụng `source_name`/`source_hash` (spec
/// Never). Trả `Ok(None)` khi Phiên không còn tồn tại (ví dụ vừa bị xoá) --
/// không phải lỗi, giống quy ước `get`/`get_detail` ở trên.
pub fn rename_session(
    db: &Db,
    session_id: SessionId,
    title: &str,
) -> Result<Option<String>, AppError> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err(AppError::new(
            Code::Request,
            "Tên phiên không được để trống",
        ));
    }
    if trimmed.chars().count() > MAX_TITLE_SCALARS {
        return Err(AppError::new(
            Code::Request,
            format!("Tên phiên vượt quá {MAX_TITLE_SCALARS} ký tự"),
        ));
    }

    let now = now_ms();
    let affected =
        db.with_connection(|conn| Ok(repo::sessions::set_title(conn, session_id, trimmed, now)?))?;
    if affected == 0 {
        return Ok(None);
    }
    Ok(Some(trimmed.to_string()))
}

/// Xoá hẳn một Phiên (story 3.1, spec Approach): xoá dòng DB trong một
/// transaction (FK `ON DELETE CASCADE` xoá luôn transcript/segment của nó)
/// rồi xoá `media/<sid>/` -- đúng thứ tự DB trước, file sau (spec Design
/// Notes: "DB trước, file sau: nếu crash giữa chừng, thư mục không còn dòng
/// DB và `reconcile` ... sẽ xoá nó lúc boot"). Idempotent theo cả hai chiều:
/// `id` không còn dòng DB xoá `0` dòng (không lỗi -- `repo::sessions::delete`)
/// và `media/<sid>/` không còn tồn tại cũng không lỗi (`remove_path_if_exists`)
/// -- gọi lại trên một Phiên đã xoá dở (DB mất, thư mục còn) vẫn dọn nốt thư
/// mục (spec I/O Matrix "Xoá session không tồn tại", "Xoá lỗi FS": "Gọi lại /
/// boot reconcile dọn tiếp"). Lỗi xoá `media/<sid>/` trả `storage` **sau khi**
/// DB đã commit -- caller không bao giờ báo "Đã xoá" trong nhánh này (spec
/// Always: "Xoá thư mục thất bại → lỗi category storage, không báo thành
/// công").
pub fn delete_session(db: &Db, root: &Path, session_id: SessionId) -> Result<(), AppError> {
    db.with_connection(|conn| {
        let tx = conn.transaction()?;
        repo::sessions::delete(&tx, session_id)?;
        tx.commit()?;
        Ok(())
    })?;

    #[cfg(test)]
    if fault::should_fail(fault::Point::DeleteMediaDir) {
        return Err(storage_error("injected: delete media dir failure"));
    }

    remove_path_if_exists(&paths::media_dir(root, session_id))
}

/// Số liệu Settings → Lưu trữ (story 3.4, spec Always): `media_bytes` là
/// tổng byte đệ quy dưới `<root>/media/`, `db_bytes` là tổng `app.db` +
/// `app.db-wal` + `app.db-shm` (file thiếu tính `0`), `session_count` là số
/// dòng `sessions`. `media_bytes`/`db_bytes` giữ dạng `f64` chứ không phải
/// `i64` -- cùng lý do `SessionListItem::created_at`: specta-typescript cấm
/// xuất kiểu BigInt, và `f64` biểu diễn chính xác mọi số nguyên byte tới
/// 2^53 (~8 PB), thừa cho một thư mục dữ liệu người dùng cục bộ.
/// `session_count` là `i32` (không phải `i64`), cùng quy ước
/// `SessionListItem::missing_gap_count`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StorageStats {
    pub media_bytes: f64,
    pub db_bytes: f64,
    pub session_count: i32,
}

/// Tổng byte đệ quy dưới `path`, bỏ qua chính `path` nếu chưa tồn tại (trả
/// `0` -- kho rỗng lúc chưa từng transcribe file nào, spec I/O Matrix "Kho
/// rỗng"). Duyệt bằng một stack tường minh (không đệ quy hàm) để không giới
/// hạn độ sâu; không có crate `walkdir` trong dependency (Code Map: tránh
/// thêm crate không cần thiết cho một phép duyệt thư mục đơn giản).
fn dir_size(path: &Path) -> Result<u64, AppError> {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(storage_error(&err.to_string())),
        };
        for entry in entries {
            let entry = entry.map_err(|err| storage_error(&err.to_string()))?;
            let metadata = entry
                .metadata()
                .map_err(|err| storage_error(&err.to_string()))?;
            if metadata.is_dir() {
                stack.push(entry.path());
            } else {
                total += metadata.len();
            }
        }
    }
    Ok(total)
}

/// Kích thước một file, `0` nếu không tồn tại (spec Always: "file thiếu =
/// 0") -- dùng cho `app.db-wal`/`app.db-shm`, vốn chỉ xuất hiện sau lần ghi
/// đầu tiên ở chế độ WAL.
fn file_len_or_zero(path: &Path) -> Result<u64, AppError> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(err) => Err(storage_error(&err.to_string())),
    }
}

/// Tổng byte của `app.db` + `app.db-wal` + `app.db-shm` dưới `root` (spec
/// Always: "DB = tổng `app.db` + `app.db-wal` + `app.db-shm`").
fn db_file_bytes(root: &Path) -> Result<u64, AppError> {
    let base = root.join(crate::db::DB_FILE_NAME);
    let wal = root.join(format!("{}-wal", crate::db::DB_FILE_NAME));
    let shm = root.join(format!("{}-shm", crate::db::DB_FILE_NAME));
    Ok(file_len_or_zero(&base)? + file_len_or_zero(&wal)? + file_len_or_zero(&shm)?)
}

/// Đo số liệu Settings → Lưu trữ hiện tại (story 3.4) -- xem [`StorageStats`]
/// cho ý nghĩa từng trường. Lỗi đọc thư mục/file quy về category `storage`
/// (spec I/O Matrix "Xem số liệu": "Lỗi đo → thông báo lỗi `storage`
/// inline").
pub fn storage_stats(db: &Db, root: &Path) -> Result<StorageStats, AppError> {
    let media_bytes = dir_size(&paths::media_root(root))?;
    let db_bytes = db_file_bytes(root)?;
    let session_count = db.with_connection(|conn| Ok(repo::sessions::count(conn)?))?;
    Ok(StorageStats {
        media_bytes: media_bytes as f64,
        db_bytes: db_bytes as f64,
        session_count: session_count as i32,
    })
}

/// Xoá toàn bộ dữ liệu họp (story 3.4, spec Boundaries Decision OQ9): mọi
/// Phiên/Transcript/segment/Proxy/Recording/Ghi chú/Memo/Tag (kể cả tag
/// không còn Phiên) và thư mục `media/` (kể cả `.staging`) -- **giữ**
/// `settings`/Consent (bảng `settings`, không đụng ở đây), API key
/// (Keychain, ngoài phạm vi DB/FS này), Template memo (bảng
/// `memo_templates`, story 3.6 spec Never: "Xoá toàn bộ dữ liệu (3.4) giữ
/// nguyên `memo_templates`" -- không có câu SQL nào trong hàm này tham chiếu
/// bảng đó nên nó tự nhiên sống sót, kiểm chứng ở test
/// `wipe_all_keeps_memo_templates`).
///
/// Thứ tự (spec Always: "DB trong một transaction ... rồi mới xoá nội dung
/// `media/`; sau đó `wal_checkpoint(TRUNCATE)`"): xoá `sessions` rồi `tags`
/// trong một transaction (FK `ON DELETE CASCADE` xoá theo
/// transcripts/segments/session_tags), sau đó xoá hẳn `media/`, cuối cùng
/// checkpoint WAL để lần đo `storage_stats` kế tiếp thấy DB nhỏ lại thật sự
/// thay vì vẫn to từ trước khi xoá. Idempotent theo cả hai chiều, cùng quy
/// ước [`delete_session`]: DB rỗng xoá `0` dòng, `media/` không còn tồn tại
/// không lỗi -- gọi lại sau một lần xoá dở (media lỗi FS) dọn tiếp được (spec
/// I/O Matrix "Lỗi FS giữa chừng": "gọi lại xoá tiếp được").
pub fn wipe_all(db: &Db, root: &Path) -> Result<(), AppError> {
    db.with_connection(|conn| {
        let tx = conn.transaction()?;
        repo::sessions::delete_all(&tx)?;
        repo::tags::delete_all(&tx)?;
        tx.commit()?;
        Ok(())
    })?;

    #[cfg(test)]
    if fault::should_fail(fault::Point::WipeMediaDir) {
        return Err(storage_error("injected: wipe media dir failure"));
    }

    remove_path_if_exists(&paths::media_root(root))?;

    db.checkpoint_truncate()
}

/// Xoá hẳn thư mục staging của một job (huỷ trước commit, hoặc dọn dẹp sau
/// một commit thành công). Idempotent: thư mục đã không tồn tại không phải
/// lỗi.
pub fn discard_staging(root: &Path, job_id: JobId) -> Result<(), AppError> {
    #[cfg(test)]
    if fault::should_fail(fault::Point::Discard) {
        return Err(storage_error("injected: staging cleanup failure"));
    }

    remove_path_if_exists(&paths::staging_dir(root, job_id))
}

fn remove_path_if_exists(path: &Path) -> Result<(), AppError> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(storage_error(&err.to_string())),
    }
}

/// Removes a stray reconcile entry, whether it turns out to be a file or a
/// directory. `session_id` is `Some` only when the entry's own name parsed
/// as one (a known-orphan or stray-inside-a-session-dir entry) — logged on
/// failure with no path content beyond it (spec Always).
fn remove_path_any(session_id: Option<SessionId>, path: &Path) {
    if fs::remove_file(path).is_ok() {
        return;
    }
    if let Err(err) = fs::remove_dir_all(path) {
        if err.kind() == std::io::ErrorKind::NotFound {
            return;
        }
        tracing::warn!(
            session_id = ?session_id,
            error = %err,
            "không dọn được entry mồ côi trong media lúc reconcile"
        );
    }
}

/// Reconcile idempotent lúc boot (AD-18, gọi từ `ipc::boot` sau `note_boot`):
/// dọn mọi thứ một crash giữa các bước của [`commit_file_session`] có thể để
/// lại, không bao giờ đụng tới một Phiên/Proxy đã commit thành công trước đó
/// (spec I/O Matrix "Crash giữa bước").
///
/// 1. Xoá hẳn `media/.staging` — publish luôn xảy ra trước commit (Design
///    Notes) nên bất kỳ staging nào còn sót đều bỏ được an toàn.
/// 2. Với mỗi entry trực tiếp dưới `media/`: không phải một `SessionId` hợp
///    lệ, hoặc là một `SessionId` không có dòng `sessions` nào -> xoá hẳn
///    (thư mục Phiên mồ côi từ cửa sổ crash giữa publish và commit, hoặc file
///    lạ).
/// 3. Với mỗi thư mục Phiên có dòng DB: nếu `proxy_ext` trỏ tới một file
///    không còn tồn tại, null hoá `proxy_ext` (DB tham chiếu Proxy đã mất);
///    giữ đúng tên Proxy hiện tại, đồng thời giữ `recording.wav` chỉ cho Phiên
///    live; mọi entry lạ khác (`.partial`, file tạm) đều bị xoá.
///
/// Chạy lại nhiều lần cho cùng kết quả: sau lần đầu, không còn gì để dọn nên
/// các lần sau là no-op (spec Always: "Reconcile chạy lại nhiều lần cho cùng
/// kết quả").
pub fn cleanup_staging(root: &Path) -> Result<(), AppError> {
    remove_path_if_exists(&paths::staging_root(root))?;
    Ok(())
}

/// Atomically removes the old staging tree from the writers' namespace. Boot
/// can then start Job and recovery writers immediately and delete this detached
/// tree in its background worker without racing files created after startup.
pub fn detach_staging(root: &Path) -> Result<Option<std::path::PathBuf>, AppError> {
    let staging = paths::staging_root(root);
    let detached = root.join(format!(".staging-cleanup-{}", JobId::new()));
    match fs::rename(&staging, &detached) {
        Ok(()) => Ok(Some(detached)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(storage_error(&error.to_string())),
    }
}

pub fn remove_detached_staging(path: &Path) -> Result<(), AppError> {
    remove_path_if_exists(path)
}

/// Removes staging trees detached by this app or left by a prior force-kill
/// between rename and cleanup. The reserved names are outside `media/`, so
/// this background scan cannot race a running Job's current staging directory.
pub fn cleanup_detached_staging(root: &Path) -> Result<(), AppError> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(storage_error(&error.to_string())),
    };
    for entry in entries {
        let entry = entry.map_err(|error| storage_error(&error.to_string()))?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(".staging-cleanup-"))
        {
            let path = entry.path();
            if fs::remove_file(&path).is_err() {
                remove_path_if_exists(&path)?;
            }
        }
    }
    Ok(())
}

/// Full media scan from `reconcile`, split out so startup can keep it off the
/// synchronous boot path while preserving the cleanup-before-recovery order.
pub fn reconcile_media(db: &Db, root: &Path) -> Result<(), AppError> {
    let known = media_refs(db)?;
    reconcile_media_refs(db, root, known, true)
}

/// Capture lightweight DB media refs before actors start. Startup reconciliation
/// uses this snapshot so it cannot mistake a just-created Live directory (which
/// briefly exists before its DB insert) for an orphan and delete it.
pub fn media_refs(db: &Db) -> Result<Vec<(SessionId, String, Option<String>)>, AppError> {
    db.with_connection(|conn| Ok(repo::sessions::list_media_refs(conn)?))
}

pub fn reconcile_media_snapshot(
    db: &Db,
    root: &Path,
    known_at_boot: Vec<(SessionId, String, Option<String>)>,
) -> Result<(), AppError> {
    reconcile_media_refs(db, root, known_at_boot, false)
}

fn reconcile_media_refs(
    db: &Db,
    root: &Path,
    known_refs: Vec<(SessionId, String, Option<String>)>,
    remove_orphans: bool,
) -> Result<(), AppError> {
    let media_root = paths::media_root(root);
    let entries = match fs::read_dir(&media_root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(storage_error(&err.to_string())),
    };

    let known: HashMap<SessionId, (String, Option<String>)> = known_refs
        .into_iter()
        .map(|(id, kind, proxy_ext)| (id, (kind, proxy_ext)))
        .collect();

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                // A single unreadable directory entry (permissions, a
                // concurrent delete) must never abort the rest of reconcile
                // (spec Always: "reconcile's `read_dir(...).flatten()` logs
                // entry errors").
                tracing::warn!(error = %err, "không đọc được một entry dưới media lúc reconcile");
                continue;
            }
        };
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            if remove_orphans {
                remove_path_any(None, &path);
            }
            continue;
        };
        // A new writer may have recreated this directory after boot detached
        // the old one. Never inspect or remove the live staging namespace here.
        if name == ".staging" {
            continue;
        }
        let Ok(session_id) = SessionId::try_from(name.as_str()) else {
            // Không parse được thành UUIDv7 -> không phải thư mục Phiên hợp
            // lệ (bao gồm cả trường hợp `.staging` lỡ tái tạo giữa hai bước
            // trên do một tiến trình khác — vẫn dọn được an toàn).
            if remove_orphans {
                remove_path_any(None, &path);
            }
            continue;
        };
        match known.get(&session_id) {
            None if remove_orphans => remove_path_any(Some(session_id), &path),
            None => {}
            Some((kind, proxy_ext)) if path.is_dir() => {
                // An error reconciling one session directory is logged and
                // the loop continues -- it never aborts the other sessions
                // (spec Always).
                if let Err(err) =
                    reconcile_session_dir(db, session_id, kind, proxy_ext.as_deref(), &path)
                {
                    tracing::warn!(
                        session_id = %session_id,
                        error = %err,
                        "không reconcile được một thư mục Phiên, tiếp tục các Phiên khác"
                    );
                }
            }
            Some(_) => remove_path_any(Some(session_id), &path),
        }
    }

    Ok(())
}

pub fn reconcile(db: &Db, root: &Path) -> Result<(), AppError> {
    cleanup_staging(root)?;
    reconcile_media(db, root)
}

/// Scans `dir` for a `proxy.<ext>` file and returns `<ext>` — used only to
/// repair a session whose `proxy_ext` is `NULL` in DB but whose proxy file
/// is actually on disk (spec Always: "relink can leave disk and DB
/// disagreeing... reconcile sets `proxy_ext` from the file").
fn find_proxy_file_ext(dir: &Path) -> Option<String> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if let Some(ext) = name.strip_prefix("proxy.") {
            if !ext.is_empty() && entry.path().is_file() {
                return Some(ext.to_string());
            }
        }
    }
    None
}

fn reconcile_session_dir(
    db: &Db,
    session_id: SessionId,
    kind: &str,
    proxy_ext: Option<&str>,
    dir: &Path,
) -> Result<(), AppError> {
    let expected_name = proxy_ext.map(|ext| format!("proxy.{ext}"));
    let proxy_file_exists = expected_name
        .as_ref()
        .map(|name| dir.join(name).is_file())
        .unwrap_or(false);

    if proxy_ext.is_some() && !proxy_file_exists {
        db.with_connection(|conn| {
            repo::sessions::set_proxy_ext(conn, session_id, None, now_ms())?;
            Ok(())
        })?;
    }

    let mut keep_names = Vec::new();
    if proxy_file_exists {
        if let Some(name) = expected_name {
            keep_names.push(name);
        }
    }
    if kind == "live" && dir.join("recording.wav").is_file() {
        keep_names.push("recording.wav".to_owned());
    }

    // `proxy_ext` is NULL (never set, or a `relink_proxy` DB write that
    // failed even after its retry) but a proxy file exists on disk -- repair
    // the mismatch by reading the extension back from the file (spec
    // Always).
    if proxy_ext.is_none() {
        if let Some(found_ext) = find_proxy_file_ext(dir) {
            db.with_connection(|conn| {
                repo::sessions::set_proxy_ext(conn, session_id, Some(&found_ext), now_ms())?;
                Ok(())
            })?;
            keep_names.push(format!("proxy.{found_ext}"));
        }
    }

    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(storage_error(&err.to_string())),
    };
    for entry in entries.flatten() {
        let is_kept = keep_names
            .iter()
            .any(|keep| entry.file_name().to_str() == Some(keep.as_str()));
        if !is_kept {
            remove_path_any(Some(session_id), &entry.path());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::segments::{GapReason, SegmentKind};
    use std::io::Write as _;
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::thread;
    use tempfile::tempdir;

    fn open_db(root: &Path) -> Db {
        Db::open(root).unwrap()
    }

    #[test]
    fn checkpoint_recovery_preserves_all_channels_and_truncates_only_the_uncheckpointed_tail() {
        let root = tempdir().unwrap();
        let path = root.path().join("stereo.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: crate::audio::OUTPUT_SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for frame in 0..100 {
            writer.write_sample(frame as i16).unwrap();
            writer.write_sample(-(frame as i16)).unwrap();
        }
        writer.finalize().unwrap();
        let checkpointed_len = fs::metadata(&path).unwrap().len();
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(&[1; 40])
            .unwrap();

        let duration = finalize_checkpointed_wav(&path).unwrap();
        let mut recovered = hound::WavReader::open(&path).unwrap();
        assert_eq!(recovered.spec().channels, 2);
        assert_eq!(recovered.duration(), 100);
        assert_eq!(recovered.samples::<i16>().map(Result::unwrap).count(), 200);
        assert_eq!(fs::metadata(&path).unwrap().len(), checkpointed_len);
        assert!((duration - 100.0 / f64::from(crate::audio::OUTPUT_SAMPLE_RATE)).abs() < 1e-9);
    }

    #[test]
    fn boot_recovery_commits_once_and_preserves_segments_notes_and_playable_wav() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let (session_id, recording_path) = seed_finalizing_live(root.path(), &db);
        db.with_connection(|conn| {
            repo::notes::save_if_newer(
                conn,
                session_id,
                &crate::core::sensitive::Sensitive::new("keep this note".to_owned()),
                3,
                now_ms(),
            )?;
            Ok(())
        })
        .unwrap();
        let original = live_recovery_candidates(&db).unwrap().remove(0);

        assert!(recover_live_session(&db, root.path(), original.clone()).unwrap());
        // Reusing a stale boot candidate is a no-op; completed sessions are
        // not selected again and no transcript/segment is added.
        assert!(!recover_live_session(&db, root.path(), original).unwrap());
        assert!(live_recovery_candidates(&db).unwrap().is_empty());

        let row = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(row.status, "complete");
        assert!(row.recovered);
        assert!((row.duration_sec - 1.0).abs() < 0.01);
        let primary_id = db
            .with_connection(|conn| {
                Ok(repo::transcripts::primary_for_session(conn, session_id)?.unwrap())
            })
            .unwrap();
        let segments = db
            .with_connection(|conn| Ok(repo::segments::list_for_transcript(conn, primary_id)?))
            .unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "final transcript");
        let note = db
            .with_connection(|conn| Ok(repo::notes::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(note.body.expose(), "keep this note");
        let reader = hound::WavReader::open(recording_path).unwrap();
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.duration(), 16_000);
        assert_eq!(
            row.proxy_ext.is_some(),
            paths::media_dir(root.path(), session_id)
                .join(format!(
                    "proxy.{}",
                    row.proxy_ext.as_deref().unwrap_or("missing")
                ))
                .is_file()
        );
    }

    #[test]
    fn boot_recovery_keeps_wav_and_segments_when_proxy_publish_fails() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let (session_id, recording_path) = seed_finalizing_live(root.path(), &db);
        let candidate = live_recovery_candidates(&db).unwrap().remove(0);

        fault::set(Some(fault::Point::PublishWrite));
        let result = recover_live_session(&db, root.path(), candidate);
        fault::set(None);

        assert!(result.unwrap());
        let row = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(row.status, "complete");
        assert!(row.recovered);
        assert_eq!(row.proxy_ext, None);
        assert!(hound::WavReader::open(recording_path).is_ok());
        let transcript_id = db
            .with_connection(|conn| {
                Ok(repo::transcripts::primary_for_session(conn, session_id)?.unwrap())
            })
            .unwrap();
        let segments = db
            .with_connection(|conn| Ok(repo::segments::list_for_transcript(conn, transcript_id)?))
            .unwrap();
        assert_eq!(segments.len(), 1);
    }

    #[test]
    fn boot_recovery_db_failure_keeps_the_orphan_available_for_retry() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let (session_id, recording_path) = seed_finalizing_live(root.path(), &db);
        let candidate = live_recovery_candidates(&db).unwrap().remove(0);
        db.with_connection(|conn| {
            conn.execute_batch(
                "CREATE TRIGGER fail_recovery_commit BEFORE UPDATE OF status ON sessions \
                 WHEN NEW.status = 'complete' BEGIN SELECT RAISE(FAIL, 'injected commit failure'); END;",
            )?;
            Ok(())
        })
        .unwrap();

        assert!(recover_live_session(&db, root.path(), candidate).is_err());
        let row = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(row.status, "finalizing");
        assert!(!row.recovered);
        assert!(recording_path.is_file());
        assert_eq!(live_recovery_candidates(&db).unwrap().len(), 1);
    }

    #[test]
    fn force_killed_live_reopens_database_and_recovers_library_row_segment_and_wav() {
        let root = tempdir().unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "live::recording::tests::boot_recovery_crash_child_waits_for_parent_kill",
                "--nocapture",
            ])
            .env("LIVE_BOOT_RECOVERY_ROOT", root.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (session_tx, session_rx) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().flatten() {
                if let Some(raw_id) = line.strip_prefix("LIVE_BOOT_RECOVERY_CHECKPOINTED:") {
                    let _ = session_tx.send(raw_id.to_owned());
                    return;
                }
            }
        });
        let raw_session_id = session_rx.recv_timeout(std::time::Duration::from_secs(15));
        if !matches!(raw_session_id, Ok(_)) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("crash writer did not checkpoint before timeout: {raw_session_id:?}");
        }
        child.kill().unwrap();
        let _ = child.wait();
        let session_id = SessionId::try_from(raw_session_id.unwrap().as_str()).unwrap();

        // Reopen after the child process is gone, as a real next boot does.
        let db = open_db(root.path());
        let candidate = live_recovery_candidates(&db)
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == session_id)
            .expect("force-killed Live remains recoverable in SQLite");
        assert!(recover_live_session(&db, root.path(), candidate).unwrap());

        let row = db
            .with_connection(|conn| {
                Ok(repo::sessions::list_for_home(conn)?
                    .into_iter()
                    .find(|row| row.id == session_id)
                    .unwrap())
            })
            .unwrap();
        assert_eq!(row.kind, "live");
        assert!(row.recovered);
        let transcript_id = db
            .with_connection(|conn| {
                Ok(repo::transcripts::primary_for_session(conn, session_id)?.unwrap())
            })
            .unwrap();
        let segments = db
            .with_connection(|conn| Ok(repo::segments::list_for_transcript(conn, transcript_id)?))
            .unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "durably flushed before kill");
        let mut wav =
            hound::WavReader::open(paths::recording_path(root.path(), session_id)).unwrap();
        assert_eq!(wav.spec().sample_rate, 16_000);
        assert_eq!(wav.duration(), 80_000);
        assert_eq!(wav.samples::<i16>().map(Result::unwrap).count(), 80_000);
    }

    #[test]
    fn boot_media_snapshot_does_not_delete_a_new_live_directory_or_fresh_staging() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let media_snapshot = media_refs(&db).unwrap();

        let session_id = SessionId::new();
        create_live_session(&db, session_id, "Started during boot").unwrap();
        let recording = paths::recording_path(root.path(), session_id);
        fs::create_dir_all(recording.parent().unwrap()).unwrap();
        fs::write(&recording, b"writer still owns this directory").unwrap();
        let staging_file = paths::staging_dir(root.path(), JobId::new()).join("in-progress");
        fs::create_dir_all(staging_file.parent().unwrap()).unwrap();
        fs::write(&staging_file, b"job staging").unwrap();

        reconcile_media_snapshot(&db, root.path(), media_snapshot).unwrap();

        assert!(recording.is_file());
        assert!(staging_file.is_file());
    }

    #[test]
    fn boot_detaches_old_staging_before_new_writers_start() {
        let root = tempdir().unwrap();
        let stale = paths::staging_dir(root.path(), JobId::new()).join("old-partial");
        fs::create_dir_all(stale.parent().unwrap()).unwrap();
        fs::write(&stale, b"stale").unwrap();

        let detached = detach_staging(root.path()).unwrap().unwrap();
        assert!(!paths::staging_root(root.path()).exists());
        let old_relative = stale
            .strip_prefix(paths::staging_root(root.path()))
            .unwrap();
        assert!(detached.join(old_relative).is_file());

        let new = paths::staging_dir(root.path(), JobId::new()).join("new-partial");
        fs::create_dir_all(new.parent().unwrap()).unwrap();
        fs::write(&new, b"new").unwrap();
        let orphaned_from_older_boot = root.path().join(".staging-cleanup-abandoned");
        fs::create_dir(&orphaned_from_older_boot).unwrap();
        cleanup_detached_staging(root.path()).unwrap();
        assert!(new.is_file());
        assert!(!detached.exists());
        assert!(!orphaned_from_older_boot.exists());
    }

    #[test]
    fn recovery_candidate_cannot_resurrect_a_row_deleted_during_proxy_work() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let (session_id, _recording_path) = seed_finalizing_live(root.path(), &db);
        let candidate = live_recovery_candidates(&db).unwrap().remove(0);
        db.with_connection(|conn| {
            repo::sessions::delete(conn, session_id)?;
            Ok(())
        })
        .unwrap();

        assert!(!recover_live_session(&db, root.path(), candidate).unwrap());

        assert!(db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?))
            .unwrap()
            .is_none());
        assert!(!paths::media_dir(root.path(), session_id)
            .join("proxy.flac")
            .exists());
        assert!(staging_root_is_empty(root.path()));
    }

    #[test]
    fn close_minimal_completion_leaves_proxy_for_the_next_boot_repair() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let (session_id, recording_path) = seed_finalizing_live(root.path(), &db);

        finalize_live_session_minimal(&db, session_id, 1.0).unwrap();
        let completed = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(completed.status, "complete");
        assert_eq!(completed.proxy_ext, None);
        let repair = live_proxy_candidates(&db).unwrap().remove(0);

        assert!(repair_live_proxy(&db, root.path(), repair).unwrap());

        let repaired = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(repaired.status, "complete");
        assert_eq!(repaired.proxy_ext.as_deref(), Some("flac"));
        assert!(recording_path.is_file());
        assert!(paths::proxy_path(root.path(), session_id, "flac").is_file());
    }

    #[test]
    fn live_session_creation_attaches_selected_tags_in_the_same_transaction() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let tag = crate::library::tags::create_or_get(&db, "Team meeting").unwrap();
        let session_id = SessionId::new();

        create_live_session_with_tags(&db, session_id, "Live session", &[tag.id]).unwrap();

        let attached = db
            .with_connection(|conn| Ok(repo::tags::list_for_session(conn, session_id)?))
            .unwrap();
        assert_eq!(attached.len(), 1);
        assert_eq!(attached[0].id, tag.id);
    }

    #[test]
    fn live_session_creation_rolls_back_when_a_selected_tag_no_longer_exists() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let session_id = SessionId::new();
        let missing_tag = crate::core::id::TagId::new();

        let error = create_live_session_with_tags(&db, session_id, "Live session", &[missing_tag])
            .unwrap_err();

        assert_eq!(error.code, Code::Request);
        let session = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?))
            .unwrap();
        assert!(session.is_none());
    }

    fn seed_finalizing_live(root: &Path, db: &Db) -> (SessionId, std::path::PathBuf) {
        let session_id = SessionId::new();
        create_live_session(db, session_id, "Live test").unwrap();
        let transcript_id = create_live_transcript(db, session_id, "live-test", None).unwrap();
        append_live_batch(
            db,
            session_id,
            transcript_id,
            &[text_segment(0.0, 0.5, "final transcript")],
            1.0,
            "finalizing",
        )
        .unwrap();

        let recording_path = paths::recording_path(root, session_id);
        fs::create_dir_all(recording_path.parent().unwrap()).unwrap();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&recording_path, spec).unwrap();
        for sample in 0..16_000 {
            writer.write_sample((sample % 100) as i16).unwrap();
        }
        writer.finalize().unwrap();
        (session_id, recording_path)
    }

    #[test]
    fn finalize_live_publishes_proxy_and_atomically_marks_session_complete() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let (session_id, recording_path) = seed_finalizing_live(root.path(), &db);

        let outcome =
            finalize_live_session(&db, root.path(), session_id, &recording_path, 2.0).unwrap();

        assert_eq!(outcome.session_id, session_id);
        assert!(outcome.proxy_error.is_none());
        assert!(paths::proxy_path(root.path(), session_id, "flac").is_file());
        let row = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(row.status, "complete");
        assert_eq!(row.proxy_ext.as_deref(), Some("flac"));
        assert!((row.duration_sec - 2.0).abs() < f64::EPSILON);
        assert!(recording_path.is_file());
    }

    #[test]
    fn finalize_live_proxy_failure_keeps_wav_and_commits_complete_without_proxy() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let (session_id, recording_path) = seed_finalizing_live(root.path(), &db);
        fault::set(Some(fault::Point::PublishWrite));

        let outcome =
            finalize_live_session(&db, root.path(), session_id, &recording_path, 1.0).unwrap();
        fault::set(None);

        assert!(outcome.proxy_error.is_some());
        assert!(recording_path.is_file());
        let row = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(row.status, "complete");
        assert_eq!(row.proxy_ext, None);
        assert!(get_detail(&db, root.path(), session_id)
            .unwrap()
            .unwrap()
            .proxy_path
            .is_none());
        assert!(staging_root_is_empty(root.path()));
    }

    #[test]
    fn finalize_live_database_failure_leaves_recovery_row_wav_and_published_proxy() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let (session_id, recording_path) = seed_finalizing_live(root.path(), &db);
        fault::set(Some(fault::Point::Commit));

        let error =
            finalize_live_session(&db, root.path(), session_id, &recording_path, 2.0).unwrap_err();
        fault::set(None);

        assert_eq!(error.code, Code::Storage);
        assert!(recording_path.is_file());
        assert!(paths::proxy_path(root.path(), session_id, "flac").is_file());
        let row = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, session_id)?.unwrap()))
            .unwrap();
        assert_eq!(row.status, "finalizing");
        assert_eq!(row.proxy_ext, None);
        assert!((row.duration_sec - 1.0).abs() < f64::EPSILON);
        let segments = db
            .with_connection(|conn| {
                let transcript_id = repo::transcripts::primary_for_session(conn, session_id)?
                    .expect("recoverable Live row retains its transcript");
                Ok(repo::segments::list_for_transcript(conn, transcript_id)?)
            })
            .unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "final transcript");
    }

    fn text_segment(start: f64, end: f64, text: &str) -> SegmentDraft {
        SegmentDraft {
            start_sec: start,
            end_sec: end,
            kind: SegmentKind::Text,
            gap_reason: None,
            text: text.to_string(),
            speaker: None,
        }
    }

    fn gap_segment(start: f64, end: f64, reason: GapReason) -> SegmentDraft {
        SegmentDraft {
            start_sec: start,
            end_sec: end,
            kind: SegmentKind::Gap,
            gap_reason: Some(reason),
            text: String::new(),
            speaker: None,
        }
    }

    /// Đếm thư mục Phiên trực tiếp dưới `media/`, bỏ qua `.staging` (không
    /// phải một thư mục Phiên — spec Design Notes).
    fn count_session_dirs(root: &Path) -> usize {
        fs::read_dir(paths::media_root(root))
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name() != ".staging")
            .count()
    }

    /// `relink_proxy` only ever discards its own throwaway job's staging
    /// subdirectory (`media/.staging/<job-id>`) — `media/.staging` itself
    /// may already exist (and stay, empty) from an earlier
    /// `commit_file_session` in the same test, so "no staging left" is
    /// "the root is gone or has no entries", not "the root doesn't exist".
    fn staging_root_is_empty(root: &Path) -> bool {
        match fs::read_dir(paths::staging_root(root)) {
            Ok(mut entries) => entries.next().is_none(),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => true,
            Err(err) => panic!("không đọc được staging root: {err}"),
        }
    }

    fn sample_draft(source_hash: Option<&str>) -> FileSessionDraft {
        FileSessionDraft {
            session: SessionDraft {
                title: "cuộc họp".to_string(),
                source_hash: source_hash.map(str::to_string),
                source_name: Some("meeting.mp4".to_string()),
                duration_sec: 3.0,
            },
            transcript: TranscriptDraft {
                model: "gemini-flash-lite-latest".to_string(),
                language: None,
                segments: vec![text_segment(0.0, 1.0, "xin chào")],
            },
        }
    }

    #[test]
    fn get_export_data_reads_speaker_and_gap_from_the_selected_transcript() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let session_id = SessionId::new();
        let outcome = commit_file_session(
            &db,
            root.path(),
            JobId::new(),
            session_id,
            FileSessionDraft {
                session: SessionDraft {
                    title: "meeting".to_string(),
                    source_hash: None,
                    source_name: None,
                    duration_sec: 5.0,
                },
                transcript: TranscriptDraft {
                    model: "model".to_string(),
                    language: Some("en".to_string()),
                    segments: vec![
                        SegmentDraft {
                            start_sec: 0.0,
                            end_sec: 2.0,
                            kind: SegmentKind::Text,
                            gap_reason: None,
                            text: "hello".to_string(),
                            speaker: Some("speaker-a".to_string()),
                        },
                        gap_segment(2.0, 3.0, GapReason::ChunkFailed),
                    ],
                },
            },
            Err(storage_error("no proxy in export test")),
        )
        .unwrap();
        let transcript_id = db
            .with_connection(|conn| {
                Ok(repo::transcripts::primary_for_session(conn, outcome.session_id)?.unwrap())
            })
            .unwrap();

        let data = get_export_data(&db, session_id, transcript_id)
            .unwrap()
            .unwrap();
        assert_eq!(data.transcript.session_id, session_id);
        assert_eq!(data.segments[0].speaker.as_deref(), Some("speaker-a"));
        assert_eq!(data.segments[1].kind, SegmentKind::Gap);
        assert_eq!(data.segments[1].gap_reason, Some(GapReason::ChunkFailed));
        assert!(get_export_data(&db, session_id, TranscriptId::new())
            .unwrap()
            .is_none());
    }

    /// P0 review fix (IDOR close): a `transcript_id` that exists but belongs
    /// to a *different* session must be treated exactly like "does not
    /// exist" -- `get_export_data` never returns data across a session
    /// boundary just because the caller happened to know/guess the bare
    /// transcript id (spec I/O Matrix "Export wrong session").
    #[test]
    fn get_export_data_returns_none_when_the_session_id_does_not_own_the_transcript() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let owning_session_id = SessionId::new();
        let outcome = commit_file_session(
            &db,
            root.path(),
            JobId::new(),
            owning_session_id,
            sample_draft(None),
            Err(storage_error("no proxy in export ownership test")),
        )
        .unwrap();
        let transcript_id = db
            .with_connection(|conn| {
                Ok(repo::transcripts::primary_for_session(conn, outcome.session_id)?.unwrap())
            })
            .unwrap();

        let other_session_id = SessionId::new();
        assert!(get_export_data(&db, other_session_id, transcript_id)
            .unwrap()
            .is_none());
        // The transcript itself is untouched and still exportable under its
        // real owning session.
        assert!(get_export_data(&db, owning_session_id, transcript_id)
            .unwrap()
            .is_some());
    }

    /// Dựng một Proxy staging thật (không phụ thuộc `media::create_proxy`,
    /// chỉ cần một file FLAC tối thiểu tồn tại đúng chỗ) để test publish mà
    /// không kéo theo toàn bộ pipeline decode/encode của story 2.1.
    fn stage_fake_proxy(root: &Path, job_id: JobId) -> std::path::PathBuf {
        let dir = paths::staging_dir(root, job_id);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("proxy-{}.flac", uuid::Uuid::now_v7()));
        fs::write(&path, b"fLaC-fake-content").unwrap();
        path
    }

    #[test]
    fn commit_publishes_proxy_writes_one_transaction_and_clears_staging() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);

        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some("hash-a")),
            Ok(staged),
        )
        .unwrap();

        assert!(outcome.proxy_error.is_none());
        let proxy_file = paths::proxy_path(root.path(), outcome.session_id, "flac");
        assert!(proxy_file.is_file());
        assert!(!paths::staging_dir(root.path(), job_id).exists());

        db.with_connection(|conn| {
            let session = repo::sessions::get(conn, outcome.session_id)?.unwrap();
            assert_eq!(session.kind, "file");
            assert_eq!(session.status, "complete");
            assert_eq!(session.proxy_ext.as_deref(), Some("flac"));
            assert_eq!(session.source_hash.as_deref(), Some("hash-a"));

            let sessions = repo::sessions::list(conn)?;
            assert_eq!(sessions.len(), 1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn commit_with_gap_reason_chunk_failed_marks_transcript_partial() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let mut draft = sample_draft(None);
        draft
            .transcript
            .segments
            .push(gap_segment(1.0, 2.0, GapReason::ChunkFailed));

        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            draft,
            Err(storage_error("no proxy")),
        )
        .unwrap();

        db.with_connection(|conn| {
            let rows: Vec<String> = conn
                .prepare("SELECT status FROM transcripts WHERE session_id = ?1")
                .unwrap()
                .query_map([outcome.session_id.to_string()], |row| row.get(0))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            assert_eq!(rows, vec!["partial".to_string()]);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn commit_with_only_disconnected_gap_marks_transcript_complete() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let mut draft = sample_draft(None);
        draft
            .transcript
            .segments
            .push(gap_segment(1.0, 2.0, GapReason::Disconnected));

        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            draft,
            Err(storage_error("no proxy")),
        )
        .unwrap();

        let session = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, outcome.session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(session.status, "complete");
    }

    #[test]
    fn commit_when_staged_proxy_is_err_still_commits_with_proxy_missing() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();

        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(None),
            Err(storage_error("staging không tạo được proxy")),
        )
        .unwrap();

        assert!(outcome.proxy_error.is_some());
        let session = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, outcome.session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(session.proxy_ext, None);
        assert_eq!(session.status, "complete", "Proxy lỗi không đổi status");
    }

    #[test]
    fn commit_publish_write_failure_still_commits_transcript_with_proxy_missing() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);

        fault::set(Some(fault::Point::PublishWrite));
        let result = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(None),
            Ok(staged),
        );
        fault::set(None);

        let outcome = result.unwrap();
        assert!(outcome.proxy_error.is_some());
        let session = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, outcome.session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(session.proxy_ext, None);
        // Spec I/O Matrix "Publish fails": publish write fails, commit ok ->
        // no `media/<sid>/` left (the directory `publish_proxy` creates up
        // front for the rename target must not survive an empty and
        // orphaned).
        assert!(
            !paths::media_dir(root.path(), outcome.session_id).exists(),
            "publish write thất bại phải dọn media/<sid>/ mồ côi"
        );
    }

    #[test]
    fn commit_publish_rename_failure_still_commits_transcript_with_proxy_missing() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);

        fault::set(Some(fault::Point::PublishRename));
        let result = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(None),
            Ok(staged),
        );
        fault::set(None);

        let outcome = result.unwrap();
        assert!(outcome.proxy_error.is_some());
        assert!(
            !paths::media_dir(root.path(), outcome.session_id).exists(),
            "publish rename thất bại phải dọn media/<sid>/ mồ côi"
        );
    }

    #[test]
    fn commit_duplicate_source_hash_rolls_back_and_removes_published_proxy() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());

        let job_a = JobId::new();
        let staged_a = stage_fake_proxy(root.path(), job_a);
        commit_file_session(
            &db,
            root.path(),
            job_a,
            SessionId::new(),
            sample_draft(Some("dup")),
            Ok(staged_a),
        )
        .unwrap();

        let job_b = JobId::new();
        let staged_b = stage_fake_proxy(root.path(), job_b);
        let err = commit_file_session(
            &db,
            root.path(),
            job_b,
            SessionId::new(),
            sample_draft(Some("dup")),
            Ok(staged_b),
        )
        .unwrap_err();
        assert_eq!(err.category, crate::core::error::Category::Storage);

        db.with_connection(|conn| {
            let sessions = repo::sessions::list(conn)?;
            assert_eq!(sessions.len(), 1, "chỉ Phiên commit đầu tiên còn tồn tại");
            Ok(())
        })
        .unwrap();

        // Thư mục media của lần commit thất bại phải bị gỡ hẳn, không mồ côi
        // trong `media/` sau rollback.
        assert_eq!(count_session_dirs(root.path()), 1);
    }

    #[test]
    fn commit_fails_when_commit_boundary_fault_injected_leaves_no_row_and_reconcile_cleans_folder()
    {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);

        fault::set(Some(fault::Point::Commit));
        let err = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some("x")),
            Ok(staged),
        );
        fault::set(None);
        assert!(err.is_err());

        db.with_connection(|conn| {
            assert_eq!(repo::sessions::list(conn)?.len(), 0);
            Ok(())
        })
        .unwrap();
        // Injected trước `tx.commit()` -> publish đã xảy ra nhưng DB rollback
        // -> gỡ ngay trong `commit_file_session`, không cần đợi reconcile.
        assert_eq!(count_session_dirs(root.path()), 0);

        // Reconcile hai lần vẫn sạch, không lỗi.
        reconcile(&db, root.path()).unwrap();
        reconcile(&db, root.path()).unwrap();
    }

    #[test]
    fn discard_staging_removes_dir_and_is_idempotent_when_missing() {
        let root = tempdir().unwrap();
        let job_id = JobId::new();
        stage_fake_proxy(root.path(), job_id);
        assert!(paths::staging_dir(root.path(), job_id).exists());

        discard_staging(root.path(), job_id).unwrap();
        assert!(!paths::staging_dir(root.path(), job_id).exists());

        // Gọi lại trên thư mục đã không còn tồn tại vẫn `Ok`.
        discard_staging(root.path(), job_id).unwrap();
    }

    #[test]
    fn get_returns_summary_for_a_committed_session_and_none_when_missing() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let session_id = SessionId::new();
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            session_id,
            sample_draft(Some("summary-hash")),
            Err(storage_error("no proxy")),
        )
        .unwrap();
        assert_eq!(outcome.session_id, session_id);

        let summary = get(&db, session_id).unwrap().unwrap();
        assert_eq!(summary.session_id, session_id);
        assert_eq!(summary.title, "cuộc họp");
        assert_eq!(summary.status, "complete");

        assert!(get(&db, SessionId::new()).unwrap().is_none());
    }

    #[test]
    fn discard_staging_failure_after_successful_commit_is_only_logged_not_propagated() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);

        fault::set(Some(fault::Point::Discard));
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some("y")),
            Ok(staged),
        );
        fault::set(None);

        assert!(
            outcome.is_ok(),
            "lỗi dọn staging không được làm hỏng một commit đã thành công"
        );
        // Reconcile dọn nốt staging còn sót.
        reconcile(&db, root.path()).unwrap();
        assert!(!paths::staging_dir(root.path(), job_id).exists());
    }

    #[test]
    fn replace_primary_transcript_keeps_session_id_and_swaps_segments() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some("z")),
            Ok(staged),
        )
        .unwrap();

        let new_transcript_id = replace_primary_transcript(
            &db,
            outcome.session_id,
            TranscriptDraft {
                model: "gemini-2.5".to_string(),
                language: Some("vi".to_string()),
                segments: vec![text_segment(0.0, 2.0, "bản chạy lại")],
            },
        )
        .unwrap();

        db.with_connection(|conn| {
            let session = repo::sessions::get(conn, outcome.session_id)?.unwrap();
            assert_eq!(session.id, outcome.session_id, "session.id giữ nguyên");

            let mut stmt = conn
                .prepare("SELECT id FROM transcripts WHERE session_id = ?1 AND variant = 'primary'")
                .unwrap();
            let ids: Vec<String> = stmt
                .query_map([outcome.session_id.to_string()], |row| row.get(0))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            assert_eq!(ids, vec![new_transcript_id.to_string()]);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn replace_primary_transcript_failure_leaves_old_primary_intact() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some("w")),
            Ok(staged),
        )
        .unwrap();

        let old_primary_id: String = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT id FROM transcripts WHERE session_id = ?1 AND variant = 'primary'",
                    [outcome.session_id.to_string()],
                    |row| row.get(0),
                )?)
            })
            .unwrap();

        fault::set(Some(fault::Point::Commit));
        let err = replace_primary_transcript(
            &db,
            outcome.session_id,
            TranscriptDraft {
                model: "m2".to_string(),
                language: None,
                segments: vec![text_segment(0.0, 1.0, "bị huỷ")],
            },
        );
        fault::set(None);
        assert!(err.is_err());

        let current_primary_id: String = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT id FROM transcripts WHERE session_id = ?1 AND variant = 'primary'",
                    [outcome.session_id.to_string()],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(current_primary_id, old_primary_id);
    }

    #[test]
    fn swap_transcript_replaces_primary_when_expected_id_still_matches() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some("swap-a")),
            Ok(staged),
        )
        .unwrap();

        let old_id = db
            .with_connection(|conn| {
                Ok(repo::transcripts::primary_for_session(
                    conn,
                    outcome.session_id,
                )?)
            })
            .unwrap()
            .unwrap();

        let new_id = swap_transcript(
            &db,
            outcome.session_id,
            old_id,
            TranscriptDraft {
                model: "gemini-2.5".to_string(),
                language: None,
                segments: vec![text_segment(0.0, 2.0, "chạy lại")],
            },
        )
        .unwrap();
        assert!(new_id.is_some());
        assert_ne!(new_id, Some(old_id));

        let current_primary_id = db
            .with_connection(|conn| {
                Ok(repo::transcripts::primary_for_session(
                    conn,
                    outcome.session_id,
                )?)
            })
            .unwrap();
        assert_eq!(current_primary_id, new_id);

        let session = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, outcome.session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(
            session.id, outcome.session_id,
            "session_id giữ nguyên sau swap"
        );
    }

    #[test]
    fn swap_transcript_returns_none_and_writes_nothing_when_expected_id_is_stale() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some("swap-b")),
            Ok(staged),
        )
        .unwrap();
        let real_primary_id = db
            .with_connection(|conn| {
                Ok(repo::transcripts::primary_for_session(
                    conn,
                    outcome.session_id,
                )?)
            })
            .unwrap()
            .unwrap();

        let stale_id = TranscriptId::new();
        let result = swap_transcript(
            &db,
            outcome.session_id,
            stale_id,
            TranscriptDraft {
                model: "m2".to_string(),
                language: None,
                segments: vec![text_segment(0.0, 1.0, "không được ghi")],
            },
        )
        .unwrap();
        assert_eq!(result, None);

        let current_primary_id = db
            .with_connection(|conn| {
                Ok(repo::transcripts::primary_for_session(
                    conn,
                    outcome.session_id,
                )?)
            })
            .unwrap()
            .unwrap();
        assert_eq!(current_primary_id, real_primary_id);
    }

    #[test]
    fn swap_transcript_failure_leaves_old_primary_intact() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some("swap-c")),
            Ok(staged),
        )
        .unwrap();
        let old_primary_id = db
            .with_connection(|conn| {
                Ok(repo::transcripts::primary_for_session(
                    conn,
                    outcome.session_id,
                )?)
            })
            .unwrap()
            .unwrap();

        fault::set(Some(fault::Point::Commit));
        let err = swap_transcript(
            &db,
            outcome.session_id,
            old_primary_id,
            TranscriptDraft {
                model: "m2".to_string(),
                language: None,
                segments: vec![text_segment(0.0, 1.0, "bị huỷ")],
            },
        );
        fault::set(None);
        assert!(err.is_err());

        let current_primary_id = db
            .with_connection(|conn| {
                Ok(repo::transcripts::primary_for_session(
                    conn,
                    outcome.session_id,
                )?)
            })
            .unwrap()
            .unwrap();
        assert_eq!(current_primary_id, old_primary_id);
    }

    #[test]
    fn get_reflects_partial_and_primary_transcript_id() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);
        let mut draft = sample_draft(Some("partial-summary"));
        draft
            .transcript
            .segments
            .push(gap_segment(1.0, 2.0, GapReason::ChunkFailed));
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            draft,
            Ok(staged),
        )
        .unwrap();

        let summary = get(&db, outcome.session_id).unwrap().unwrap();
        assert!(summary.partial);
        let primary_id = db
            .with_connection(|conn| {
                Ok(repo::transcripts::primary_for_session(
                    conn,
                    outcome.session_id,
                )?)
            })
            .unwrap();
        assert_eq!(summary.primary_transcript_id, primary_id);
    }

    #[test]
    fn reconcile_removes_orphan_session_folder_with_no_db_row() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let orphan_id = SessionId::new();
        let orphan_dir = paths::media_dir(root.path(), orphan_id);
        fs::create_dir_all(&orphan_dir).unwrap();
        fs::write(orphan_dir.join("proxy.flac"), b"orphan").unwrap();

        reconcile(&db, root.path()).unwrap();

        assert!(!orphan_dir.exists());
    }

    #[test]
    fn reconcile_removes_stray_entries_directly_under_media_root() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let media_root = paths::media_root(root.path());
        fs::create_dir_all(&media_root).unwrap();
        fs::write(media_root.join("not-a-session-id.txt"), b"stray").unwrap();

        reconcile(&db, root.path()).unwrap();

        assert!(!media_root.join("not-a-session-id.txt").exists());
    }

    #[test]
    fn reconcile_wipes_the_entire_staging_root() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        stage_fake_proxy(root.path(), JobId::new());
        stage_fake_proxy(root.path(), JobId::new());
        assert!(paths::staging_root(root.path()).exists());

        reconcile(&db, root.path()).unwrap();

        assert!(!paths::staging_root(root.path()).exists());
    }

    #[test]
    fn reconcile_nulls_proxy_ext_when_the_referenced_file_is_gone() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some("v")),
            Ok(staged),
        )
        .unwrap();

        // Mô phỏng file Proxy biến mất sau khi đã commit thành công.
        fs::remove_file(paths::proxy_path(root.path(), outcome.session_id, "flac")).unwrap();

        reconcile(&db, root.path()).unwrap();

        let session = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, outcome.session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(session.proxy_ext, None);
    }

    #[test]
    fn reconcile_removes_unreferenced_stray_files_inside_a_valid_session_dir() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some("u")),
            Ok(staged),
        )
        .unwrap();

        let dir = paths::media_dir(root.path(), outcome.session_id);
        fs::write(dir.join("proxy-leftover.flac.partial"), b"stray").unwrap();

        reconcile(&db, root.path()).unwrap();

        assert!(dir.join("proxy.flac").is_file(), "Proxy đang dùng vẫn còn");
        assert!(!dir.join("proxy-leftover.flac.partial").exists());
    }

    #[test]
    fn reconcile_preserves_recording_only_for_live_sessions_and_removes_other_strays() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let live_id = SessionId::new();
        let file_id = SessionId::new();
        db.with_connection(|conn| {
            for (id, kind) in [(live_id, "live"), (file_id, "file")] {
                repo::sessions::insert(
                    conn,
                    repo::sessions::NewSession {
                        id,
                        kind,
                        title: "session",
                        source_hash: None,
                        source_name: None,
                        status: if kind == "live" {
                            "recording"
                        } else {
                            "complete"
                        },
                        recovered: false,
                        duration_sec: 0.0,
                        proxy_ext: None,
                        created_at: 1,
                        updated_at: 1,
                    },
                )?;
            }
            Ok(())
        })
        .unwrap();

        let live_dir = paths::media_dir(root.path(), live_id);
        let file_dir = paths::media_dir(root.path(), file_id);
        fs::create_dir_all(&live_dir).unwrap();
        fs::create_dir_all(&file_dir).unwrap();
        fs::write(live_dir.join("recording.wav"), b"live wav bytes").unwrap();
        fs::write(file_dir.join("recording.wav"), b"stray file-session data").unwrap();
        fs::write(live_dir.join("unrelated.tmp"), b"stray").unwrap();

        reconcile(&db, root.path()).unwrap();

        assert!(live_dir.join("recording.wav").is_file());
        assert!(!live_dir.join("unrelated.tmp").exists());
        assert!(!file_dir.join("recording.wav").exists());
        db.with_connection(|conn| {
            assert_eq!(
                repo::sessions::get(conn, live_id)?.unwrap().status,
                "recording"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn reconcile_logs_and_continues_past_one_unreadable_session_directory() {
        // Spec I/O Matrix "Reconcile one bad": "one session dir errors ->
        // others still reconciled, warn" -- an unreadable directory (real
        // `read_dir` failure, not the file-fault-injection points, which
        // only cover `commit_file_session`'s own publish/commit steps) makes
        // `reconcile_session_dir` return `Err` for exactly that one session.
        use std::os::unix::fs::PermissionsExt;

        let root = tempdir().unwrap();
        let db = open_db(root.path());

        let job_bad = JobId::new();
        let staged_bad = stage_fake_proxy(root.path(), job_bad);
        let bad_outcome = commit_file_session(
            &db,
            root.path(),
            job_bad,
            SessionId::new(),
            sample_draft(Some("bad")),
            Ok(staged_bad),
        )
        .unwrap();
        let bad_dir = paths::media_dir(root.path(), bad_outcome.session_id);
        let mut denied = fs::metadata(&bad_dir).unwrap().permissions();
        denied.set_mode(0o000);
        fs::set_permissions(&bad_dir, denied).unwrap();

        let job_good = JobId::new();
        let staged_good = stage_fake_proxy(root.path(), job_good);
        let good_outcome = commit_file_session(
            &db,
            root.path(),
            job_good,
            SessionId::new(),
            sample_draft(Some("good")),
            Ok(staged_good),
        )
        .unwrap();
        let good_dir = paths::media_dir(root.path(), good_outcome.session_id);
        fs::write(good_dir.join("stray.partial"), b"x").unwrap();

        let result = reconcile(&db, root.path());

        // Restore permissions so the tempdir can clean itself up regardless
        // of the assertions below.
        let mut restored = fs::metadata(&bad_dir).unwrap().permissions();
        restored.set_mode(0o700);
        fs::set_permissions(&bad_dir, restored).unwrap();

        assert!(
            result.is_ok(),
            "một Phiên lỗi không được làm cả reconcile thất bại: {result:?}"
        );
        assert!(
            !good_dir.join("stray.partial").exists(),
            "Phiên tốt vẫn phải được reconcile dù Phiên khác lỗi"
        );
    }

    #[test]
    fn reconcile_never_touches_a_previously_committed_session_and_is_idempotent() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some("t")),
            Ok(staged),
        )
        .unwrap();
        let proxy_file = paths::proxy_path(root.path(), outcome.session_id, "flac");
        let bytes_before = fs::read(&proxy_file).unwrap();

        reconcile(&db, root.path()).unwrap();
        reconcile(&db, root.path()).unwrap();

        assert_eq!(fs::read(&proxy_file).unwrap(), bytes_before);
        let session = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, outcome.session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(session.proxy_ext.as_deref(), Some("flac"));
    }

    #[test]
    fn reconcile_on_a_fresh_root_with_no_media_dir_is_a_noop() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        reconcile(&db, root.path()).unwrap();
    }

    // `get_detail`/`relink_proxy` (story 2.7) — spec I/O Matrix "Mở Phiên",
    // "Proxy thiếu", "Chọn lại khớp/sai", "Phiên live thiếu Proxy".

    /// Ghi một WAV mono 48 kHz thật (không phải nội dung fLaC giả như
    /// `stage_fake_proxy`) — `relink_proxy` thành công phải chạy qua
    /// `media::create_proxy` (decode + encode) thật, không chỉ đổi tên file.
    fn write_real_wav(path: &Path, seconds: u32) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for frame in 0..(48_000 * seconds) {
            let sample = ((frame as f32 * 0.01).sin() * i16::MAX as f32) as i16;
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
    }

    fn insert_live_session(db: &Db) -> SessionId {
        let session_id = SessionId::new();
        db.with_connection(|conn| {
            repo::sessions::insert(
                conn,
                repo::sessions::NewSession {
                    id: session_id,
                    kind: "live",
                    title: "phiên live",
                    source_hash: None,
                    source_name: None,
                    status: "complete",
                    recovered: false,
                    duration_sec: 5.0,
                    proxy_ext: None,
                    created_at: 0,
                    updated_at: 0,
                },
            )?;
            Ok(())
        })
        .unwrap();
        session_id
    }

    #[test]
    fn get_detail_returns_none_when_session_is_missing() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        assert!(get_detail(&db, root.path(), SessionId::new())
            .unwrap()
            .is_none());
    }

    #[test]
    fn get_detail_includes_transcript_segments_and_a_proxy_path_that_exists_on_disk() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);
        let mut draft = sample_draft(Some("detail-hash"));
        draft
            .transcript
            .segments
            .push(gap_segment(1.0, 2.0, GapReason::ChunkFailed));
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            draft,
            Ok(staged),
        )
        .unwrap();

        let detail = get_detail(&db, root.path(), outcome.session_id)
            .unwrap()
            .unwrap();
        assert_eq!(detail.session_id, outcome.session_id);
        assert_eq!(detail.kind, "file");
        assert_eq!(detail.title, "cuộc họp");
        assert_eq!(detail.source_name.as_deref(), Some("meeting.mp4"));
        assert!(!detail.recovered);
        let proxy_path = detail.proxy_path.expect("proxy đã publish phải có path");
        assert!(Path::new(&proxy_path).is_file());

        let transcript = detail.transcript.expect("Phiên đã commit luôn có primary");
        assert_eq!(transcript.variant, "primary");
        assert_eq!(transcript.status, "partial");
        assert_eq!(transcript.segments.len(), 2);
        assert_eq!(transcript.segments[0].kind, "text");
        assert_eq!(transcript.segments[0].text, "xin chào");
        assert_eq!(transcript.segments[1].kind, "gap");
        assert_eq!(
            transcript.segments[1].gap_reason.as_deref(),
            Some("chunk_failed")
        );
    }

    #[test]
    fn get_detail_includes_the_sessions_tags_and_ignores_other_sessions_tags() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let outcome = commit_file_session(
            &db,
            root.path(),
            JobId::new(),
            SessionId::new(),
            sample_draft(None),
            Err(storage_error("no proxy in tags test")),
        )
        .unwrap();
        let other = commit_file_session(
            &db,
            root.path(),
            JobId::new(),
            SessionId::new(),
            sample_draft(None),
            Err(storage_error("no proxy in tags test")),
        )
        .unwrap();

        let tag = crate::library::tags::create_or_get(&db, "khách A").unwrap();
        crate::library::tags::attach_tag(&db, outcome.session_id, tag.id).unwrap();

        let detail = get_detail(&db, root.path(), outcome.session_id)
            .unwrap()
            .unwrap();
        assert_eq!(detail.tags, vec![tag]);

        let other_detail = get_detail(&db, root.path(), other.session_id)
            .unwrap()
            .unwrap();
        assert!(other_detail.tags.is_empty());
    }

    /// Story 3.7 spec Boundaries Always: "Badge `memo` ... khi Phiên có ít
    /// nhất một memo" -- `has_memo` phản ánh đúng `repo::memos`, không phụ
    /// thuộc gì khác.
    #[test]
    fn get_detail_has_memo_reflects_whether_a_memo_row_exists_for_the_session() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let outcome = commit_file_session(
            &db,
            root.path(),
            JobId::new(),
            SessionId::new(),
            sample_draft(None),
            Err(storage_error("no proxy in has_memo test")),
        )
        .unwrap();

        assert!(
            !get_detail(&db, root.path(), outcome.session_id)
                .unwrap()
                .unwrap()
                .has_memo
        );

        let body = crate::core::sensitive::Sensitive::new("# Memo".to_string());
        let prompt = crate::core::sensitive::Sensitive::new("{transcript}".to_string());
        db.with_connection(|conn| {
            let transcript_id = repo::transcripts::primary_for_session(conn, outcome.session_id)?
                .expect("commit_file_session luôn tạo transcript primary");
            Ok(repo::memos::upsert(
                conn,
                outcome.session_id,
                crate::core::id::MemoTemplateId::new(),
                &body,
                0,
                transcript_id,
                "complete",
                None,
                "Tên",
                &prompt,
                "m",
            )?)
        })
        .unwrap();

        assert!(
            get_detail(&db, root.path(), outcome.session_id)
                .unwrap()
                .unwrap()
                .has_memo
        );
    }

    #[test]
    fn get_detail_proxy_path_is_none_when_proxy_ext_is_null_or_file_is_gone() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();

        let missing_ext = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some("no-ext")),
            Err(storage_error("no proxy")),
        )
        .unwrap();
        assert!(get_detail(&db, root.path(), missing_ext.session_id)
            .unwrap()
            .unwrap()
            .proxy_path
            .is_none());

        let job_id_b = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id_b);
        let has_ext = commit_file_session(
            &db,
            root.path(),
            job_id_b,
            SessionId::new(),
            sample_draft(Some("gone-file")),
            Ok(staged),
        )
        .unwrap();
        fs::remove_file(paths::proxy_path(root.path(), has_ext.session_id, "flac")).unwrap();
        assert!(get_detail(&db, root.path(), has_ext.session_id)
            .unwrap()
            .unwrap()
            .proxy_path
            .is_none());
    }

    #[test]
    fn relink_proxy_rejects_a_live_session_without_touching_anything() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let session_id = insert_live_session(&db);
        let picked = root.path().join("picked.wav");
        write_real_wav(&picked, 1);

        let outcome = relink_proxy(&db, root.path(), session_id, &picked).unwrap();
        assert_eq!(outcome, RelinkOutcome::NotFileSession);
        assert!(staging_root_is_empty(root.path()));
    }

    #[test]
    fn relink_proxy_returns_hash_mismatch_and_leaves_the_old_proxy_untouched() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some("expected-hash")),
            Ok(staged),
        )
        .unwrap();
        let proxy_file = paths::proxy_path(root.path(), outcome.session_id, "flac");
        let bytes_before = fs::read(&proxy_file).unwrap();

        let picked = root.path().join("wrong.wav");
        write_real_wav(&picked, 1);
        let result = relink_proxy(&db, root.path(), outcome.session_id, &picked).unwrap();

        assert_eq!(result, RelinkOutcome::HashMismatch);
        assert_eq!(fs::read(&proxy_file).unwrap(), bytes_before);
        assert!(staging_root_is_empty(root.path()));
        let session = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, outcome.session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(session.proxy_ext.as_deref(), Some("flac"));
    }

    #[test]
    fn relink_proxy_publishes_a_new_proxy_and_updates_proxy_ext_when_hash_matches() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);

        let source = root.path().join("source.wav");
        write_real_wav(&source, 1);
        let source_hash = media::sha256_file(&source).unwrap();

        let draft = sample_draft(Some(&source_hash));
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            draft,
            Ok(staged),
        )
        .unwrap();
        let proxy_file = paths::proxy_path(root.path(), outcome.session_id, "flac");
        let bytes_before = fs::read(&proxy_file).unwrap();

        let result = relink_proxy(&db, root.path(), outcome.session_id, &source).unwrap();

        assert_eq!(result, RelinkOutcome::Relinked);
        assert!(staging_root_is_empty(root.path()));
        let bytes_after = fs::read(&proxy_file).unwrap();
        assert_ne!(
            bytes_before, bytes_after,
            "publish phải thay hẳn nội dung Proxy cũ bằng Proxy mới"
        );
        let session = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, outcome.session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(session.proxy_ext.as_deref(), Some("flac"));

        let detail = get_detail(&db, root.path(), outcome.session_id)
            .unwrap()
            .unwrap();
        assert!(detail.proxy_path.is_some());
    }

    #[test]
    fn relink_proxy_publish_write_failure_leaves_no_staging_and_keeps_the_old_proxy() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);

        let source = root.path().join("source.wav");
        write_real_wav(&source, 1);
        let source_hash = media::sha256_file(&source).unwrap();
        let draft = sample_draft(Some(&source_hash));
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            draft,
            Ok(staged),
        )
        .unwrap();
        let proxy_file = paths::proxy_path(root.path(), outcome.session_id, "flac");
        let bytes_before = fs::read(&proxy_file).unwrap();

        fault::set(Some(fault::Point::PublishWrite));
        let result = relink_proxy(&db, root.path(), outcome.session_id, &source);
        fault::set(None);

        assert!(result.is_err());
        assert!(staging_root_is_empty(root.path()));
        assert_eq!(fs::read(&proxy_file).unwrap(), bytes_before);
    }

    #[test]
    fn relink_proxy_retries_set_proxy_ext_once_and_still_succeeds() {
        // Spec Always: "if `set_proxy_ext` fails after the file was
        // published, retry the DB write once" -- one failure must not fail
        // the whole relink.
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);

        let source = root.path().join("source.wav");
        write_real_wav(&source, 1);
        let source_hash = media::sha256_file(&source).unwrap();
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some(&source_hash)),
            Ok(staged),
        )
        .unwrap();

        fault::set_proxy_ext_failures(1);
        let result = relink_proxy(&db, root.path(), outcome.session_id, &source);
        fault::set_proxy_ext_failures(0);

        assert_eq!(result.unwrap(), RelinkOutcome::Relinked);
        let session = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, outcome.session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(session.proxy_ext.as_deref(), Some("flac"));
    }

    #[test]
    fn relink_proxy_set_proxy_ext_failing_twice_reports_the_mismatch_and_reconcile_repairs_it() {
        // Spec I/O Matrix "Relink DB fails": "publish ok, set_proxy_ext
        // fails twice -> error; later reconcile sets `proxy_ext`" (`Code::
        // Storage`). The published file must stay on disk -- the fix is
        // never to roll it back, only to report and let the next
        // `reconcile` repair the DB.
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);

        let source = root.path().join("source.wav");
        write_real_wav(&source, 1);
        let source_hash = media::sha256_file(&source).unwrap();

        // Start from a session whose `proxy_ext` is already NULL (its own
        // first publish failed) -- the same starting point a `relink_proxy`
        // DB-write failure leaves behind: a proxy file on disk with no
        // recorded extension.
        fault::set(Some(fault::Point::PublishWrite));
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some(&source_hash)),
            Ok(staged),
        )
        .unwrap();
        fault::set(None);
        assert!(outcome.proxy_error.is_some());

        fault::set_proxy_ext_failures(2);
        let result = relink_proxy(&db, root.path(), outcome.session_id, &source);
        fault::set_proxy_ext_failures(0);

        let error = result.expect_err("set_proxy_ext lỗi cả hai lần phải trả Err");
        assert_eq!(error.code, Code::Storage);

        let proxy_file = paths::proxy_path(root.path(), outcome.session_id, "flac");
        assert!(
            proxy_file.is_file(),
            "file Proxy mới publish rồi phải giữ nguyên trên đĩa, không rollback"
        );
        let session = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, outcome.session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(
            session.proxy_ext, None,
            "DB chưa ghi được -- đúng như lỗi báo, chưa tự sửa ở đây"
        );

        reconcile(&db, root.path()).unwrap();

        let repaired = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, outcome.session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(
            repaired.proxy_ext.as_deref(),
            Some("flac"),
            "reconcile phải tự sửa proxy_ext từ file thật trên đĩa"
        );
    }

    // -----------------------------------------------------------------
    // Story 3.1: `rename_session` / `delete_session`.
    // -----------------------------------------------------------------

    #[test]
    fn rename_session_trims_and_saves_the_new_title() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(None),
            Err(storage_error("no proxy")),
        )
        .unwrap();

        let saved = rename_session(&db, outcome.session_id, "  Họp sprint 12  ").unwrap();
        assert_eq!(saved.as_deref(), Some("Họp sprint 12"));

        let session = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, outcome.session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(session.title, "Họp sprint 12");
        assert_eq!(
            session.source_hash, None,
            "đổi tên không được đụng source_hash/source_name (spec Never)"
        );
    }

    #[test]
    fn rename_session_rejects_empty_or_whitespace_only_title() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let outcome = commit_file_session(
            &db,
            root.path(),
            JobId::new(),
            SessionId::new(),
            sample_draft(None),
            Err(storage_error("no proxy")),
        )
        .unwrap();

        let err = rename_session(&db, outcome.session_id, "   ").unwrap_err();
        assert_eq!(err.code, Code::Request);

        let session = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, outcome.session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(session.title, "cuộc họp", "tên cũ phải giữ nguyên");
    }

    #[test]
    fn rename_session_rejects_a_title_over_200_unicode_scalars() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let outcome = commit_file_session(
            &db,
            root.path(),
            JobId::new(),
            SessionId::new(),
            sample_draft(None),
            Err(storage_error("no proxy")),
        )
        .unwrap();

        let too_long = "x".repeat(201);
        let err = rename_session(&db, outcome.session_id, &too_long).unwrap_err();
        assert_eq!(err.code, Code::Request);

        // Exactly 200 is allowed.
        let exactly_200 = "y".repeat(200);
        let saved = rename_session(&db, outcome.session_id, &exactly_200).unwrap();
        assert_eq!(saved, Some(exactly_200));
    }

    #[test]
    fn rename_session_returns_none_when_the_session_no_longer_exists() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        assert_eq!(
            rename_session(&db, SessionId::new(), "tên mới").unwrap(),
            None
        );
    }

    #[test]
    fn delete_session_removes_db_row_transcript_segments_and_media_dir() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some("delete-a")),
            Ok(staged),
        )
        .unwrap();
        assert!(paths::media_dir(root.path(), outcome.session_id).is_dir());

        delete_session(&db, root.path(), outcome.session_id).unwrap();

        db.with_connection(|conn| {
            assert!(repo::sessions::get(conn, outcome.session_id)?.is_none());
            let transcript_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM transcripts WHERE session_id = ?1",
                [outcome.session_id.to_string()],
                |row| row.get(0),
            )?;
            assert_eq!(transcript_count, 0);
            Ok(())
        })
        .unwrap();
        assert!(
            !paths::media_dir(root.path(), outcome.session_id).exists(),
            "media/<sid>/ phải biến mất, không còn file mồ côi"
        );
    }

    #[test]
    fn delete_session_is_idempotent_for_an_already_missing_session() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        // Không dòng DB, không thư mục -- vẫn `Ok(())` (spec I/O Matrix "Xoá
        // session không tồn tại").
        delete_session(&db, root.path(), SessionId::new()).unwrap();
    }

    #[test]
    fn delete_session_retried_after_the_db_row_is_already_gone_still_cleans_the_media_dir() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some("delete-b")),
            Ok(staged),
        )
        .unwrap();

        // Simulate a crash between the DB delete and the media cleanup: drop
        // the DB row directly, leaving the directory behind.
        db.with_connection(|conn| Ok(repo::sessions::delete(conn, outcome.session_id)?))
            .unwrap();
        assert!(paths::media_dir(root.path(), outcome.session_id).is_dir());

        // A retried delete on a session whose DB row is already gone must
        // still clean up the orphaned directory (spec I/O Matrix "Xoá lỗi
        // FS": "Gọi lại ... dọn thư mục mồ côi", idempotent).
        delete_session(&db, root.path(), outcome.session_id).unwrap();
        assert!(!paths::media_dir(root.path(), outcome.session_id).exists());
    }

    #[test]
    fn delete_session_fs_failure_still_leaves_the_db_row_deleted_and_reports_storage() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some("delete-c")),
            Ok(staged),
        )
        .unwrap();

        fault::set(Some(fault::Point::DeleteMediaDir));
        let err = delete_session(&db, root.path(), outcome.session_id).unwrap_err();
        fault::set(None);
        assert_eq!(err.category, crate::core::error::Category::Storage);

        // DB row must already be gone -- caller never reports "Đã xoá" in
        // this branch, but a retry (or boot reconcile) does not resurrect it
        // either (spec Always: "không báo thành công").
        let session = db
            .with_connection(|conn| Ok(repo::sessions::get(conn, outcome.session_id)?))
            .unwrap();
        assert!(session.is_none());
    }

    #[test]
    fn storage_stats_on_a_fresh_root_is_all_zero() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        // `Db::open` đã ghi migration -- `app.db`/`app.db-wal`/`app.db-shm`
        // đã tồn tại nhưng nhỏ; chỉ khẳng định không lỗi và `session_count`
        // đúng 0, không khẳng định `db_bytes == 0` (spec I/O Matrix "Kho
        // rỗng" nói về xoá, không nói DB luôn rỗng byte).
        let stats = storage_stats(&db, root.path()).unwrap();
        assert_eq!(stats.media_bytes, 0.0);
        assert_eq!(stats.session_count, 0);
        assert!(stats.db_bytes >= 0.0);
    }

    #[test]
    fn storage_stats_counts_media_bytes_and_sessions() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);
        let proxy_len = fs::metadata(&staged).unwrap().len();
        commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some("stats-a")),
            Ok(staged),
        )
        .unwrap();

        let stats = storage_stats(&db, root.path()).unwrap();
        assert_eq!(stats.media_bytes, proxy_len as f64);
        assert_eq!(stats.session_count, 1);
    }

    #[test]
    fn wipe_all_removes_sessions_tags_and_media_but_keeps_settings() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);
        let outcome = commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some("wipe-a")),
            Ok(staged),
        )
        .unwrap();
        let tag = crate::library::tags::create_or_get(&db, "a").unwrap();
        crate::library::tags::attach_tag(&db, outcome.session_id, tag.id).unwrap();

        let settings = crate::settings::Settings {
            onboarding_completed: true,
            consent_accepted_version: 7,
            ..crate::settings::Settings::default()
        };
        crate::settings::save(&db, &settings).unwrap();

        wipe_all(&db, root.path()).unwrap();

        db.with_connection(|conn| {
            let sessions: i64 = conn
                .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
                .unwrap();
            let tags: i64 = conn
                .query_row("SELECT COUNT(*) FROM tags", [], |row| row.get(0))
                .unwrap();
            assert_eq!(sessions, 0);
            assert_eq!(tags, 0);
            Ok(())
        })
        .unwrap();
        assert!(
            !paths::media_root(root.path()).exists(),
            "media/ phải biến mất hoàn toàn, kể cả .staging"
        );

        let reloaded = crate::settings::load(&db);
        assert_eq!(reloaded, settings, "settings phải giữ nguyên sau khi xoá");
    }

    #[test]
    fn wipe_all_on_an_empty_store_is_idempotent() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        wipe_all(&db, root.path()).unwrap();
        wipe_all(&db, root.path()).unwrap();
    }

    /// Story 3.6 spec Never: "Xoá toàn bộ dữ liệu (3.4) giữ nguyên
    /// `memo_templates`" -- seed hai mẫu mặc định + một mẫu người dùng, xoá
    /// toàn bộ, rồi kiểm cả ba dòng còn nguyên.
    #[test]
    fn wipe_all_keeps_memo_templates() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        crate::memo::templates::list(&db, "vi").unwrap();
        crate::memo::templates::create(&db, "Của tôi".to_string(), "{transcript}".to_string())
            .unwrap();

        wipe_all(&db, root.path()).unwrap();

        let count: i64 = db
            .with_connection(|conn| {
                Ok(conn.query_row("SELECT COUNT(*) FROM memo_templates", [], |row| row.get(0))?)
            })
            .unwrap();
        assert_eq!(
            count, 3,
            "memo_templates phải giữ nguyên sau khi xoá toàn bộ"
        );
    }

    #[test]
    fn wipe_all_fs_failure_still_leaves_db_rows_deleted_and_a_retry_finishes_the_cleanup() {
        let root = tempdir().unwrap();
        let db = open_db(root.path());
        let job_id = JobId::new();
        let staged = stage_fake_proxy(root.path(), job_id);
        commit_file_session(
            &db,
            root.path(),
            job_id,
            SessionId::new(),
            sample_draft(Some("wipe-b")),
            Ok(staged),
        )
        .unwrap();

        fault::set(Some(fault::Point::WipeMediaDir));
        let err = wipe_all(&db, root.path()).unwrap_err();
        fault::set(None);
        assert_eq!(err.category, crate::core::error::Category::Storage);

        db.with_connection(|conn| {
            let sessions: i64 = conn
                .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
                .unwrap();
            assert_eq!(sessions, 0, "DB đã commit dù bước xoá media lỗi sau đó");
            Ok(())
        })
        .unwrap();
        assert!(paths::media_root(root.path()).exists());

        // Gọi lại dọn nốt phần còn lại (spec I/O Matrix "Lỗi FS giữa
        // chừng": "gọi lại xoá tiếp được").
        wipe_all(&db, root.path()).unwrap();
        assert!(!paths::media_root(root.path()).exists());
    }
}
