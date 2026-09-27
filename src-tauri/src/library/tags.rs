//! Tag: chuẩn hoá tên, validate giới hạn, gọi `db::repo::tags` (story 3.2,
//! spec Approach: "module `library/tags.rs` + IPC gắn/gỡ/tạo/xoá toàn cục").
//! Đây là chủ ghi duy nhất cho `tags`/`session_tags` ngoài `db/repo/tags.rs`
//! -- `ipc::` gọi qua đây, không gọi thẳng `repo::tags`.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::core::error::{AppError, Code};
use crate::core::id::{SessionId, TagId};
use crate::db::repo;
use crate::db::Db;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// Giới hạn tên tag đếm theo Unicode scalar (spec Boundaries Always: "> 80 ký
/// tự (Unicode scalar) bị từ chối").
const MAX_NAME_SCALARS: usize = 80;

/// Giới hạn số tag mỗi Phiên (spec Boundaries Always: "Phiên đã có 20 tag thì
/// gắn thêm bị từ chối").
pub const MAX_TAGS_PER_SESSION: i64 = 20;

/// Chuẩn hoá một tên tag thô: trim rồi gộp mọi chuỗi khoảng trắng liên tiếp
/// (kể cả tab/xuống dòng, `str::split_whitespace`) thành đúng một dấu cách
/// (spec Boundaries Always: "`name` = trim + gộp khoảng trắng"; spec I/O
/// Matrix "Khoảng trắng": `"  sprint   12 "` → lưu `sprint 12`). `name_key` là
/// kết quả đó lowercase Unicode-aware qua `str::to_lowercase` của Rust — cố ý
/// không dùng `COLLATE NOCASE` của SQLite (chỉ ASCII-aware, spec Design
/// Notes: "`DỰ ÁN` → `dự án`"). Trả `None` khi chuỗi rỗng hoặc chỉ khoảng
/// trắng sau chuẩn hoá (spec I/O Matrix "Khoảng trắng": `"   "` → từ chối).
/// Frontend đã `normalize('NFC')` trước khi gửi (spec Design Notes) — hàm này
/// không tự làm lại NFC.
pub fn normalize_tag_name(raw: &str) -> Option<(String, String)> {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    let name_key = collapsed.to_lowercase();
    Some((collapsed, name_key))
}

fn validate_name(raw: &str) -> Result<(String, String), AppError> {
    let Some((name, name_key)) = normalize_tag_name(raw) else {
        return Err(AppError::new(Code::Request, "Tên tag không được để trống"));
    };
    if name.chars().count() > MAX_NAME_SCALARS {
        return Err(AppError::new(
            Code::Request,
            format!("Tên tag vượt quá {MAX_NAME_SCALARS} ký tự"),
        ));
    }
    Ok((name, name_key))
}

/// Tag tối giản (id + tên) — dùng cho `tags_create` và `SessionDetail::tags`
/// (spec Code Map).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TagSummary {
    pub id: TagId,
    pub name: String,
}

/// Tag kèm số Phiên đang gắn — dùng cho `tags_list` (spec Boundaries: "trả
/// tag kèm số phiên, sắp số phiên giảm dần rồi tên"). `session_count` là
/// `i32` (không phải `i64`), cùng quy ước với `SessionListItem::
/// missing_gap_count` — specta-typescript cấm xuất kiểu BigInt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TagWithCount {
    pub id: TagId,
    pub name: String,
    pub session_count: i32,
}

fn to_summary(row: repo::tags::TagRow) -> TagSummary {
    TagSummary {
        id: row.id,
        name: row.name,
    }
}

/// Tạo tag mới hoặc trả về tag đã có cùng `name_key` (spec I/O Matrix "Trùng
/// hoa thường": có `DỰ ÁN`, tạo `dự án` → trả tag `DỰ ÁN` sẵn có, không tạo
/// mới; "Tạo đồng thời": hai luồng tạo `Khách A` → đúng 1 dòng, cả hai nhận
/// cùng id). Validate rỗng/quá dài trước khi chạm DB (spec I/O Matrix "Khoảng
/// trắng", "Quá dài") — cả hai trả `Code::Request`.
pub fn create_or_get(db: &Db, raw_name: &str) -> Result<TagSummary, AppError> {
    let (name, name_key) = validate_name(raw_name)?;
    let id = TagId::new();
    let now = now_ms();
    let row = db.with_connection(|conn| {
        Ok(repo::tags::upsert_by_name_key(
            conn, id, &name, &name_key, now,
        )?)
    })?;
    Ok(to_summary(row))
}

