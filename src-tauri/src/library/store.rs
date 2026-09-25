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

    let now = now_ms();
    if let Err(err) = db.with_connection(|conn| {
        Ok(repo::sessions::set_proxy_ext(
            conn,
            session_id,
            Some(&ext),
            now,
        )?)
    }) {
        let _ = discard_staging(root, job_id);
        return Err(err);
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
        assert!(
            get_export_data(&db, session_id, TranscriptId::new())
                .unwrap()
                .is_none()
        );
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
        assert!(
            get_export_data(&db, other_session_id, transcript_id)
                .unwrap()
                .is_none()
        );
        // The transcript itself is untouched and still exportable under its
        // real owning session.
        assert!(
            get_export_data(&db, owning_session_id, transcript_id)
                .unwrap()
                .is_some()
        );
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
}
