//! Ghi chú tự lưu (story 3.5, spec Approach): validate độ dài, bọc
//! `Sensitive<String>` trước khi chạm `db::repo::notes`, và ánh xạ
//! `SaveOutcome` nội bộ sang kiểu công khai `NotesSaveOutcome` qua IPC. Đây
//! là chủ ghi duy nhất cho `notes` ngoài `db/repo/notes.rs` -- `ipc::` gọi
//! qua đây, không gọi thẳng `repo::notes`.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::error::{AppError, Code};
use crate::core::id::SessionId;
use crate::core::sensitive::Sensitive;
use crate::db::repo;
use crate::db::Db;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// Giới hạn độ dài ghi chú tính theo Unicode scalar (spec Boundaries Always:
/// "Body giới hạn 100 000 ký tự (vượt → lỗi `Request`)").
pub const MAX_BODY_SCALARS: usize = 100_000;

/// Ghi chú tại ACK gần nhất -- `body` là `String` công khai qua IPC (spec
/// Always: "payload IPC là `String`"); `Sensitive<String>` chỉ tồn tại trong
/// Rust nội bộ (`repo::notes`), không lộ ra kiểu này.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NoteSnapshot {
    pub body: String,
    /// `i32` không phải `i64` -- cùng lý do `TagWithCount::session_count`:
    /// specta-typescript cấm xuất kiểu BigInt, và revision (tăng dần theo
    /// lần gõ dừng của một Phiên) không bao giờ tới gần `i32::MAX`.
    pub revision: i32,
    /// Mili-giây kể từ Unix epoch (UTC), giữ dạng `f64` -- cùng quy ước
    /// `SessionDetail::created_at` (một mốc thời gian có thể vượt
    /// `i32::MAX` nhưng vẫn nguyên trong dải số nguyên chính xác của `f64`).
    pub updated_at: f64,
}

/// Kết quả `notes_save` (spec Boundaries Always). `rename_all_fields`
/// khớp quy ước `SessionLookup` (`ipc/mod.rs`) cho enum có biến thể mang dữ
/// liệu qua IPC.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NotesSaveOutcome {
    Saved { revision: i32, updated_at: f64 },
    Stale { revision: i32 },
    NotFound,
}

fn validate_body(raw: String) -> Result<Sensitive<String>, AppError> {
    if raw.chars().count() > MAX_BODY_SCALARS {
        return Err(AppError::new(
            Code::Request,
            format!("Ghi chú vượt quá {MAX_BODY_SCALARS} ký tự"),
        ));
    }
    Ok(Sensitive::new(raw))
}

/// Đọc ghi chú đã lưu của một Phiên -- `Ok(None)` khi chưa từng lưu (không
/// phải lỗi, frontend hiện textarea rỗng).
pub fn get(db: &Db, session_id: SessionId) -> Result<Option<NoteSnapshot>, AppError> {
    db.with_connection(|conn| {
        Ok(repo::notes::get(conn, session_id)?.map(|row| NoteSnapshot {
            body: row.body.into_inner(),
            revision: row.revision as i32,
            updated_at: row.updated_at as f64,
        }))
    })
}