/// Mọi tag kèm số Phiên, sắp số phiên giảm dần rồi tên (spec Boundaries).
pub fn list_with_counts(db: &Db) -> Result<Vec<TagWithCount>, AppError> {
    db.with_connection(|conn| {
        Ok(repo::tags::list_with_counts(conn)?
            .into_iter()
            .map(|row| TagWithCount {
                id: row.id,
                name: row.name,
                session_count: row.session_count as i32,
            })
            .collect())
    })
}

/// Mọi tag đang gắn với một Phiên, tên tăng dần — dùng bởi
/// `library::store::get_detail`.
pub fn list_for_session(db: &Db, session_id: SessionId) -> Result<Vec<TagSummary>, AppError> {
    db.with_connection(|conn| {
        Ok(repo::tags::list_for_session(conn, session_id)?
            .into_iter()
            .map(to_summary)
            .collect())
    })
}

/// Gắn một tag cho một Phiên (spec I/O Matrix "Tag thứ 21"): đã gắn sẵn là
/// no-op thành công (kể cả khi Phiên đã ở đúng 20 tag) — chỉ một tag *mới*
/// đẩy Phiên vượt quá 20 mới bị từ chối. Cả hai bước (kiểm đã-gắn, kiểm giới
/// hạn, ghi) chạy trong một transaction để không có cửa sổ đua giữa hai lời
/// gọi `attach_tag` đồng thời trên cùng Phiên (spec Boundaries Always: "kiểm
/// ≤ 20 trong transaction").
pub fn attach_tag(db: &Db, session_id: SessionId, tag_id: TagId) -> Result<(), AppError> {
    db.with_connection(|conn| {
        let tx = conn.transaction()?;
        if repo::tags::is_attached(&tx, session_id, tag_id)? {
            tx.commit()?;
            return Ok(());
        }
        let count = repo::tags::count_for_session(&tx, session_id)?;
        if count >= MAX_TAGS_PER_SESSION {
            return Err(AppError::new(
                Code::Request,
                format!("Phiên đã có tối đa {MAX_TAGS_PER_SESSION} tag"),
            ));
        }
        repo::tags::attach(&tx, session_id, tag_id)?;
        tx.commit()?;
        Ok(())
    })
}

/// Gỡ một tag khỏi một Phiên -- idempotent, liên kết không tồn tại không phải
/// lỗi.
pub fn detach_tag(db: &Db, session_id: SessionId, tag_id: TagId) -> Result<(), AppError> {
    db.with_connection(|conn| {
        repo::tags::detach(conn, session_id, tag_id)?;
        Ok(())
    })
}

