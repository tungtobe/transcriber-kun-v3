//! SQL cho `tags`/`session_tags` — nơi duy nhất khác ngoài
//! `db/migrations/mod.rs` được phép chứa câu SQL cho hai bảng này (spec
//! Boundaries). Chuẩn hoá tên/giới hạn 20 tag mỗi Phiên nằm ở
//! `library::tags`, không ở đây — module này chỉ đọc/ghi thẳng.

use rusqlite::{params, Connection, OptionalExtension};

use crate::core::id::{SessionId, TagId};

#[derive(Debug, Clone, PartialEq)]
pub struct TagRow {
    pub id: TagId,
    pub name: String,
    pub name_key: String,
    pub created_at: i64,
}

/// Một tag kèm số Phiên đang gắn nó — dùng cho `tags_list` (spec Boundaries:
/// "`tags_list` trả tag kèm số phiên, sắp số phiên giảm dần rồi tên").
#[derive(Debug, Clone, PartialEq)]
pub struct TagWithCount {
    pub id: TagId,
    pub name: String,
    pub session_count: i64,
}

fn parse_tag_id(raw: &str) -> rusqlite::Result<TagId> {
    TagId::try_from(raw).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("giá trị `tags.id` không phải UUIDv7 hợp lệ: {raw}").into(),
        )
    })
}

fn row_to_tag(row: &rusqlite::Row<'_>) -> rusqlite::Result<TagRow> {
    let id: String = row.get(0)?;
    Ok(TagRow {
        id: parse_tag_id(&id)?,
        name: row.get(1)?,
        name_key: row.get(2)?,
        created_at: row.get(3)?,
    })
}

/// Tạo tag idempotent theo `name_key` (spec Boundaries Always: "Tạo tag là
/// idempotent theo `name_key` (`INSERT … ON CONFLICT DO NOTHING` rồi đọc lại)
/// — tạo đồng thời không lỗi, không trùng"). `id`/`name`/`created_at` chỉ
/// dùng khi dòng thật sự mới được chèn -- một `name_key` đã tồn tại (do lời
/// gọi này hay một lời gọi khác, kể cả đồng thời) luôn trả về đúng dòng đã có
/// sẵn, không bao giờ tạo dòng thứ hai. `name`/`name_key` đã được caller
/// (`library::tags::normalize_tag_name`) chuẩn hoá và validate -- hàm này chỉ
/// ghi thẳng.
pub fn upsert_by_name_key(
    conn: &Connection,
    id: TagId,
    name: &str,
    name_key: &str,
    created_at: i64,
) -> rusqlite::Result<TagRow> {
    conn.execute(
        "INSERT INTO tags (id, name, name_key, created_at) VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT (name_key) DO NOTHING",
        params![id.to_string(), name, name_key, created_at],
    )?;
    conn.query_row(
        "SELECT id, name, name_key, created_at FROM tags WHERE name_key = ?1",
        params![name_key],
        row_to_tag,
    )
}

/// Mọi tag kèm số Phiên đang gắn, sắp số phiên giảm dần rồi tên tăng dần
/// (spec Boundaries Always).
pub fn list_with_counts(conn: &Connection) -> rusqlite::Result<Vec<TagWithCount>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, COUNT(st.session_id) AS cnt \
         FROM tags t \
         LEFT JOIN session_tags st ON st.tag_id = t.id \
         GROUP BY t.id \
         ORDER BY cnt DESC, t.name ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        Ok(TagWithCount {
            id: parse_tag_id(&id)?,
            name: row.get(1)?,
            session_count: row.get(2)?,
        })
    })?;
    rows.collect()
}

/// Mọi tag đang gắn với một Phiên, tên tăng dần -- dùng cho chi tiết Phiên
/// (`library::store::get_detail`).
pub fn list_for_session(conn: &Connection, session_id: SessionId) -> rusqlite::Result<Vec<TagRow>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, t.name_key, t.created_at \
         FROM tags t \
         JOIN session_tags st ON st.tag_id = t.id \
         WHERE st.session_id = ?1 \
         ORDER BY t.name ASC",
    )?;
    let rows = stmt.query_map(params![session_id.to_string()], row_to_tag)?;
    rows.collect()
}

