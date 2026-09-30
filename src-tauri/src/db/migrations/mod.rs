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
/// `tags`/`session_tags` (story 3.2, spec Boundaries Always): `name_key` là
/// khoá chuẩn hoá UNIQUE (trim + gộp khoảng trắng rồi lowercase Unicode ở
/// `library::tags::normalize_tag_name` — không dựa vào `COLLATE NOCASE`, chỉ
/// ASCII-aware) dùng để tạo tag idempotent (`INSERT ... ON CONFLICT
/// (name_key) DO NOTHING` rồi đọc lại). `session_tags` là bảng nối thuần với
/// khoá chính ghép `(session_id, tag_id)` — không có cột riêng, không cần
/// UNIQUE thêm vì khoá chính đã đúng vai trò đó. Cả hai FK đều `ON DELETE
/// CASCADE`: xoá một Phiên gỡ mọi liên kết của nó (tag vẫn còn), xoá một tag
/// gỡ liên kết của nó khỏi mọi Phiên (Phiên vẫn còn) — đúng hai chiều "Xoá
/// Phiên"/"Xoá tag toàn cục" ở spec I/O Matrix.
/// `notes` (story 3.5, spec Boundaries Always): một dòng tối đa mỗi Phiên
/// (`session_id` vừa là PK vừa là FK `ON DELETE CASCADE` — xoá Phiên/xoá toàn
/// bộ dọn ghi chú theo, Chạy lại/Transcribe lại giữ nguyên vì chúng không
/// đụng `sessions.id`). `revision` do frontend cấp, tăng dần — `library::
/// notes::save` chỉ ghi khi `revision` mới lớn hơn revision đang lưu (upsert
/// có điều kiện ở `db::repo::notes::save_if_newer`), nên ACK/lệnh về sai thứ
/// tự không thể ghi đè bản mới hơn.
/// `memo_templates` (story 3.6, spec Boundaries Always): `id` UUIDv7 do app
/// sinh; `is_default` phân biệt 2 mẫu mặc định mỗi locale (seed lười ở
/// `memo::templates::list`, không seed ở đây — spec Never) với mẫu người
/// dùng (`locale`/`default_key` đều `NULL`). `UNIQUE(locale, default_key)`
/// là điều kiện để seed idempotent qua `INSERT ... ON CONFLICT DO NOTHING`
/// (`ensure_default`) và khôi phục qua `... DO UPDATE` (`restore_default`) --
/// SQLite coi nhiều dòng `(NULL, NULL)` là không trùng nhau nên ràng buộc
/// này không giới hạn số mẫu người dùng.
///
/// `memos` (story 3.7, spec Boundaries Always): cache theo cặp `(session_id,
/// template_id)` -- `PRIMARY KEY(session_id, template_id)` nên
/// `memo::generate` chỉ cần một `INSERT ... ON CONFLICT(session_id,
/// template_id) DO UPDATE` để "sinh lại" thay đúng dòng cũ. `session_id` là
/// FK `ON DELETE CASCADE` (xoá Phiên/xoá toàn bộ dọn theo memo của nó, cùng
/// khuôn `notes`). `template_id` **không** FK (cột `TEXT` trần, spec: "memo
/// sống sót khi template bị xoá") -- `template_name`/`template_prompt` chụp
/// lại nội dung mẫu lúc sinh nên một memo vẫn đọc được đầy đủ dù
/// `memo_templates` đã xoá dòng đó. `transcript_id` cũng cố ý không FK cùng
/// lý do: Chạy lại (`repo::transcripts::replace_primary`) xoá hẳn transcript
/// `primary` cũ trong cùng transaction, và `memo::generate` cần so khớp
/// `transcript_id` đã lưu với transcript `primary` hiện tại để suy
/// `fromPreviousTranscript` -- một FK ở đây sẽ chặn chính transaction xoá
/// đó. `notes_revision` để `NULL` khi Phiên chưa từng có ghi chú lúc sinh.
static MIGRATIONS: [M; 11] = [
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
    M::up(
        "CREATE TABLE tags (\n\
             id TEXT PRIMARY KEY,\n\
             name TEXT NOT NULL,\n\
             name_key TEXT NOT NULL UNIQUE,\n\
             created_at INTEGER NOT NULL\n\
         );\n\
         CREATE TABLE session_tags (\n\
             session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,\n\
             tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,\n\
             PRIMARY KEY (session_id, tag_id)\n\
         );",
    ),
    M::up(
        "CREATE TABLE notes (\n\
             session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,\n\
             body TEXT NOT NULL,\n\
             revision INTEGER NOT NULL,\n\
             updated_at INTEGER NOT NULL\n\
         );",
    ),
    M::up(
        "CREATE TABLE memo_templates (\n\
             id TEXT PRIMARY KEY,\n\
             name TEXT NOT NULL,\n\
             prompt TEXT NOT NULL,\n\
             is_default INTEGER NOT NULL CHECK (is_default IN (0, 1)),\n\
             locale TEXT,\n\
             default_key TEXT,\n\
             created_at INTEGER NOT NULL,\n\
             updated_at INTEGER NOT NULL,\n\
             UNIQUE (locale, default_key)\n\
         );",
    ),
    M::up(
        "CREATE TABLE memos (\n\
             session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,\n\
             template_id TEXT NOT NULL,\n\
             body TEXT NOT NULL,\n\
             created_at INTEGER NOT NULL,\n\
             transcript_id TEXT NOT NULL,\n\
             transcript_status TEXT NOT NULL CHECK (transcript_status IN ('complete', 'partial')),\n\
             notes_revision INTEGER,\n\
             template_name TEXT NOT NULL,\n\
             template_prompt TEXT NOT NULL,\n\
             model TEXT NOT NULL,\n\
             PRIMARY KEY (session_id, template_id)\n\
         );",
    ),
    M::up(
        "CREATE UNIQUE INDEX transcripts_one_retranscribe_per_session\n\
             ON transcripts (session_id) WHERE variant = 'retranscribe';",
    ),
    // Mỗi Phiên chỉ còn một transcript: bản `retranscribe` (nếu có) là bản
    // mới nhất nên thay `primary` cũ và trở thành `primary`. Ràng buộc/chỉ
    // mục cũ giữ nguyên (schema không đổi), chỉ dữ liệu được chuyển.
    M::up(
        "DELETE FROM transcripts\n\
             WHERE variant = 'primary'\n\
               AND session_id IN (SELECT session_id FROM transcripts WHERE variant = 'retranscribe');\n\
         UPDATE transcripts SET variant = 'primary' WHERE variant = 'retranscribe';",
    ),
    // Record the last remote recommendation accepted for each built-in
    // template. Comparing this snapshot with the current row detects local
    // edits before a later signed recommendation is applied.
    M::up(
        "CREATE TABLE recommended_template_state (\n\
             locale TEXT NOT NULL,\n\
             external_id TEXT NOT NULL,\n\
             recommended_name TEXT NOT NULL,\n\
             recommended_prompt TEXT NOT NULL,\n\
             updated_at INTEGER NOT NULL,\n\
             PRIMARY KEY (locale, external_id)\n\
         );",
    ),
    // Ads impressions and action counts are local-only. Creative IDs are
    // opaque keys and are never used as filenames or sent back to a server.
    M::up(
        "CREATE TABLE ad_creative_state (\n\
             creative_id TEXT PRIMARY KEY,\n\
             last_displayed_at INTEGER,\n\
             impressions INTEGER NOT NULL DEFAULT 0 CHECK (impressions >= 0),\n\
             clicks INTEGER NOT NULL DEFAULT 0 CHECK (clicks >= 0),\n\
             reports INTEGER NOT NULL DEFAULT 0 CHECK (reports >= 0)\n\
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
    fn migration_11_creates_ad_creative_state() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        assert!(table_exists(&conn, "recommended_template_state"));
        assert!(table_exists(&conn, "ad_creative_state"));
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 11);
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
        assert_eq!(version_after_first, 11);
        assert_eq!(version_after_second, 11);
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
    fn migration_9_promotes_retranscribe_over_primary_and_keeps_lone_primaries() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        runner().to_version(&mut conn, 8).unwrap();
        for id in ["s1", "s2"] {
            conn.execute(
                "INSERT INTO sessions (id, kind, title, status, duration_sec, created_at, updated_at) \
                 VALUES (?1, 'live', 't', 'complete', 1.0, 0, 0)",
                [id],
            )
            .unwrap();
        }
        for (id, session, variant) in [
            ("p1", "s1", "primary"),
            ("r1", "s1", "retranscribe"),
            ("p2", "s2", "primary"),
        ] {
            conn.execute(
                "INSERT INTO transcripts (id, session_id, variant, status, model, created_at) \
                 VALUES (?1, ?2, ?3, 'complete', 'm', 0)",
                [id, session, variant],
            )
            .unwrap();
        }
        run(&mut conn).unwrap();
        let rows: Vec<(String, String)> = conn
            .prepare("SELECT id, variant FROM transcripts ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![
                ("p2".to_string(), "primary".to_string()),
                ("r1".to_string(), "primary".to_string()),
            ]
        );
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

    /// Acceptance Criteria: "Given app khởi động trên DB cũ (3 migration),
    /// when chạy migration, then có `tags`/`session_tags`, `user_version` =
    /// 4, dữ liệu cũ giữ nguyên." Dựng một DB dừng lại đúng ở migration thứ 3
    /// (chưa biết gì về `tags`), chèn một Phiên, rồi chạy `run` (mọi migration,
    /// bao gồm thứ 4 mới) và kiểm dữ liệu cũ còn nguyên cạnh bảng mới.
    #[test]
    fn upgrading_from_three_migrations_adds_tags_tables_and_keeps_old_data() {
        let mut conn = Connection::open_in_memory().unwrap();
        Migrations::from_slice(&MIGRATIONS[..3])
            .to_latest(&mut conn)
            .unwrap();
        conn.execute(
            "INSERT INTO sessions (id, kind, title, status, duration_sec, created_at, updated_at) \
             VALUES ('s1', 'file', 'cuộc họp cũ', 'complete', 1.0, 0, 0)",
            [],
        )
        .unwrap();
        let version_before: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version_before, 3);

        run(&mut conn).unwrap();

        let version_after: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version_after, 11);
        assert!(table_exists(&conn, "tags"));
        assert!(table_exists(&conn, "session_tags"));
        let title: String = conn
            .query_row("SELECT title FROM sessions WHERE id = 's1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(title, "cuộc họp cũ", "dữ liệu cũ phải giữ nguyên");
    }

    #[test]
    fn creates_tags_and_session_tags_tables() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        assert!(table_exists(&conn, "tags"));
        assert!(table_exists(&conn, "session_tags"));
    }

    #[test]
    fn tags_name_key_is_unique() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO tags (id, name, name_key, created_at) VALUES ('t1', 'DỰ ÁN', 'dự án', 0)",
            [],
        )
        .unwrap();
        let err = conn
            .execute(
                "INSERT INTO tags (id, name, name_key, created_at) VALUES ('t2', 'dự án', 'dự án', 0)",
                [],
            )
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("unique"));
    }

    #[test]
    fn deleting_a_session_cascades_to_session_tags_but_keeps_the_tag() {
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
            "INSERT INTO tags (id, name, name_key, created_at) VALUES ('t1', 'A', 'a', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session_tags (session_id, tag_id) VALUES ('s1', 't1')",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM sessions WHERE id = 's1'", [])
            .unwrap();

        let link_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM session_tags", [], |row| row.get(0))
            .unwrap();
        assert_eq!(link_count, 0, "xoá Phiên phải gỡ liên kết session_tags");
        let tag_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tags", [], |row| row.get(0))
            .unwrap();
        assert_eq!(tag_count, 1, "xoá Phiên không được đụng tới tag");
    }

    #[test]
    fn deleting_a_tag_cascades_to_session_tags_but_keeps_the_session() {
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
            "INSERT INTO tags (id, name, name_key, created_at) VALUES ('t1', 'A', 'a', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session_tags (session_id, tag_id) VALUES ('s1', 't1')",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM tags WHERE id = 't1'", [])
            .unwrap();

        let link_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM session_tags", [], |row| row.get(0))
            .unwrap();
        assert_eq!(link_count, 0, "xoá tag phải gỡ liên kết session_tags");
        let session_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(session_count, 1, "xoá tag không được đụng tới Phiên");
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

    #[test]
    fn creates_notes_table() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        assert!(table_exists(&conn, "notes"));
    }

    /// Story 3.5 spec Boundaries Always: "xoá Phiên/xoá toàn bộ dọn ghi chú
    /// qua cascade".
    #[test]
    fn deleting_a_session_cascades_to_its_note() {
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
            "INSERT INTO notes (session_id, body, revision, updated_at) VALUES ('s1', 'ghi chú', 1, 0)",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM sessions WHERE id = 's1'", [])
            .unwrap();

        let note_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(note_count, 0, "xoá Phiên phải xoá theo ghi chú của nó");
    }

    /// Story 3.5 spec Boundaries Always: `notes.session_id` là PK -- một
    /// Phiên tối đa một dòng ghi chú.
    #[test]
    fn notes_session_id_is_primary_key_and_rejects_a_second_row() {
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
            "INSERT INTO notes (session_id, body, revision, updated_at) VALUES ('s1', 'a', 1, 0)",
            [],
        )
        .unwrap();
        let err = conn
            .execute(
                "INSERT INTO notes (session_id, body, revision, updated_at) VALUES ('s1', 'b', 2, 1)",
                [],
            )
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("unique"));
    }

    #[test]
    fn creates_memo_templates_table() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        assert!(table_exists(&conn, "memo_templates"));
    }

    /// Story 3.6 spec Boundaries Always: `UNIQUE(locale, default_key)` --
    /// hai mẫu mặc định cùng `(locale, default_key)` bị chặn, nhưng nhiều mẫu
    /// người dùng cùng `(NULL, NULL)` không bị chặn (SQLite coi mỗi NULL là
    /// khác nhau trong ràng buộc UNIQUE).
    #[test]
    fn memo_templates_locale_default_key_is_unique_but_null_rows_are_not() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO memo_templates (id, name, prompt, is_default, locale, default_key, created_at, updated_at) \
             VALUES ('t1', 'a', '{transcript}', 1, 'vi', 'meeting-minutes', 0, 0)",
            [],
        )
        .unwrap();
        let err = conn
            .execute(
                "INSERT INTO memo_templates (id, name, prompt, is_default, locale, default_key, created_at, updated_at) \
                 VALUES ('t2', 'b', '{transcript}', 1, 'vi', 'meeting-minutes', 0, 0)",
                [],
            )
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("unique"));

        // Hai mẫu người dùng (locale/default_key đều NULL) chèn được cả hai.
        conn.execute(
            "INSERT INTO memo_templates (id, name, prompt, is_default, locale, default_key, created_at, updated_at) \
             VALUES ('u1', 'của tôi 1', '{transcript}', 0, NULL, NULL, 0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memo_templates (id, name, prompt, is_default, locale, default_key, created_at, updated_at) \
             VALUES ('u2', 'của tôi 2', '{transcript}', 0, NULL, NULL, 0, 0)",
            [],
        )
        .unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM memo_templates", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn memo_templates_is_default_check_rejects_values_outside_zero_or_one() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        let err = conn
            .execute(
                "INSERT INTO memo_templates (id, name, prompt, is_default, locale, default_key, created_at, updated_at) \
                 VALUES ('t1', 'a', '{transcript}', 2, NULL, NULL, 0, 0)",
                [],
            )
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("check"));
    }

    #[test]
    fn creates_memos_table() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        assert!(table_exists(&conn, "memos"));
    }

    /// Story 3.7 spec Boundaries Always: `PRIMARY KEY(session_id,
    /// template_id)` -- một cặp Phiên/Template tối đa một dòng memo.
    #[test]
    fn memos_primary_key_is_the_session_template_pair() {
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
            "INSERT INTO memos (session_id, template_id, body, created_at, transcript_id, \
             transcript_status, notes_revision, template_name, template_prompt, model) \
             VALUES ('s1', 'tmpl-1', 'nội dung', 0, 't1', 'complete', 3, 'Biên bản họp', '{transcript}', 'gemini-flash-lite-latest')",
            [],
        )
        .unwrap();
        let err = conn
            .execute(
                "INSERT INTO memos (session_id, template_id, body, created_at, transcript_id, \
                 transcript_status, notes_revision, template_name, template_prompt, model) \
                 VALUES ('s1', 'tmpl-1', 'nội dung khác', 1, 't2', 'partial', NULL, 'Tên khác', '{transcript}', 'model-b')",
                [],
            )
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("unique"));
    }

    /// Story 3.7 spec Boundaries Always: xoá Phiên dọn theo memo của nó qua
    /// `ON DELETE CASCADE` -- `template_id` sống sót (không FK) dù template
    /// đã bị xoá trước đó không đụng gì tới dòng memo.
    #[test]
    fn deleting_a_session_cascades_to_its_memos_but_template_id_is_not_a_foreign_key() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        run(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, kind, title, status, duration_sec, created_at, updated_at) \
             VALUES ('s1', 'file', 't', 'complete', 1.0, 0, 0)",
            [],
        )
        .unwrap();
        // `template_id = 'template-da-bi-xoa'` không tồn tại ở `memo_templates`
        // -- vẫn chèn được vì cột này không phải FK.
        conn.execute(
            "INSERT INTO memos (session_id, template_id, body, created_at, transcript_id, \
             transcript_status, notes_revision, template_name, template_prompt, model) \
             VALUES ('s1', 'template-da-bi-xoa', 'nội dung', 0, 't1', 'complete', NULL, 'Tên', '{transcript}', 'm')",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM sessions WHERE id = 's1'", [])
            .unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM memos", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0, "xoá Phiên phải xoá theo memo của nó");
    }

    #[test]
    fn memos_transcript_status_check_rejects_values_outside_complete_or_partial() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        run(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, kind, title, status, duration_sec, created_at, updated_at) \
             VALUES ('s1', 'file', 't', 'complete', 1.0, 0, 0)",
            [],
        )
        .unwrap();
        let err = conn
            .execute(
                "INSERT INTO memos (session_id, template_id, body, created_at, transcript_id, \
                 transcript_status, notes_revision, template_name, template_prompt, model) \
                 VALUES ('s1', 'tmpl-1', 'x', 0, 't1', 'bogus', NULL, 'Tên', '{transcript}', 'm')",
                [],
            )
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("check"));
    }

    #[test]
    fn only_one_retranscribe_transcript_allowed_per_session_but_primary_is_independent() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        run(&mut conn).unwrap();
        for id in ["s1", "s2"] {
            conn.execute(
                "INSERT INTO sessions (id, kind, title, status, duration_sec, created_at, updated_at) \
                 VALUES (?1, 'live', 't', 'complete', 1.0, 0, 0)",
                [id],
            )
            .unwrap();
        }
        let insert = |conn: &Connection, id: &str, session: &str, variant: &str| {
            conn.execute(
                "INSERT INTO transcripts (id, session_id, variant, status, model, created_at) \
                 VALUES (?1, ?2, ?3, 'complete', 'm', 0)",
                [id, session, variant],
            )
        };
        insert(&conn, "p1", "s1", "primary").unwrap();
        insert(&conn, "r1", "s1", "retranscribe").unwrap();
        let err = insert(&conn, "r2", "s1", "retranscribe").unwrap_err();
        assert!(err.to_string().to_lowercase().contains("unique"));
        // Another session may have its own retranscribe.
        insert(&conn, "r3", "s2", "retranscribe").unwrap();
        // The unique index is partial: a primary next to it is unaffected.
        insert(&conn, "p2", "s2", "primary").unwrap();
    }
}
