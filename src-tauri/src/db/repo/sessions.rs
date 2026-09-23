//! SQL cho bảng `sessions` — nơi duy nhất khác ngoài `db/migrations/mod.rs`
//! được phép chứa câu SQL cho bảng này (spec Boundaries).

use rusqlite::{params, Connection, OptionalExtension};

use crate::core::id::SessionId;

/// Tham số chèn một Phiên mới. `kind`/`status` là chuỗi thô khớp CHECK ở
/// migration (`"file"`/`"live"`, `"recording"`/`"finalizing"`/`"complete"`) —
/// story này chỉ bao giờ truyền `kind = "file"`, `status = "complete"`
/// (`library::store::commit_file_session`); Phiên live (Epic 4) sẽ truyền
/// biến thể còn lại qua cùng hàm này sau.
#[derive(Debug, Clone, Copy)]
pub struct NewSession<'a> {
    pub id: SessionId,
    pub kind: &'a str,
    pub title: &'a str,
    pub source_hash: Option<&'a str>,
    pub source_name: Option<&'a str>,
    pub status: &'a str,
    pub recovered: bool,
    pub duration_sec: f64,
    pub proxy_ext: Option<&'a str>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionRow {
    pub id: SessionId,
    pub kind: String,
    pub title: String,
    pub source_hash: Option<String>,
    pub source_name: Option<String>,
    pub status: String,
    pub recovered: bool,
    pub duration_sec: f64,
    pub proxy_ext: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

const SELECT_COLUMNS: &str = "id, kind, title, source_hash, source_name, status, recovered, \
     duration_sec, proxy_ext, created_at, updated_at";

fn parse_session_id(raw: &str) -> rusqlite::Result<SessionId> {
    SessionId::try_from(raw).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("giá trị `sessions.id` không phải UUIDv7 hợp lệ: {raw}").into(),
        )
    })
}

fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    let id: String = row.get(0)?;
    Ok(SessionRow {
        id: parse_session_id(&id)?,
        kind: row.get(1)?,
        title: row.get(2)?,
        source_hash: row.get(3)?,
        source_name: row.get(4)?,
        status: row.get(5)?,
        recovered: row.get::<_, i64>(6)? != 0,
        duration_sec: row.get(7)?,
        proxy_ext: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

/// Chèn một Phiên mới. Vi phạm CHECK/UNIQUE (`source_hash` trùng — spec I/O
/// Matrix "Trùng hash") trả thẳng `rusqlite::Error`, không nuốt lỗi — caller
/// (`library::store`) chịu trách nhiệm rollback transaction và gỡ Proxy vừa
/// publish.
pub fn insert(conn: &Connection, new: NewSession<'_>) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO sessions (id, kind, title, source_hash, source_name, status, recovered, \
             duration_sec, proxy_ext, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            new.id.to_string(),
            new.kind,
            new.title,
            new.source_hash,
            new.source_name,
            new.status,
            new.recovered as i64,
            new.duration_sec,
            new.proxy_ext,
            new.created_at,
            new.updated_at,
        ],
    )?;
    Ok(())
}

/// Đọc một Phiên theo id, `None` nếu không tồn tại.
pub fn get(conn: &Connection, id: SessionId) -> rusqlite::Result<Option<SessionRow>> {
    conn.query_row(
        &format!("SELECT {SELECT_COLUMNS} FROM sessions WHERE id = ?1"),
        params![id.to_string()],
        row_to_session,
    )
    .optional()
}

/// Đọc toàn bộ Phiên, mới nhất trước.
pub fn list(conn: &Connection) -> rusqlite::Result<Vec<SessionRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLUMNS} FROM sessions ORDER BY created_at DESC"
    ))?;
    let rows = stmt.query_map([], row_to_session)?;
    rows.collect()
}

/// Cập nhật `proxy_ext` của một Phiên (`None` = `proxy_missing`, spec Design
/// Notes). Dùng bởi `library::store::commit_file_session` (Proxy lỗi lúc
/// commit) và `library::store::reconcile` (Proxy đã publish trước đó nhưng
/// file trên đĩa không còn — spec I/O Matrix "Crash giữa bước").
pub fn set_proxy_ext(
    conn: &Connection,
    id: SessionId,
    proxy_ext: Option<&str>,
    updated_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sessions SET proxy_ext = ?1, updated_at = ?2 WHERE id = ?3",
        params![proxy_ext, updated_at, id.to_string()],
    )?;
    Ok(())
}

/// Tra một Phiên theo `source_hash` chính xác (spec I/O Matrix "Trùng hash":
/// `transcribe_start` gọi trước khi tạo Job — trùng thì trả `Existing
/// { session_id }`, không tạo Job, không gọi Gemini). `source_hash` là
/// `UNIQUE` khi khác `NULL` nên tối đa một dòng khớp.
pub fn find_by_source_hash(
    conn: &Connection,
    source_hash: &str,
) -> rusqlite::Result<Option<SessionId>> {
    conn.query_row(
        "SELECT id FROM sessions WHERE source_hash = ?1",
        params![source_hash],
        |row| row.get::<_, String>(0),
    )
    .optional()?
    .map(|id| parse_session_id(&id))
    .transpose()
}

