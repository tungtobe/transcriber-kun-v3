//! DB: một `rusqlite::Connection` (WAL) duy nhất, do module này giữ và cấp
//! phát truy cập tuần tự qua [`Db::with_connection`]. SQL chỉ nằm trong
//! `repo/<entity>.rs` và `migrations/mod.rs` (spec Boundaries) — mọi nơi
//! khác gọi qua các hàm ở đây hoặc ở `repo::*`, không tự viết SQL.
//!
//! Command Tauri chạy đồng bộ trên main thread; command nào chạm DB phải bọc
//! lời gọi vào `tauri::async_runtime::spawn_blocking` ở tầng `ipc::` (Code
//! Map) — `Db` chỉ cần `Send + Sync` để việc đó an toàn, đã thoả vì
//! `Mutex<Connection>` là `Sync` khi `Connection: Send`.

pub mod migrations;
pub mod repo;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::core::error::{AppError, Code};

/// Tên file DB chính dưới `app_data_dir` -- `pub(crate)` (không còn riêng
/// module này) vì story 3.4's `library::store::storage_stats` cần ghép đúng
/// tên này với hậu tố `-wal`/`-shm` để đo dung lượng DB mà không lặp lại
/// chuỗi `"app.db"` ở một nơi thứ hai.
pub(crate) const DB_FILE_NAME: &str = "app.db";