/// Lưu ghi chú nếu `revision` mới lớn hơn revision đang lưu (spec Boundaries
/// Always). Validate độ dài *trước* khi chạm DB (spec I/O Matrix: một body
/// quá dài không bao giờ tạo giao dịch DB) -- lỗi trả `Code::Request`, cùng
/// category với các lỗi validate khác trong `library::` (ví dụ
/// `library::tags::validate_name`).
pub fn save(
    db: &Db,
    session_id: SessionId,
    body: String,
    revision: i32,
) -> Result<NotesSaveOutcome, AppError> {
    let body = validate_body(body)?;
    let now = now_ms();
    db.with_connection(|conn| {
        let outcome = repo::notes::save_if_newer(conn, session_id, &body, revision as i64, now)?;
        Ok(match outcome {
            repo::notes::SaveOutcome::Saved {
                revision,
                updated_at,
            } => NotesSaveOutcome::Saved {
                revision: revision as i32,
                updated_at: updated_at as f64,
            },
            repo::notes::SaveOutcome::Stale { revision } => NotesSaveOutcome::Stale {
                revision: revision as i32,
            },
            repo::notes::SaveOutcome::NotFound => NotesSaveOutcome::NotFound,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::sessions::{self, NewSession};
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
    fn get_is_none_before_the_first_save() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        assert_eq!(get(&db, session_id).unwrap(), None);
    }

    #[test]
    fn save_then_get_round_trips_the_body_and_revision() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);

        let outcome = save(&db, session_id, "ghi chú đầu".to_string(), 1).unwrap();
        assert!(matches!(
            outcome,
            NotesSaveOutcome::Saved { revision: 1, .. }
        ));

        let snapshot = get(&db, session_id).unwrap().unwrap();
        assert_eq!(snapshot.body, "ghi chú đầu");
        assert_eq!(snapshot.revision, 1);
    }

    #[test]
    fn save_rejects_a_body_longer_than_the_limit_without_writing() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        let too_long = "a".repeat(MAX_BODY_SCALARS + 1);

        let err = save(&db, session_id, too_long, 1).unwrap_err();
        assert_eq!(err.category, crate::core::error::Category::Model);
        assert_eq!(get(&db, session_id).unwrap(), None, "không được ghi gì");
    }

    /// Đúng giới hạn (100 000 ký tự) vẫn hợp lệ -- chỉ *vượt* mới bị từ
    /// chối.
    #[test]
    fn save_accepts_a_body_exactly_at_the_limit() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        let exact = "a".repeat(MAX_BODY_SCALARS);

        let outcome = save(&db, session_id, exact, 1).unwrap();
        assert!(matches!(outcome, NotesSaveOutcome::Saved { .. }));
    }

    #[test]
    fn save_returns_stale_and_does_not_overwrite_a_newer_revision() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        save(&db, session_id, "mới nhất".to_string(), 5).unwrap();

        let outcome = save(&db, session_id, "cũ hơn".to_string(), 4).unwrap();
        assert_eq!(outcome, NotesSaveOutcome::Stale { revision: 5 });
        assert_eq!(get(&db, session_id).unwrap().unwrap().body, "mới nhất");
    }

    #[test]
    fn save_returns_not_found_for_a_session_that_does_not_exist() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        let outcome = save(&db, session_id, "mồ côi".to_string(), 1).unwrap();
        assert_eq!(outcome, NotesSaveOutcome::NotFound);
    }

    /// Spec I/O Matrix "Chạy lại / xoá phiên": xoá Phiên dọn ghi chú theo
    /// cascade (đã kiểm ở `db::migrations`) -- test này kiểm thêm qua tầng
    /// `library::notes::get` để chắc không có state nào khác giữ lại ghi chú
    /// đã "xoá".
    #[test]
    fn deleting_the_session_removes_its_note() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        save(&db, session_id, "sẽ mất".to_string(), 1).unwrap();

        db.with_connection(|conn| Ok(sessions::delete(conn, session_id)?))
            .unwrap();

        assert_eq!(get(&db, session_id).unwrap(), None);
    }

    /// Spec Boundaries Always: "không log/diagnostics nào chứa nội dung" --
    /// `NoteSnapshot` mang `body` công khai (đây là hợp đồng IPC, không phải
    /// rò rỉ: `Sensitive<String>` chỉ bọc ở tầng repo nội bộ), nhưng bất kỳ
    /// giá trị nội bộ nào đi qua `Sensitive` (như `repo::notes::NoteRow`)
    /// không bao giờ lộ nội dung qua `Debug` -- cùng bài kiểm dạng
    /// `core/log.rs::fake_session_log_never_leaks_raw_secrets_to_disk`.
    #[test]
    fn internal_note_row_debug_never_leaks_the_body() {
        let secret = Sensitive::new("nội dung ghi chú tuyệt mật".to_string());
        let debug = format!("{secret:?}");
        assert!(!debug.contains("tuyệt mật"));
        assert_eq!(debug, "[redacted]");
    }
}
