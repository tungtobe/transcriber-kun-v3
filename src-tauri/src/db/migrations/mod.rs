//! Migration chỉ tiến — hằng chuỗi SQL trong Rust (spec Never: không tạo file
//! `*.sql`). Đây là nơi duy nhất trong toàn repo được phép chứa `CREATE
//! TABLE` (Acceptance Criteria kiểm bằng grep).

use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

/// Bảng `settings` duy nhất của story này (spec Never: không bảng nào khác,
/// không bảng `jobs`). Value lưu dạng JSON text để `settings::Settings`
/// (de)serialize không cần một cột riêng cho từng khoá.
static MIGRATIONS: [M; 1] = [M::up(
    "CREATE TABLE settings (\n\
         key TEXT PRIMARY KEY,\n\
         value TEXT NOT NULL\n\
     );",
)];

fn runner() -> Migrations<'static> {
    Migrations::from_slice(&MIGRATIONS)
}

/// Chạy mọi migration còn thiếu trên `conn` tới bản mới nhất. Idempotent:
/// gọi lại trên DB đã ở bản mới nhất không làm gì (`rusqlite_migration` tự
/// theo dõi qua `PRAGMA user_version`).
pub fn run(conn: &mut Connection) -> rusqlite_migration::Result<()> {
    runner().to_latest(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_validate() {
        runner().validate().expect("migration phải hợp lệ");
    }

    #[test]
    fn running_twice_is_a_noop() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        let version_after_first: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        run(&mut conn).unwrap();
        let version_after_second: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version_after_first, 1);
        assert_eq!(version_after_second, 1);
    }

    #[test]
    fn creates_settings_table() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='settings'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