/// Mọi liên kết `(session_id, tag_id)` -- dùng bởi
/// `repo::sessions::list_for_home` để gom tag theo Phiên trong đúng hai truy
/// vấn tổng cộng, không N+1 (spec Code Map).
pub fn list_all_links(conn: &Connection) -> rusqlite::Result<Vec<(String, TagId)>> {
    let mut stmt = conn.prepare("SELECT session_id, tag_id FROM session_tags")?;
    let rows = stmt.query_map([], |row| {
        let session_id: String = row.get(0)?;
        let tag_id: String = row.get(1)?;
        Ok((session_id, tag_id))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (session_id, tag_id) = row?;
        out.push((session_id, parse_tag_id(&tag_id)?));
    }
    Ok(out)
}

/// Có đang gắn `tag_id` cho `session_id` hay chưa -- dùng bởi
/// `library::tags::attach_tag` để quyết định no-op (spec I/O Matrix "Tag thứ
/// 21": "gắn tag đã có trên Phiên là no-op") trước khi kiểm giới hạn 20.
pub fn is_attached(
    conn: &Connection,
    session_id: SessionId,
    tag_id: TagId,
) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT 1 FROM session_tags WHERE session_id = ?1 AND tag_id = ?2",
        params![session_id.to_string(), tag_id.to_string()],
        |_| Ok(()),
    )
    .optional()
    .map(|found| found.is_some())
}

pub fn exists(conn: &Connection, tag_id: TagId) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT 1 FROM tags WHERE id = ?1",
        params![tag_id.to_string()],
        |_| Ok(()),
    )
    .optional()
    .map(|found| found.is_some())
}

/// Số tag đang gắn với một Phiên -- dùng để kiểm giới hạn 20 (spec Boundaries
/// Always).
pub fn count_for_session(conn: &Connection, session_id: SessionId) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM session_tags WHERE session_id = ?1",
        params![session_id.to_string()],
        |row| row.get(0),
    )
}

/// Gắn một tag cho một Phiên -- `INSERT OR IGNORE` nên gắn lại một liên kết
/// đã có là no-op, không lỗi (spec I/O Matrix "Tag thứ 21": "gắn tag đã có
/// trên Phiên là no-op"). Không tự kiểm giới hạn 20 -- caller
/// (`library::tags::attach_tag`) kiểm trước, trong cùng transaction.
pub fn attach(conn: &Connection, session_id: SessionId, tag_id: TagId) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO session_tags (session_id, tag_id) VALUES (?1, ?2)",
        params![session_id.to_string(), tag_id.to_string()],
    )?;
    Ok(())
}

/// Gỡ một tag khỏi một Phiên. Idempotent: liên kết không tồn tại gỡ `0` dòng,
/// không phải lỗi.
pub fn detach(conn: &Connection, session_id: SessionId, tag_id: TagId) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM session_tags WHERE session_id = ?1 AND tag_id = ?2",
        params![session_id.to_string(), tag_id.to_string()],
    )
}

/// Xoá hẳn một tag toàn cục -- FK `ON DELETE CASCADE` (migration) xoá luôn
/// mọi `session_tags` tham chiếu nó trong cùng lệnh `DELETE` này (spec I/O
/// Matrix "Xoá tag toàn cục": "tag và 3 liên kết biến mất"). Idempotent:
/// `tag_id` không tồn tại xoá `0` dòng, không phải lỗi.
pub fn delete_tag(conn: &Connection, tag_id: TagId) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM tags WHERE id = ?1",
        params![tag_id.to_string()],
    )
}