/// Đọc `(id, proxy_ext)` của mọi Phiên — dùng bởi `library::store::reconcile`
/// để biết thư mục `media/<id>` nào có dòng DB tham chiếu và Proxy nào đang
/// được tham chiếu, không cần toàn bộ cột khác của [`SessionRow`].
pub fn list_media_refs(conn: &Connection) -> rusqlite::Result<Vec<(SessionId, Option<String>)>> {
    let mut stmt = conn.prepare("SELECT id, proxy_ext FROM sessions")?;
    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let proxy_ext: Option<String> = row.get(1)?;
        Ok((id, proxy_ext))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, proxy_ext) = row?;
        out.push((parse_session_id(&id)?, proxy_ext));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn open_migrated() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        migrations::run(&mut conn).unwrap();
        conn
    }

    fn sample(id: SessionId) -> NewSession<'static> {
        NewSession {
            id,
            kind: "file",
            title: "cuộc họp",
            source_hash: Some("hash-1"),
            source_name: Some("meeting.mp4"),
            status: "complete",
            recovered: false,
            duration_sec: 12.5,
            proxy_ext: Some("flac"),
            created_at: 1_000,
            updated_at: 1_000,
        }
    }

    #[test]
    fn insert_then_get_round_trips_all_columns() {
        let conn = open_migrated();
        let id = SessionId::new();
        insert(&conn, sample(id)).unwrap();

        let row = get(&conn, id).unwrap().expect("phải đọc lại được");
        assert_eq!(row.id, id);
        assert_eq!(row.kind, "file");
        assert_eq!(row.title, "cuộc họp");
        assert_eq!(row.source_hash.as_deref(), Some("hash-1"));
        assert_eq!(row.source_name.as_deref(), Some("meeting.mp4"));
        assert_eq!(row.status, "complete");
        assert!(!row.recovered);
        assert!((row.duration_sec - 12.5).abs() < f64::EPSILON);
        assert_eq!(row.proxy_ext.as_deref(), Some("flac"));
        assert_eq!(row.created_at, 1_000);
        assert_eq!(row.updated_at, 1_000);
    }

    #[test]
    fn get_on_missing_id_returns_none() {
        let conn = open_migrated();
        assert!(get(&conn, SessionId::new()).unwrap().is_none());
    }

    #[test]
    fn list_returns_every_session() {
        let conn = open_migrated();
        let a = SessionId::new();
        let b = SessionId::new();
        insert(&conn, sample(a)).unwrap();
        insert(
            &conn,
            NewSession {
                source_hash: None,
                ..sample(b)
            },
        )
        .unwrap();
        let rows = list(&conn).unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn duplicate_source_hash_is_rejected() {
        let conn = open_migrated();
        insert(&conn, sample(SessionId::new())).unwrap();
        let err = insert(&conn, sample(SessionId::new())).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("unique"));
    }

    #[test]
    fn null_source_hash_does_not_collide() {
        let conn = open_migrated();
        insert(
            &conn,
            NewSession {
                source_hash: None,
                ..sample(SessionId::new())
            },
        )
        .unwrap();
        insert(
            &conn,
            NewSession {
                source_hash: None,
                ..sample(SessionId::new())
            },
        )
        .unwrap();
        assert_eq!(list(&conn).unwrap().len(), 2);
    }

    #[test]
    fn set_proxy_ext_updates_proxy_ext_and_updated_at() {
        let conn = open_migrated();
        let id = SessionId::new();
        insert(&conn, sample(id)).unwrap();

        set_proxy_ext(&conn, id, None, 2_000).unwrap();

        let row = get(&conn, id).unwrap().unwrap();
        assert_eq!(row.proxy_ext, None);
        assert_eq!(row.updated_at, 2_000);
    }

    #[test]
    fn find_by_source_hash_finds_the_exact_match_and_none_when_absent() {
        let conn = open_migrated();
        let a = SessionId::new();
        insert(
            &conn,
            NewSession {
                source_hash: Some("hash-a"),
                ..sample(a)
            },
        )
        .unwrap();

        assert_eq!(find_by_source_hash(&conn, "hash-a").unwrap(), Some(a));
        assert_eq!(find_by_source_hash(&conn, "hash-missing").unwrap(), None);
    }

    #[test]
    fn list_media_refs_returns_id_and_proxy_ext_pairs() {
        let conn = open_migrated();
        let a = SessionId::new();
        let b = SessionId::new();
        insert(&conn, sample(a)).unwrap();
        insert(
            &conn,
            NewSession {
                source_hash: None,
                proxy_ext: None,
                ..sample(b)
            },
        )
        .unwrap();

        let mut refs = list_media_refs(&conn).unwrap();
        refs.sort_by_key(|(id, _)| id.to_string());
        let mut expected = vec![(a, Some("flac".to_string())), (b, None)];
        expected.sort_by_key(|(id, _)| id.to_string());
        assert_eq!(refs, expected);
    }
}
