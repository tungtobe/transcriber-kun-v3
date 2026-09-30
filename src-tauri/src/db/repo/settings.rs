//! SQL cho bảng `settings` — nơi duy nhất khác ngoài `db/migrations/mod.rs`
//! được phép chứa câu SQL (spec Boundaries).

use std::collections::HashMap;

use rusqlite::{params, Connection};

/// Đọc toàn bộ hàng của bảng `settings` thành map `key -> value` (value là
/// chuỗi JSON thô, tầng `settings::` chịu trách nhiệm parse). Bảng rỗng trả
/// map rỗng, không phải lỗi.
pub fn read_all(conn: &Connection) -> rusqlite::Result<HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut map = HashMap::new();
    for row in rows {
        let (key, value) = row?;
        map.insert(key, value);
    }
    Ok(map)
}

/// Ghi nhiều khoá trong một transaction (upsert). Toàn bộ hoặc không gì cả:
/// nếu một khoá ghi lỗi, transaction rollback, không có khoá nào bị ghi lỡ
/// dở (spec: "Lưu settings ... Lỗi ghi → storage, không phát event").
pub fn upsert_many(conn: &mut Connection, entries: &[(&str, String)]) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    upsert_many_in_transaction(&tx, entries)?;
    tx.commit()
}

/// Upsert selected settings inside a caller-owned transaction, allowing the
/// recommendation service to commit settings and memo templates atomically.
pub fn upsert_many_in_transaction(
    conn: &Connection,
    entries: &[(&str, String)],
) -> rusqlite::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let mut stmt = conn.prepare(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )?;
    for (key, value) in entries {
        stmt.execute(params![key, value])?;
    }
    Ok(())
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

    #[test]
    fn read_all_on_empty_table_returns_empty_map() {
        let conn = open_migrated();
        let map = read_all(&conn).unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn upsert_then_read_round_trips() {
        let mut conn = open_migrated();
        upsert_many(&mut conn, &[("theme", "\"dark\"".to_string())]).unwrap();
        let map = read_all(&conn).unwrap();
        assert_eq!(map.get("theme").map(String::as_str), Some("\"dark\""));
    }

    #[test]
    fn upsert_overwrites_existing_key() {
        let mut conn = open_migrated();
        upsert_many(&mut conn, &[("theme", "\"dark\"".to_string())]).unwrap();
        upsert_many(&mut conn, &[("theme", "\"light\"".to_string())]).unwrap();
        let map = read_all(&conn).unwrap();
        assert_eq!(map.get("theme").map(String::as_str), Some("\"light\""));
        assert_eq!(map.len(), 1);
    }
}
