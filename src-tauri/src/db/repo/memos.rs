//! SQL cho `memos` — nơi duy nhất khác ngoài `db/migrations/mod.rs` được phép
//! chứa câu SQL cho bảng này (spec Boundaries). `body`/`template_prompt` đi
//! qua ranh giới repo dưới dạng `Sensitive<String>` (spec Boundaries Always:
//! "nội dung memo/prompt/ghi chú không vào log") -- module này chỉ đọc/ghi
//! thẳng, quyết định `UpsertOutcome` từ hình dạng kết quả SQL (cùng khuôn
//! `repo::notes::save_if_newer`), không tự quyết định prompt/provenance.

use rusqlite::{params, Connection, OptionalExtension};

use crate::core::id::{MemoTemplateId, SessionId, TranscriptId};
use crate::core::sensitive::Sensitive;

fn parse_transcript_id(raw: &str) -> rusqlite::Result<TranscriptId> {
    TranscriptId::try_from(raw).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("giá trị `memos.transcript_id` không phải UUIDv7 hợp lệ: {raw}").into(),
        )
    })
}

/// Một dòng `memos` — `body`/`template_prompt` bọc `Sensitive<String>` nên
/// `Debug` của `MemoRow` không bao giờ in nội dung thật.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoRow {
    pub body: Sensitive<String>,
    pub created_at: i64,
    pub transcript_id: TranscriptId,
    /// Chuỗi thô `"complete" | "partial"` -- cùng quy ước
    /// `repo::transcripts::Status::as_str`, memo không tự map sang enum
    /// riêng cho một giá trị chỉ dùng lại để hiển thị dòng nguồn.
    pub transcript_status: String,
    pub notes_revision: Option<i64>,
    pub template_name: String,
    pub template_prompt: Sensitive<String>,
    pub model: String,
}

fn row_to_memo(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoRow> {
    let transcript_id: String = row.get(2)?;
    Ok(MemoRow {
        body: Sensitive::new(row.get(0)?),
        created_at: row.get(1)?,
        transcript_id: parse_transcript_id(&transcript_id)?,
        transcript_status: row.get(3)?,
        notes_revision: row.get(4)?,
        template_name: row.get(5)?,
        template_prompt: Sensitive::new(row.get(6)?),
        model: row.get(7)?,
    })
}

const SELECT_COLUMNS: &str = "body, created_at, transcript_id, transcript_status, \
     notes_revision, template_name, template_prompt, model";

/// Đọc memo đã cache của một cặp (Phiên, Template) -- `None` khi chưa từng
/// sinh (spec Boundaries Always: "`memo_get` ... Mở lại memo chỉ đọc DB,
/// không gọi Gemini").
pub fn get(
    conn: &Connection,
    session_id: SessionId,
    template_id: MemoTemplateId,
) -> rusqlite::Result<Option<MemoRow>> {
    let sql =
        format!("SELECT {SELECT_COLUMNS} FROM memos WHERE session_id = ?1 AND template_id = ?2");
    conn.query_row(
        &sql,
        params![session_id.to_string(), template_id.to_string()],
        row_to_memo,
    )
    .optional()
}

/// `true` khi Phiên có ít nhất một memo -- dùng cho badge `memo` ở
/// `SessionHeader` (spec Boundaries Always: "Badge `memo` ... khi Phiên có
/// ít nhất một memo").
pub fn exists_for_session(conn: &Connection, session_id: SessionId) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM memos WHERE session_id = ?1)",
        params![session_id.to_string()],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count != 0)
}

/// Kết quả [`upsert`] (spec Boundaries Always: "commit sau đó (FK lỗi/Phiên
/// không còn) coi như `Cancelled`, không tái tạo dữ liệu"). `SessionGone`
/// phân biệt "đã ghi" với "Phiên bị xoá đúng lúc commit" để caller
/// (`memo::generate`) ánh xạ đúng sang outcome `Cancelled` của lệnh IPC thay
/// vì trả một `AppError` không đâu vào đâu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertOutcome {
    Saved,
    SessionGone,
}

