//! SQL cho `notes` — nơi duy nhất khác ngoài `db/migrations/mod.rs` được phép
//! chứa câu SQL cho bảng này (spec Boundaries). Chuẩn hoá/giới hạn độ dài/
//! `NotesSaveOutcome` công khai nằm ở `library::notes`, không ở đây — module
//! này chỉ đọc/ghi thẳng và quyết định `Saved`/`Stale`/`NotFound` từ hình
//! dạng kết quả SQL.

use rusqlite::{params, Connection, OptionalExtension};

use crate::core::id::SessionId;
use crate::core::sensitive::Sensitive;

/// Một dòng `notes` — `body` đi qua ranh giới repo dưới dạng `Sensitive<String>`
/// (spec Boundaries Always: "body đi qua repo/store dưới dạng
/// `Sensitive<String>`; không log/diagnostics nào chứa nội dung"): `Debug`
/// của `NoteRow` không bao giờ in nội dung ghi chú thật, kể cả khi ai đó lỡ
/// `tracing::debug!("{row:?}")` một `NoteRow`.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteRow {
    pub body: Sensitive<String>,
    pub revision: i64,
    pub updated_at: i64,
}

/// Kết quả `save_if_newer` (spec Boundaries Always: "trả `Saved { revision,
/// updatedAt }`, `Stale { revision }` (không ghi) hoặc `NotFound`").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveOutcome {
    Saved { revision: i64, updated_at: i64 },
    Stale { revision: i64 },
    NotFound,
}

/// Đọc ghi chú hiện tại của một Phiên -- `None` khi chưa từng lưu (chưa có
/// dòng `notes`), phân biệt với ghi chú rỗng (`body = ""`, một dòng có thật).
pub fn get(conn: &Connection, session_id: SessionId) -> rusqlite::Result<Option<NoteRow>> {
    conn.query_row(
        "SELECT body, revision, updated_at FROM notes WHERE session_id = ?1",
        params![session_id.to_string()],
        |row| {
            Ok(NoteRow {
                body: Sensitive::new(row.get(0)?),
                revision: row.get(1)?,
                updated_at: row.get(2)?,
            })
        },
    )
    .optional()
}