#[derive(Debug)]
pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    /// Mở (hoặc tạo) `<dir>/app.db`, bật WAL, chạy migration tới bản mới
    /// nhất. `dir` phải đã tồn tại hoặc tạo được — lỗi tạo thư mục/mở DB/
    /// migration đều quy về category `storage`.
    pub fn open(dir: &Path) -> Result<Self, AppError> {
        std::fs::create_dir_all(dir)?;

        let mut conn = Connection::open(dir.join(DB_FILE_NAME))?;

        // Bật ràng buộc khoá ngoại cho connection này (story 2.3 Tasks: "bật
        // foreign keys"). SQLite tắt mặc định và đây là pragma theo-connection
        // (không lưu trong file DB), nên phải đặt lại mỗi lần mở — có đúng một
        // connection sống suốt vòng đời app ở đây nên đặt một lần là đủ. Cần
        // bật trước khi insert dữ liệu tham chiếu (`transcripts.session_id`,
        // `segments.transcript_id`) để `ON DELETE CASCADE` thực sự chạy.
        conn.pragma_update(None, "foreign_keys", true)?;

        // `PRAGMA journal_mode = WAL` luôn trả một hàng kết quả (chế độ áp
        // dụng được) kể cả khi dùng để "set" — phải `query_row`, không
        // `pragma_update`, nếu không rusqlite sẽ coi là lỗi "unexpected row".
        let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(AppError::new(
                Code::Storage,
                format!("không bật được WAL, journal_mode hiện tại: {mode}"),
            ));
        }

        // Story 3.5 (OQ10 autorun, 2026-09-27): `synchronous = FULL` đặt
        // tường minh -- WAL mặc định `NORMAL`, không đủ mạnh cho cơ chế bền
        // của `notes_save` (ACK = sau khi transaction thật sự `fsync` xuống
        // đĩa, không chỉ ghi vào WAL trong bộ nhớ trang OS). Không trả hàng
        // kết quả như `journal_mode` nên dùng `pragma_update`, không
        // `query_row`.
        conn.pragma_update(None, "synchronous", "FULL")?;

        migrations::run(&mut conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Truy cập tuần tự tới connection duy nhất. `mutex` bị poison (một
    /// closure trước đó panic khi đang giữ lock) cũng quy về `storage` thay
    /// vì panic tiếp — DB không mở được không được phép làm sập app (spec
    /// I/O Matrix).
    pub fn with_connection<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut guard = self
            .conn
            .lock()
            .map_err(|_| AppError::new(Code::Storage, "db mutex poisoned"))?;
        f(&mut guard)
    }

    /// Chạy `PRAGMA wal_checkpoint(TRUNCATE)` (story 3.4, spec Always: "sau
    /// đó `wal_checkpoint(TRUNCATE)` và đo lại thực tế — không giả định DB =
    /// 0"). Gọi sau `wipe_all` xoá sạch `sessions`/`tags`: dồn nội dung WAL
    /// vào `app.db` rồi cắt file `-wal` về gần 0 byte, để lần đo dung lượng
    /// kế tiếp phản ánh đúng thực tế thay vì vẫn thấy WAL to từ trước khi
    /// xoá. Giống `PRAGMA journal_mode` ở [`Db::open`], `wal_checkpoint` luôn
    /// trả một hàng (busy, log, checkpointed) nên phải `query_row`.
    pub fn checkpoint_truncate(&self) -> Result<(), AppError> {
        self.with_connection(|conn| {
            conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_row| Ok(()))?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn open_creates_db_file_with_wal_and_latest_user_version() {
        let dir = tempdir().unwrap();
        let db = Db::open(dir.path()).expect("phải mở được DB trên thư mục ghi được");

        assert!(dir.path().join("app.db").exists());
        // WAL mode luôn kèm file `-wal` sau lần ghi đầu (ở đây migration đã
        // tạo bảng `settings` nên file `-wal`/`-shm` đã tồn tại) — Acceptance
        // Criteria kiểm rõ sự tồn tại của `app.db-wal`.
        assert!(dir.path().join("app.db-wal").exists());
        db.with_connection(|conn| {
            let mode: String = conn
                .query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .unwrap();
            assert_eq!(mode.to_lowercase(), "wal");
            let version: i64 = conn
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .unwrap();
            assert_eq!(version, 5);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn foreign_keys_pragma_is_on() {
        let dir = tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        db.with_connection(|conn| {
            let on: i64 = conn
                .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
                .unwrap();
            assert_eq!(on, 1);
            Ok(())
        })
        .unwrap();
    }

    /// Story 3.5 (OQ10 autorun): `synchronous = FULL` đặt tường minh trong
    /// `Db::open`, không dựa vào mặc định `NORMAL` của WAL.
    #[test]
    fn synchronous_pragma_is_full() {
        let dir = tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        db.with_connection(|conn| {
            // SQLite trả số nguyên cho `PRAGMA synchronous`: 0=OFF, 1=NORMAL,
            // 2=FULL, 3=EXTRA.
            let mode: i64 = conn
                .query_row("PRAGMA synchronous", [], |row| row.get(0))
                .unwrap();
            assert_eq!(mode, 2, "synchronous phải là FULL (2)");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn deleting_a_transcript_cascades_to_its_segments() {
        let dir = tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO sessions (id, kind, title, status, duration_sec, created_at, updated_at) \
                 VALUES ('s1', 'file', 't', 'complete', 1.0, 0, 0)",
                [],
            )?;
            conn.execute(
                "INSERT INTO transcripts (id, session_id, variant, status, model, created_at) \
                 VALUES ('t1', 's1', 'primary', 'complete', 'm', 0)",
                [],
            )?;
            conn.execute(
                "INSERT INTO segments (transcript_id, idx, start_sec, end_sec, kind, text) \
                 VALUES ('t1', 0, 0.0, 1.0, 'text', 'hello')",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        db.with_connection(|conn| {
            conn.execute("DELETE FROM transcripts WHERE id = 't1'", [])?;
            Ok(())
        })
        .unwrap();

        db.with_connection(|conn| {
            let count: i64 = conn
                .query_row("SELECT count(*) FROM segments", [], |row| row.get(0))
                .unwrap();
            assert_eq!(count, 0, "xoá transcript phải kéo theo xoá segments của nó");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn deleting_a_session_cascades_through_transcripts_to_segments() {
        let dir = tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO sessions (id, kind, title, status, duration_sec, created_at, updated_at) \
                 VALUES ('s1', 'file', 't', 'complete', 1.0, 0, 0)",
                [],
            )?;
            conn.execute(
                "INSERT INTO transcripts (id, session_id, variant, status, model, created_at) \
                 VALUES ('t1', 's1', 'primary', 'complete', 'm', 0)",
                [],
            )?;
            conn.execute(
                "INSERT INTO segments (transcript_id, idx, start_sec, end_sec, kind, text) \
                 VALUES ('t1', 0, 0.0, 1.0, 'text', 'hello')",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        db.with_connection(|conn| {
            conn.execute("DELETE FROM sessions WHERE id = 's1'", [])?;
            Ok(())
        })
        .unwrap();

        db.with_connection(|conn| {
            let transcripts: i64 = conn
                .query_row("SELECT count(*) FROM transcripts", [], |row| row.get(0))
                .unwrap();
            let segments: i64 = conn
                .query_row("SELECT count(*) FROM segments", [], |row| row.get(0))
                .unwrap();
            assert_eq!(transcripts, 0);
            assert_eq!(segments, 0);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn reopening_existing_db_is_a_noop_and_keeps_data() {
        let dir = tempdir().unwrap();
        {
            let db = Db::open(dir.path()).unwrap();
            db.with_connection(|conn| {
                conn.execute(
                    "INSERT INTO settings (key, value) VALUES ('theme', '\"dark\"')",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        }

        let db = Db::open(dir.path()).expect("mở lại DB đã tồn tại phải thành công");
        let value: String = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT value FROM settings WHERE key = 'theme'",
                    [],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(value, "\"dark\"");
    }

    #[test]
    fn open_fails_with_storage_category_when_dir_not_writable() {
        // Dựng một đường dẫn cha không tồn tại và không thể tạo được: dùng
        // một file thường làm "thư mục cha" — `create_dir_all` phải lỗi vì
        // đường dẫn đó đã bị chiếm bởi một file.
        let dir = tempdir().unwrap();
        let blocking_file = dir.path().join("not-a-dir");
        std::fs::write(&blocking_file, b"x").unwrap();
        let target = blocking_file.join("nested");

        let err = Db::open(&target).expect_err("phải lỗi khi thư mục cha là một file");
        assert_eq!(err.category, crate::core::error::Category::Storage);
    }
}