/// Thay hẳn memo của cặp (Phiên, Template) bằng bản mới -- `INSERT ...
/// ON CONFLICT(session_id, template_id) DO UPDATE` nguyên tử, không có cửa
/// sổ đua giữa đọc dòng cũ và ghi dòng mới (spec Boundaries Always: "Chỉ khi
/// thành công **và** commit DB mới thay memo cũ của cặp"). Vi phạm FK
/// `session_id → sessions(id)` (Phiên đã bị xoá giữa lúc Gemini đang trả
/// lời) bắt ở đây thay vì kiểm tồn tại trước bằng một truy vấn riêng, cùng
/// kỹ thuật `repo::notes::save_if_newer`.
#[allow(clippy::too_many_arguments)]
pub fn upsert(
    conn: &Connection,
    session_id: SessionId,
    template_id: MemoTemplateId,
    body: &Sensitive<String>,
    created_at: i64,
    transcript_id: TranscriptId,
    transcript_status: &str,
    notes_revision: Option<i64>,
    template_name: &str,
    template_prompt: &Sensitive<String>,
    model: &str,
) -> rusqlite::Result<UpsertOutcome> {
    let result = conn.execute(
        "INSERT INTO memos (session_id, template_id, body, created_at, transcript_id, \
         transcript_status, notes_revision, template_name, template_prompt, model) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
         ON CONFLICT(session_id, template_id) DO UPDATE SET \
             body = excluded.body, created_at = excluded.created_at, \
             transcript_id = excluded.transcript_id, \
             transcript_status = excluded.transcript_status, \
             notes_revision = excluded.notes_revision, \
             template_name = excluded.template_name, \
             template_prompt = excluded.template_prompt, \
             model = excluded.model",
        params![
            session_id.to_string(),
            template_id.to_string(),
            body.expose(),
            created_at,
            transcript_id.to_string(),
            transcript_status,
            notes_revision,
            template_name,
            template_prompt.expose(),
            model,
        ],
    );
    match result {
        Ok(_) => Ok(UpsertOutcome::Saved),
        Err(rusqlite::Error::SqliteFailure(err, _))
            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            Ok(UpsertOutcome::SessionGone)
        }
        Err(other) => Err(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::sessions::{self, NewSession};
    use crate::db::Db;
    use tempfile::tempdir;

    fn open_db() -> (tempfile::TempDir, Db) {
        let dir = tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        (dir, db)
    }

    fn insert_session(db: &Db, id: SessionId) {
        db.with_connection(|conn| {
            Ok(sessions::insert(
                conn,
                NewSession {
                    id,
                    kind: "file",
                    title: "t",
                    source_hash: None,
                    source_name: None,
                    status: "complete",
                    recovered: false,
                    duration_sec: 1.0,
                    proxy_ext: None,
                    created_at: 0,
                    updated_at: 0,
                },
            )?)
        })
        .unwrap();
    }

    #[test]
    fn get_returns_none_when_never_generated() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        let row = db
            .with_connection(|conn| Ok(get(conn, session_id, MemoTemplateId::new())?))
            .unwrap();
        assert_eq!(row, None);
    }

    #[test]
    fn upsert_inserts_then_replaces_the_cached_memo() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        let template_id = MemoTemplateId::new();
        insert_session(&db, session_id);
        let transcript_id = TranscriptId::new();
        let body = Sensitive::new("# Memo bản đầu".to_string());
        let prompt = Sensitive::new("{transcript}".to_string());

        let outcome = db
            .with_connection(|conn| {
                Ok(upsert(
                    conn,
                    session_id,
                    template_id,
                    &body,
                    1_000,
                    transcript_id,
                    "complete",
                    Some(3),
                    "Biên bản họp",
                    &prompt,
                    "gemini-flash-lite-latest",
                )?)
            })
            .unwrap();
        assert_eq!(outcome, UpsertOutcome::Saved);

        let row = db
            .with_connection(|conn| Ok(get(conn, session_id, template_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(row.body.expose(), "# Memo bản đầu");
        assert_eq!(row.transcript_id, transcript_id);
        assert_eq!(row.notes_revision, Some(3));

        // Sinh lại: thay đúng dòng cũ, không nhân bản.
        let new_transcript_id = TranscriptId::new();
        let second_body = Sensitive::new("# Memo bản hai".to_string());
        db.with_connection(|conn| {
            Ok(upsert(
                conn,
                session_id,
                template_id,
                &second_body,
                2_000,
                new_transcript_id,
                "partial",
                None,
                "Biên bản họp",
                &prompt,
                "gemini-flash-lite-latest",
            )?)
        })
        .unwrap();

        let row = db
            .with_connection(|conn| Ok(get(conn, session_id, template_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(row.body.expose(), "# Memo bản hai");
        assert_eq!(row.transcript_id, new_transcript_id);
        assert_eq!(row.notes_revision, None);
        assert_eq!(row.transcript_status, "partial");

        let count: i64 = db
            .with_connection(|conn| {
                Ok(conn.query_row("SELECT COUNT(*) FROM memos", [], |r| r.get(0))?)
            })
            .unwrap();
        assert_eq!(count, 1, "upsert không được nhân bản dòng");
    }

    /// Spec Boundaries Always: "commit sau đó (FK lỗi/Phiên không còn) coi
    /// như `Cancelled`".
    #[test]
    fn upsert_reports_session_gone_instead_of_erroring_when_the_session_was_deleted() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        let template_id = MemoTemplateId::new();
        // Cố ý không `insert_session` -- mô phỏng Phiên đã bị xoá giữa lúc
        // Gemini đang trả lời.
        let body = Sensitive::new("mồ côi".to_string());
        let prompt = Sensitive::new("{transcript}".to_string());
        let outcome = db
            .with_connection(|conn| {
                Ok(upsert(
                    conn,
                    session_id,
                    template_id,
                    &body,
                    0,
                    TranscriptId::new(),
                    "complete",
                    None,
                    "Tên",
                    &prompt,
                    "m",
                )?)
            })
            .unwrap();
        assert_eq!(outcome, UpsertOutcome::SessionGone);
        assert_eq!(
            db.with_connection(|conn| Ok(get(conn, session_id, template_id)?))
                .unwrap(),
            None
        );
    }

    #[test]
    fn exists_for_session_reflects_whether_any_memo_row_exists() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        assert!(!db
            .with_connection(|conn| Ok(exists_for_session(conn, session_id)?))
            .unwrap());

        let body = Sensitive::new("x".to_string());
        let prompt = Sensitive::new("{transcript}".to_string());
        db.with_connection(|conn| {
            Ok(upsert(
                conn,
                session_id,
                MemoTemplateId::new(),
                &body,
                0,
                TranscriptId::new(),
                "complete",
                None,
                "Tên",
                &prompt,
                "m",
            )?)
        })
        .unwrap();

        assert!(db
            .with_connection(|conn| Ok(exists_for_session(conn, session_id)?))
            .unwrap());
    }

    /// `template_id` không FK -- một memo sinh từ một template đã bị xoá
    /// trước đó vẫn `upsert`/`get` được bình thường (spec Boundaries Always:
    /// "Memo vẫn đọc được với tên snapshot").
    #[test]
    fn upsert_and_get_work_for_a_template_id_that_does_not_exist_in_memo_templates() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        let vanished_template_id = MemoTemplateId::new();
        let body = Sensitive::new("nội dung".to_string());
        let prompt = Sensitive::new("{transcript}".to_string());
        db.with_connection(|conn| {
            Ok(upsert(
                conn,
                session_id,
                vanished_template_id,
                &body,
                0,
                TranscriptId::new(),
                "complete",
                None,
                "Mẫu đã xoá",
                &prompt,
                "m",
            )?)
        })
        .unwrap();

        let row = db
            .with_connection(|conn| Ok(get(conn, session_id, vanished_template_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(row.template_name, "Mẫu đã xoá");
    }
}