/// Upsert có điều kiện: chỉ ghi khi `revision` mới lớn hơn revision đang lưu
/// (spec Boundaries Always) -- `INSERT ... ON CONFLICT(session_id) DO UPDATE
/// ... WHERE excluded.revision > notes.revision` là một câu SQL nguyên tử,
/// không có cửa sổ đua giữa đọc và ghi kể cả khi hai lời gọi `notes_save`
/// chạy đồng thời trên cùng Phiên (spec Design Notes).
///
/// `NotFound` khi `session_id` không tồn tại trong `sessions` -- FK
/// `notes.session_id → sessions(id)` chặn `INSERT` bằng lỗi constraint, được
/// bắt ở đây thay vì kiểm tồn tại trước bằng một truy vấn riêng (một câu SQL
/// duy nhất, không có cửa sổ đua giữa kiểm và ghi nếu Phiên bị xoá đúng lúc
/// đó).
pub fn save_if_newer(
    conn: &Connection,
    session_id: SessionId,
    body: &Sensitive<String>,
    revision: i64,
    updated_at: i64,
) -> rusqlite::Result<SaveOutcome> {
    let changed = conn.execute(
        "INSERT INTO notes (session_id, body, revision, updated_at) VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(session_id) DO UPDATE SET \
             body = excluded.body, revision = excluded.revision, updated_at = excluded.updated_at \
         WHERE excluded.revision > notes.revision",
        params![session_id.to_string(), body.expose(), revision, updated_at],
    );
    match changed {
        Ok(rows) if rows > 0 => Ok(SaveOutcome::Saved {
            revision,
            updated_at,
        }),
        // 0 dòng đổi chỉ có thể tới từ nhánh `DO UPDATE ... WHERE` sai (dòng
        // đã tồn tại, `revision` mới không lớn hơn) -- một `INSERT` không
        // xung đột luôn ghi đúng 1 dòng hoặc lỗi, không bao giờ trả 0. Dòng
        // đã tồn tại nên đọc lại `revision` hiện tại chắc chắn có kết quả.
        Ok(_) => {
            let current_revision: i64 = conn.query_row(
                "SELECT revision FROM notes WHERE session_id = ?1",
                params![session_id.to_string()],
                |row| row.get(0),
            )?;
            Ok(SaveOutcome::Stale {
                revision: current_revision,
            })
        }
        Err(rusqlite::Error::SqliteFailure(err, _))
            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            Ok(SaveOutcome::NotFound)
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
    fn get_returns_none_when_never_saved() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        let note = db
            .with_connection(|conn| Ok(get(conn, session_id)?))
            .unwrap();
        assert_eq!(note, None);
    }

    #[test]
    fn save_if_newer_inserts_the_first_revision() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        let body = Sensitive::new("xin chào".to_string());

        let outcome = db
            .with_connection(|conn| Ok(save_if_newer(conn, session_id, &body, 1, 1_000)?))
            .unwrap();
        assert_eq!(
            outcome,
            SaveOutcome::Saved {
                revision: 1,
                updated_at: 1_000
            }
        );

        let row = db
            .with_connection(|conn| Ok(get(conn, session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(row.body.expose(), "xin chào");
        assert_eq!(row.revision, 1);
        assert_eq!(row.updated_at, 1_000);
    }

    #[test]
    fn save_if_newer_overwrites_when_revision_is_strictly_greater() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        let first = Sensitive::new("bản đầu".to_string());
        let second = Sensitive::new("bản hai".to_string());

        db.with_connection(|conn| Ok(save_if_newer(conn, session_id, &first, 1, 1_000)?))
            .unwrap();
        let outcome = db
            .with_connection(|conn| Ok(save_if_newer(conn, session_id, &second, 2, 2_000)?))
            .unwrap();

        assert_eq!(
            outcome,
            SaveOutcome::Saved {
                revision: 2,
                updated_at: 2_000
            }
        );
        let row = db
            .with_connection(|conn| Ok(get(conn, session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(row.body.expose(), "bản hai");
    }

    /// Spec I/O Matrix "Stale": "DB rev 5, gửi rev 4 → `Stale{5}`, DB không
    /// đổi".
    #[test]
    fn save_if_newer_rejects_a_revision_that_is_not_greater_and_does_not_write() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        let body = Sensitive::new("giữ nguyên".to_string());
        db.with_connection(|conn| Ok(save_if_newer(conn, session_id, &body, 5, 5_000)?))
            .unwrap();

        let stale_body = Sensitive::new("không được ghi".to_string());
        let outcome = db
            .with_connection(|conn| Ok(save_if_newer(conn, session_id, &stale_body, 4, 6_000)?))
            .unwrap();

        assert_eq!(outcome, SaveOutcome::Stale { revision: 5 });
        let row = db
            .with_connection(|conn| Ok(get(conn, session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(row.body.expose(), "giữ nguyên", "DB không được đổi");
        assert_eq!(row.revision, 5);
    }

    /// Cùng `revision` (đua/gọi lại) cũng bị coi là stale -- chỉ `revision`
    /// lớn hơn nghiêm ngặt mới được ghi.
    #[test]
    fn save_if_newer_rejects_the_same_revision_again() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        let body = Sensitive::new("a".to_string());
        db.with_connection(|conn| Ok(save_if_newer(conn, session_id, &body, 1, 1_000)?))
            .unwrap();

        let outcome = db
            .with_connection(|conn| Ok(save_if_newer(conn, session_id, &body, 1, 2_000)?))
            .unwrap();
        assert_eq!(outcome, SaveOutcome::Stale { revision: 1 });
    }

    /// Spec Boundaries Always: `notes_save` từ chối khi Phiên không tồn tại.
    #[test]
    fn save_if_newer_returns_not_found_when_session_does_not_exist() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        let body = Sensitive::new("mồ côi".to_string());

        let outcome = db
            .with_connection(|conn| Ok(save_if_newer(conn, session_id, &body, 1, 1_000)?))
            .unwrap();
        assert_eq!(outcome, SaveOutcome::NotFound);
        assert_eq!(
            db.with_connection(|conn| Ok(get(conn, session_id)?))
                .unwrap(),
            None
        );
    }

    /// Spec I/O Matrix "Bền qua đóng đột ngột": ACK rev 3 rồi mở lại DB
    /// (không đóng kết nối đầu) đọc được đúng body rev 3.
    #[test]
    fn saved_note_survives_opening_a_second_connection_on_the_same_file() {
        let dir = tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        let body = Sensitive::new("bền qua reopen".to_string());
        db.with_connection(|conn| Ok(save_if_newer(conn, session_id, &body, 3, 3_000)?))
            .unwrap();

        // Mở một `Db` thứ hai trên cùng thư mục mà không đóng cái đầu --
        // WAL + `synchronous = FULL` phải khiến bản ghi đã ACK đọc được ngay
        // từ connection khác.
        let second = Db::open(dir.path()).unwrap();
        let row = second
            .with_connection(|conn| Ok(get(conn, session_id)?))
            .unwrap()
            .unwrap();
        assert_eq!(row.body.expose(), "bền qua reopen");
        assert_eq!(row.revision, 3);
    }
}
