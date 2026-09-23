//! Migration chỉ tiến — hằng chuỗi SQL trong Rust (spec Never: không tạo file
//! `*.sql`). Đây là nơi duy nhất trong toàn repo được phép chứa `CREATE
//! TABLE` (Acceptance Criteria kiểm bằng grep).

use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

/// Bảng `settings` (story 1.2), `local_counters` (story 1.10) và
/// `sessions`/`transcripts`/`segments` (story 2.3, migration tiến — không sửa
/// hai migration trước). `local_counters` giữ bộ đếm cục bộ (số phiên, lỗi
/// theo category, marker `cleanShutdown` cho crash) — chỉ số nguyên, không
/// nội dung, không bao giờ gửi đi đâu (spec Boundaries "Riêng tư của bộ
/// đếm"). Value của `settings` lưu dạng JSON text để `settings::Settings`
/// (de)serialize không cần một cột riêng cho từng khoá.
///
/// `sessions`/`transcripts`/`segments` (spec 2.3 Tasks): `sessions.status`
/// gồm cả `recording`/`finalizing` cho Phiên live tương lai (Epic 4) dù story
/// này chỉ ghi `complete` (spec Never: "Không chuyển Phiên live
/// recording|finalizing"). `transcripts` có một chỉ mục unique riêng phần
/// (`WHERE variant = 'primary'`) để chỉ một bản `primary`/Phiên tồn tại cùng
/// lúc — nhiều bản `retranscribe` không bị chặn bởi ràng buộc đó. `segments`
/// gộp cả đoạn text lẫn khoảng thiếu (gap) vào một bảng, phân biệt bằng
/// `kind`; CHECK biconditional `(kind = 'gap') = (gap_reason IS NOT NULL)`
/// đảm bảo `gap_reason` có mặt đúng lúc `kind = 'gap'`, không lúc nào khác.
static MIGRATIONS: [M; 3] = [
    M::up(
        "CREATE TABLE settings (\n\
             key TEXT PRIMARY KEY,\n\
             value TEXT NOT NULL\n\
         );",
    ),
    M::up(
        "CREATE TABLE local_counters (\n\
             key TEXT PRIMARY KEY,\n\
             value INTEGER NOT NULL\n\
         );",
    ),
    M::up(
        "CREATE TABLE sessions (\n\
             id TEXT PRIMARY KEY,\n\
             kind TEXT NOT NULL CHECK (kind IN ('file', 'live')),\n\
             title TEXT NOT NULL,\n\
             source_hash TEXT UNIQUE,\n\
             source_name TEXT,\n\
             status TEXT NOT NULL CHECK (status IN ('recording', 'finalizing', 'complete')),\n\
             recovered INTEGER NOT NULL DEFAULT 0 CHECK (recovered IN (0, 1)),\n\
             duration_sec REAL NOT NULL,\n\
             proxy_ext TEXT,\n\
             created_at INTEGER NOT NULL,\n\
             updated_at INTEGER NOT NULL\n\
         );\n\
         CREATE TABLE transcripts (\n\
             id TEXT PRIMARY KEY,\n\
             session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,\n\
             variant TEXT NOT NULL CHECK (variant IN ('primary', 'retranscribe')),\n\
             status TEXT NOT NULL CHECK (status IN ('complete', 'partial')),\n\
             model TEXT NOT NULL,\n\
             language TEXT,\n\
             created_at INTEGER NOT NULL\n\
         );\n\
         CREATE UNIQUE INDEX transcripts_one_primary_per_session\n\
             ON transcripts (session_id)\n\
             WHERE variant = 'primary';\n\
         CREATE TABLE segments (\n\
             transcript_id TEXT NOT NULL REFERENCES transcripts(id) ON DELETE CASCADE,\n\
             idx INTEGER NOT NULL,\n\
             start_sec REAL NOT NULL,\n\
             end_sec REAL NOT NULL,\n\
             kind TEXT NOT NULL CHECK (kind IN ('text', 'gap')),\n\
             gap_reason TEXT CHECK (gap_reason IN ('chunk_failed', 'disconnected')),\n\
             text TEXT NOT NULL DEFAULT '',\n\
             speaker TEXT,\n\
             PRIMARY KEY (transcript_id, idx),\n\
             CHECK ((kind = 'gap') = (gap_reason IS NOT NULL)),\n\
             CHECK (end_sec >= start_sec)\n\
         );",
    ),
];

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
        assert_eq!(version_after_first, 3);
        assert_eq!(version_after_second, 3);
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

    fn table_exists(conn: &Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [name],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
            == 1
    }

    #[test]
    fn creates_sessions_transcripts_and_segments_tables() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        assert!(table_exists(&conn, "sessions"));
        assert!(table_exists(&conn, "transcripts"));
        assert!(table_exists(&conn, "segments"));
    }

    #[test]
    fn sessions_kind_check_rejects_values_outside_file_or_live() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        let err = conn
            .execute(
                "INSERT INTO sessions (id, kind, title, status, duration_sec, created_at, updated_at) \
                 VALUES ('s1', 'bogus', 't', 'complete', 1.0, 0, 0)",
                [],
            )
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("check"));
    }

    #[test]
    fn only_one_primary_transcript_allowed_per_session() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        run(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, kind, title, status, duration_sec, created_at, updated_at) \
             VALUES ('s1', 'file', 't', 'complete', 1.0, 0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transcripts (id, session_id, variant, status, model, created_at) \
             VALUES ('t1', 's1', 'primary', 'complete', 'm', 0)",
            [],
        )
        .unwrap();
        // Một bản `retranscribe` thứ hai không bị chặn -- chỉ `primary` mới
        // giới hạn một bản/Phiên.
        conn.execute(
            "INSERT INTO transcripts (id, session_id, variant, status, model, created_at) \
             VALUES ('t2', 's1', 'retranscribe', 'complete', 'm', 0)",
            [],
        )
        .unwrap();
        let err = conn
            .execute(
                "INSERT INTO transcripts (id, session_id, variant, status, model, created_at) \
                 VALUES ('t3', 's1', 'primary', 'complete', 'm', 0)",
                [],
            )
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("unique"));
    }

    #[test]
    fn segments_check_requires_gap_reason_exactly_when_kind_is_gap() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        run(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, kind, title, status, duration_sec, created_at, updated_at) \
             VALUES ('s1', 'file', 't', 'complete', 1.0, 0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transcripts (id, session_id, variant, status, model, created_at) \
             VALUES ('t1', 's1', 'primary', 'complete', 'm', 0)",
            [],
        )
        .unwrap();

        // `kind = 'text'` với `gap_reason` khác NULL -> vi phạm CHECK.
        let err = conn
            .execute(
                "INSERT INTO segments (transcript_id, idx, start_sec, end_sec, kind, gap_reason, text) \
                 VALUES ('t1', 0, 0.0, 1.0, 'text', 'chunk_failed', 'x')",
                [],
            )
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("check"));

        // `kind = 'gap'` thiếu `gap_reason` -> vi phạm CHECK.
        let err = conn
            .execute(
                "INSERT INTO segments (transcript_id, idx, start_sec, end_sec, kind, text) \
                 VALUES ('t1', 0, 0.0, 1.0, 'gap', '')",
                [],
            )
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("check"));

        // `end_sec < start_sec` -> vi phạm CHECK.
        let err = conn
            .execute(
                "INSERT INTO segments (transcript_id, idx, start_sec, end_sec, kind, text) \
                 VALUES ('t1', 0, 5.0, 1.0, 'text', 'x')",
                [],
            )
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("check"));

        // Hợp lệ cả hai dạng.
        conn.execute(
            "INSERT INTO segments (transcript_id, idx, start_sec, end_sec, kind, text) \
             VALUES ('t1', 0, 0.0, 1.0, 'text', 'hello')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO segments (transcript_id, idx, start_sec, end_sec, kind, gap_reason, text) \
             VALUES ('t1', 1, 1.0, 2.0, 'gap', 'disconnected', '')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn creates_local_counters_table() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='local_counters'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