/// Xoá mọi tag toàn cục (story 3.4, spec Approach "Xoá toàn bộ dữ liệu"). FK
/// `ON DELETE CASCADE` xoá luôn mọi `session_tags` còn sót trong cùng lệnh
/// `DELETE` này -- gọi trong cùng transaction với
/// [`super::sessions::delete_all`] ở `library::store::wipe_all`, sau khi
/// `sessions` đã xoá (thứ tự nào cũng đúng vì cả hai FK đều `CASCADE`, nhưng
/// giữ đúng thứ tự spec liệt kê). Idempotent: kho rỗng xoá `0` dòng.
pub fn delete_all(conn: &Connection) -> rusqlite::Result<usize> {
    conn.execute("DELETE FROM tags", [])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::id::SessionId;
    use crate::db::migrations;
    use crate::db::repo::sessions::{self, NewSession};

    fn open_migrated_fk() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        migrations::run(&mut conn).unwrap();
        conn
    }

    fn insert_session(conn: &Connection, id: SessionId) {
        sessions::insert(
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
        )
        .unwrap();
    }

    #[test]
    fn upsert_by_name_key_creates_once_and_returns_the_same_row_on_conflict() {
        let conn = open_migrated_fk();
        let first_id = TagId::new();
        let first = upsert_by_name_key(&conn, first_id, "DỰ ÁN", "dự án", 0).unwrap();
        assert_eq!(first.id, first_id);
        assert_eq!(first.name, "DỰ ÁN");

        let second_id = TagId::new();
        let second = upsert_by_name_key(&conn, second_id, "dự án", "dự án", 0).unwrap();
        assert_eq!(
            second.id, first_id,
            "phải trả về dòng đã tồn tại, không tạo mới"
        );
        assert_eq!(second.name, "DỰ ÁN", "giữ nguyên tên đã lưu lần đầu");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tags", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn list_with_counts_orders_by_session_count_desc_then_name_asc() {
        let conn = open_migrated_fk();
        let s1 = SessionId::new();
        let s2 = SessionId::new();
        insert_session(&conn, s1);
        insert_session(&conn, s2);

        let popular = upsert_by_name_key(&conn, TagId::new(), "zzz", "zzz", 0).unwrap();
        let rare_a = upsert_by_name_key(&conn, TagId::new(), "aaa", "aaa", 0).unwrap();
        let rare_b = upsert_by_name_key(&conn, TagId::new(), "bbb", "bbb", 0).unwrap();
        attach(&conn, s1, popular.id).unwrap();
        attach(&conn, s2, popular.id).unwrap();
        attach(&conn, s1, rare_a.id).unwrap();

        let rows = list_with_counts(&conn).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![popular.id, rare_a.id, rare_b.id]
        );
        assert_eq!(rows[0].session_count, 2);
        assert_eq!(rows[1].session_count, 1);
        assert_eq!(rows[2].session_count, 0);
    }

    #[test]
    fn attach_is_idempotent_and_detach_on_missing_link_is_a_noop() {
        let conn = open_migrated_fk();
        let session_id = SessionId::new();
        insert_session(&conn, session_id);
        let tag = upsert_by_name_key(&conn, TagId::new(), "a", "a", 0).unwrap();

        attach(&conn, session_id, tag.id).unwrap();
        attach(&conn, session_id, tag.id).unwrap();
        assert_eq!(count_for_session(&conn, session_id).unwrap(), 1);
        assert!(is_attached(&conn, session_id, tag.id).unwrap());

        let affected = detach(&conn, session_id, tag.id).unwrap();
        assert_eq!(affected, 1);
        assert_eq!(detach(&conn, session_id, tag.id).unwrap(), 0);
        assert!(!is_attached(&conn, session_id, tag.id).unwrap());
    }

    #[test]
    fn list_for_session_returns_only_that_sessions_tags_sorted_by_name() {
        let conn = open_migrated_fk();
        let session_id = SessionId::new();
        let other_session = SessionId::new();
        insert_session(&conn, session_id);
        insert_session(&conn, other_session);
        let b = upsert_by_name_key(&conn, TagId::new(), "b", "b", 0).unwrap();
        let a = upsert_by_name_key(&conn, TagId::new(), "a", "a", 0).unwrap();
        attach(&conn, session_id, b.id).unwrap();
        attach(&conn, session_id, a.id).unwrap();
        attach(&conn, other_session, b.id).unwrap();

        let tags = list_for_session(&conn, session_id).unwrap();
        assert_eq!(
            tags.iter().map(|t| t.id).collect::<Vec<_>>(),
            vec![a.id, b.id]
        );
    }

    #[test]
    fn delete_tag_is_idempotent_and_cascades_links() {
        let conn = open_migrated_fk();
        let session_id = SessionId::new();
        insert_session(&conn, session_id);
        let tag = upsert_by_name_key(&conn, TagId::new(), "a", "a", 0).unwrap();
        attach(&conn, session_id, tag.id).unwrap();

        assert_eq!(delete_tag(&conn, tag.id).unwrap(), 1);
        assert_eq!(count_for_session(&conn, session_id).unwrap(), 0);
        assert_eq!(delete_tag(&conn, tag.id).unwrap(), 0);
    }

    #[test]
    fn list_all_links_returns_every_session_tag_pair() {
        let conn = open_migrated_fk();
        let s1 = SessionId::new();
        let s2 = SessionId::new();
        insert_session(&conn, s1);
        insert_session(&conn, s2);
        let tag = upsert_by_name_key(&conn, TagId::new(), "a", "a", 0).unwrap();
        attach(&conn, s1, tag.id).unwrap();
        attach(&conn, s2, tag.id).unwrap();

        let mut links: Vec<(String, String)> = list_all_links(&conn)
            .unwrap()
            .into_iter()
            .map(|(session_id, tag_id)| (session_id, tag_id.to_string()))
            .collect();
        links.sort();
        let mut expected = vec![
            (s1.to_string(), tag.id.to_string()),
            (s2.to_string(), tag.id.to_string()),
        ];
        expected.sort();
        assert_eq!(links, expected);
    }
}