/// Xoá hẳn một tag toàn cục (spec I/O Matrix "Xoá tag toàn cục": "tag và 3
/// liên kết biến mất") -- FK `ON DELETE CASCADE` xoá luôn mọi liên kết
/// `session_tags` trong cùng lệnh `DELETE`. Idempotent: `tag_id` không tồn
/// tại không phải lỗi.
pub fn delete_tag(db: &Db, tag_id: TagId) -> Result<(), AppError> {
    db.with_connection(|conn| {
        repo::tags::delete_tag(conn, tag_id)?;
        Ok(())
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
    fn normalize_tag_name_trims_and_collapses_internal_whitespace() {
        assert_eq!(
            normalize_tag_name("  sprint   12 "),
            Some(("sprint 12".to_string(), "sprint 12".to_string()))
        );
    }

    #[test]
    fn normalize_tag_name_rejects_blank_input() {
        assert_eq!(normalize_tag_name(""), None);
        assert_eq!(normalize_tag_name("   "), None);
    }

    #[test]
    fn normalize_tag_name_lowercases_unicode_aware_not_ascii_only() {
        let (name, key) = normalize_tag_name("DỰ ÁN").unwrap();
        assert_eq!(name, "DỰ ÁN");
        assert_eq!(key, "dự án");
    }

    #[test]
    fn create_or_get_rejects_blank_and_too_long_names() {
        let (_dir, db) = open_db();
        let err = create_or_get(&db, "   ").unwrap_err();
        assert_eq!(err.category, crate::core::error::Category::Model);

        let too_long: String = "a".repeat(MAX_NAME_SCALARS + 1);
        let err = create_or_get(&db, &too_long).unwrap_err();
        assert_eq!(err.category, crate::core::error::Category::Model);
    }

    #[test]
    fn create_or_get_is_case_insensitive_idempotent() {
        let (_dir, db) = open_db();
        let first = create_or_get(&db, "DỰ ÁN").unwrap();
        let second = create_or_get(&db, "dự án").unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(second.name, "DỰ ÁN");
        assert_eq!(list_with_counts(&db).unwrap().len(), 1);
    }

    #[test]
    fn create_or_get_from_two_threads_at_once_yields_exactly_one_row() {
        use std::sync::Arc;
        let dir = tempdir().unwrap();
        let db = Arc::new(Db::open(dir.path()).unwrap());

        let db_a = db.clone();
        let db_b = db.clone();
        let a = std::thread::spawn(move || create_or_get(&db_a, "Khách A").unwrap());
        let b = std::thread::spawn(move || create_or_get(&db_b, "khách a").unwrap());
        let a = a.join().unwrap();
        let b = b.join().unwrap();

        assert_eq!(a.id, b.id, "cả hai luồng phải nhận cùng id");
        assert_eq!(list_with_counts(&db).unwrap().len(), 1, "đúng 1 dòng tags");
    }

    #[test]
    fn attach_tag_rejects_the_21st_new_tag_but_allows_reattaching_an_existing_one() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);

        let mut tags = Vec::new();
        for i in 0..MAX_TAGS_PER_SESSION {
            let tag = create_or_get(&db, &format!("tag-{i}")).unwrap();
            attach_tag(&db, session_id, tag.id).unwrap();
            tags.push(tag);
        }

        // Đã ở đúng 20 tag -- gắn lại một tag đã có là no-op thành công.
        attach_tag(&db, session_id, tags[0].id).unwrap();

        // Một tag mới thứ 21 bị từ chối.
        let new_tag = create_or_get(&db, "tag-21").unwrap();
        let err = attach_tag(&db, session_id, new_tag.id).unwrap_err();
        assert_eq!(err.category, crate::core::error::Category::Model);

        let attached = list_for_session(&db, session_id).unwrap();
        assert_eq!(attached.len(), MAX_TAGS_PER_SESSION as usize);
    }

    #[test]
    fn detach_tag_is_idempotent() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        let tag = create_or_get(&db, "a").unwrap();
        attach_tag(&db, session_id, tag.id).unwrap();

        detach_tag(&db, session_id, tag.id).unwrap();
        assert!(list_for_session(&db, session_id).unwrap().is_empty());
        // Gọi lại trên liên kết đã gỡ vẫn `Ok`.
        detach_tag(&db, session_id, tag.id).unwrap();
    }

    #[test]
    fn deleting_a_session_keeps_the_tag_but_deleting_a_tag_removes_its_links() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        let tag = create_or_get(&db, "a").unwrap();
        attach_tag(&db, session_id, tag.id).unwrap();

        db.with_connection(|conn| Ok(sessions::delete(conn, session_id)?))
            .unwrap();
        assert_eq!(
            list_with_counts(&db).unwrap()[0].session_count,
            0,
            "xoá Phiên phải giữ nguyên tag, chỉ mất liên kết"
        );

        let other_session = SessionId::new();
        insert_session(&db, other_session);
        attach_tag(&db, other_session, tag.id).unwrap();
        delete_tag(&db, tag.id).unwrap();
        assert!(list_for_session(&db, other_session).unwrap().is_empty());
        assert!(list_with_counts(&db).unwrap().is_empty());
    }

    #[test]
    fn rerun_and_rename_never_touch_session_tags() {
        // Story 3.2 spec Boundaries Always: "Tag không phụ thuộc tên/
        // transcript: đổi tên, Chạy lại, Transcribe lại không đụng
        // `session_tags`". `rename_session`/`swap_transcript` chỉ chạm
        // `sessions.title`/`transcripts` -- không có câu SQL nào trong
        // `library::tags`/`db::repo::tags` tham chiếu tới chúng, nên không có
        // đường nào để chúng đụng `session_tags`. Bài test này khẳng định gắn
        // tag rồi đổi tên Phiên vẫn giữ nguyên tag.
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        let tag = create_or_get(&db, "a").unwrap();
        attach_tag(&db, session_id, tag.id).unwrap();

        crate::library::store::rename_session(&db, session_id, "tên mới").unwrap();

        assert_eq!(list_for_session(&db, session_id).unwrap(), vec![tag]);
    }
}
