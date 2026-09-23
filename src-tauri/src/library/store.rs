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
use std::fs::{self, File};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::error::{AppError, Code};
use crate::core::id::{JobId, SessionId, TranscriptId};
use crate::core::paths;
use crate::db::repo::{self, segments::SegmentDraft, transcripts::Variant};
use crate::db::Db;

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
    }

    thread_local! {
        static INJECT: Cell<Option<Point>> = const { Cell::new(None) };
    }

    pub fn set(point: Option<Point>) {
        INJECT.with(|cell| cell.set(point));
    }

    pub fn should_fail(point: Point) -> bool {
        INJECT.with(|cell| cell.get() == Some(point))
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
            Err(err) => (None, Some(err)),
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
            let _ = fs::remove_dir_all(paths::media_dir(root, session_id));
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
/// Phiên, không phải toàn bộ `SessionRow`.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSummary {
    pub session_id: SessionId,
    pub title: String,
    pub duration_sec: f64,
    pub status: String,
}

/// Đọc tóm tắt một Phiên theo id, `None` nếu không còn tồn tại (spec I/O
/// Matrix "`/session/:id`": "Phiên đã lưu / không còn").
pub fn get(db: &Db, session_id: SessionId) -> Result<Option<SessionSummary>, AppError> {
    db.with_connection(|conn| {
        Ok(
            repo::sessions::get(conn, session_id)?.map(|row| SessionSummary {
                session_id: row.id,
                title: row.title,
                duration_sec: row.duration_sec,
                status: row.status,
            }),
        )
    })
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

fn remove_path_any(path: &Path) {
    if fs::remove_file(path).is_err() {
        let _ = fs::remove_dir_all(path);
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
///    mọi entry khác trong thư mục đó ngoài đúng tên Proxy hiện tại (`.partial`
///    hay tên lạ) đều bị xoá.
///
/// Chạy lại nhiều lần cho cùng kết quả: sau lần đầu, không còn gì để dọn nên
/// các lần sau là no-op (spec Always: "Reconcile chạy lại nhiều lần cho cùng
/// kết quả").
pub fn reconcile(db: &Db, root: &Path) -> Result<(), AppError> {
    remove_path_if_exists(&paths::staging_root(root))?;

    let media_root = paths::media_root(root);
    let entries = match fs::read_dir(&media_root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(storage_error(&err.to_string())),
    };

    let known: HashMap<SessionId, Option<String>> = db
        .with_connection(|conn| Ok(repo::sessions::list_media_refs(conn)?.into_iter().collect()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            remove_path_any(&path);
            continue;
        };
        let Ok(session_id) = SessionId::try_from(name.as_str()) else {
            // Không parse được thành UUIDv7 -> không phải thư mục Phiên hợp
            // lệ (bao gồm cả trường hợp `.staging` lỡ tái tạo giữa hai bước
            // trên do một tiến trình khác — vẫn dọn được an toàn).
            remove_path_any(&path);
            continue;
        };
        match known.get(&session_id) {
            None => remove_path_any(&path),
            Some(proxy_ext) if path.is_dir() => {
                reconcile_session_dir(db, session_id, proxy_ext.as_deref(), &path)?;
            }
            Some(_) => remove_path_any(&path),
        }
    }

    Ok(())
}

fn reconcile_session_dir(
    db: &Db,
    session_id: SessionId,
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

    let keep_name = if proxy_file_exists {
        expected_name
    } else {
        None
    };

    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(storage_error(&err.to_string())),
    };
    for entry in entries.flatten() {
        let is_kept = keep_name
            .as_deref()
            .map(|keep| entry.file_name().to_str() == Some(keep))
            .unwrap_or(false);
        if !is_kept {
            remove_path_any(&entry.path());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::segments::{GapReason, SegmentKind};
    use tempfile::tempdir;

    fn open_db(root: &Path) -> Db {
        Db::open(root).unwrap()
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
}
