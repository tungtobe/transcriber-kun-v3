//! SQL cho bảng `local_counters` — nơi duy nhất khác ngoài
//! `db/migrations/mod.rs` được phép chứa câu SQL cho bảng này (spec
//! Boundaries: "SQL chỉ ở `db/repo/counters.rs`"). Không biết gì về ý nghĩa
//! của từng `key` (số phiên, lỗi theo category, marker crash...) — tầng
//! `diagnostics::` sở hữu các hằng tên khoá và logic nghiệp vụ, giống cách
//! `settings::` sở hữu khoá của bảng `settings` trong khi `repo::settings`
//! chỉ đọc/ghi thô.

use rusqlite::{params, Connection, OptionalExtension};

/// Đọc giá trị hiện tại của `key`, mặc định `0` nếu khoá chưa từng ghi (mọi
/// bộ đếm bắt đầu từ 0 — spec: "hiển thị 0" khi chưa ai tăng).
pub fn get(conn: &Connection, key: &str) -> rusqlite::Result<i64> {
    Ok(get_optional(conn, key)?.unwrap_or(0))
}

/// Đọc giá trị hiện tại của `key`, `None` nếu chưa từng ghi. Khác với [`get`]
/// ở chỗ phân biệt được "chưa từng ghi" với "đã ghi giá trị 0" — cần cho
/// marker `cleanShutdown`, nơi "chưa từng ghi" (lần chạy đầu tiên của app)
/// không được tính là "lần trước thoát không sạch".
pub fn get_optional(conn: &Connection, key: &str) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT value FROM local_counters WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
}

/// Tăng `key` thêm 1 (upsert: tạo mới với giá trị 1 nếu chưa có) rồi trả về
/// giá trị mới.
pub fn increment(conn: &Connection, key: &str) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO local_counters (key, value) VALUES (?1, 1) \
         ON CONFLICT(key) DO UPDATE SET value = value + 1",
        params![key],
    )?;
    get(conn, key)
}

/// Ghi đè `key` thành đúng `value` (upsert) — dùng cho marker boolean
/// (`cleanShutdown`, lưu 0/1) chứ không phải bộ đếm tăng dần.
pub fn set(conn: &Connection, key: &str, value: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO local_counters (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
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
    fn get_on_missing_key_returns_zero() {
        let conn = open_migrated();
        assert_eq!(get(&conn, "sessions").unwrap(), 0);
    }

    #[test]
    fn get_optional_on_missing_key_returns_none() {
        let conn = open_migrated();
        assert_eq!(get_optional(&conn, "cleanShutdown").unwrap(), None);
    }

    #[test]
    fn increment_creates_then_increments() {
        let conn = open_migrated();
        assert_eq!(increment(&conn, "sessions").unwrap(), 1);
        assert_eq!(increment(&conn, "sessions").unwrap(), 2);
        assert_eq!(increment(&conn, "sessions").unwrap(), 3);
        assert_eq!(get(&conn, "sessions").unwrap(), 3);
    }

    #[test]
    fn increment_is_independent_per_key() {
        let conn = open_migrated();
        increment(&conn, "errors.auth").unwrap();
        increment(&conn, "errors.auth").unwrap();
        increment(&conn, "errors.network").unwrap();
        assert_eq!(get(&conn, "errors.auth").unwrap(), 2);
        assert_eq!(get(&conn, "errors.network").unwrap(), 1);
        assert_eq!(get(&conn, "errors.storage").unwrap(), 0);
    }

    #[test]
    fn set_writes_then_overwrites() {
        let conn = open_migrated();
        set(&conn, "cleanShutdown", 1).unwrap();
        assert_eq!(get_optional(&conn, "cleanShutdown").unwrap(), Some(1));
        set(&conn, "cleanShutdown", 0).unwrap();
        assert_eq!(get_optional(&conn, "cleanShutdown").unwrap(), Some(0));
    }
}
