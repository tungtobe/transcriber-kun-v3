//! Settings: service typed sở hữu cấu hình app, có mặc định cho từng khoá,
//! đọc/ghi qua `db::repo::settings` (giá trị lưu dạng JSON text), phát event
//! khi đổi (AD-8). Story này chỉ có một khoá `theme`; story sau thêm khoá
//! mới vào struct `Settings` mà không đổi shape đã có (spec Decisions).

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::core::error::{AppError, Code};
use crate::db::{repo, Db};

/// `theme: 'system' | 'light' | 'dark'`, mặc định `system` (spec Decisions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub theme: Theme,
}

/// Phát khi `save` ghi bền thành công — đúng một lần, mang giá trị mới toàn
/// bộ (không phát khi ghi lỗi, spec I/O Matrix).
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct SettingsChanged(pub Settings);

const KEY_THEME: &str = "theme";

/// Đọc toàn bộ settings từ DB. Khoá thiếu hoặc value không parse được (hỏng)
/// dùng mặc định của khoá đó, khoá khác giữ nguyên (spec I/O Matrix). Không
/// có khoá thứ hai ở story này nên "khoá khác giữ nguyên" hiện là vô hiệu,
/// nhưng cấu trúc match theo từng khoá đã sẵn cho story sau thêm khoá mà
/// không phá vỡ tính chất này.
///
/// Không tự trả lỗi: nếu tự thân việc đọc DB lỗi (không phải do value hỏng,
/// mà do lỗi I/O/khoá), fallback về mặc định toàn bộ và chỉ log cảnh báo —
/// `settings_get` không bao giờ fail vì lý do "chưa có gì để đọc".
pub fn load(db: &Db) -> Settings {
    let raw = db
        .with_connection(|conn| Ok(repo::settings::read_all(conn)?))
        .unwrap_or_else(|err| {
            tracing::warn!(error = %err, "không đọc được settings, dùng toàn bộ giá trị mặc định");
            Default::default()
        });

    let theme = match raw.get(KEY_THEME) {
        None => Theme::default(),
        Some(value) => serde_json::from_str::<Theme>(value).unwrap_or_else(|_| {
            tracing::warn!(
                key = KEY_THEME,
                "giá trị settings không parse được, dùng mặc định"
            );
            Theme::default()
        }),
    };

    Settings { theme }
}

/// Ghi toàn bộ settings trong một transaction (spec Boundaries). Lỗi ghi trả
/// `AppError` category `storage`; người gọi (ipc) chỉ phát `SettingsChanged`
/// khi hàm này trả `Ok`.
pub fn save(db: &Db, settings: &Settings) -> Result<(), AppError> {
    let theme_json = serde_json::to_string(&settings.theme)
        .map_err(|err| AppError::new(Code::Format, err.to_string()))?;

    db.with_connection(|conn| {
        Ok(repo::settings::upsert_many(
            conn,
            &[(KEY_THEME, theme_json)],
        )?)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;
    use tracing_subscriber::fmt::MakeWriter;

    /// `MakeWriter` ghi vào một buffer trong bộ nhớ dùng chung — cho phép
    /// test cài một subscriber cục bộ (qua `tracing::subscriber::with_default`)
    /// rồi đọc lại đúng những gì đã log, không đụng file thật hay subscriber
    /// toàn cục.
    #[derive(Clone, Default)]
    struct SharedBuf(Arc<Mutex<Vec<u8>>>);

    impl SharedBuf {
        fn contents(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
        }
    }

    impl io::Write for SharedBuf {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for SharedBuf {
        type Writer = SharedBuf;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    fn open_db() -> Db {
        let dir = tempdir().unwrap();
        // `Db::open` chỉ nhận thư mục — giữ `dir` sống bằng cách "quên" nó có
        // chủ đích (test ngắn hạn, thư mục dọn theo tiến trình test); `keep`
        // tránh xoá sớm khi `dir` ra khỏi scope.
        Db::open(&dir.keep()).unwrap()
    }

    #[test]
    fn load_on_empty_table_returns_defaults() {
        let db = open_db();
        assert_eq!(
            load(&db),
            Settings {
                theme: Theme::System
            }
        );
    }

    #[test]
    fn save_then_load_round_trips() {
        let db = open_db();
        save(&db, &Settings { theme: Theme::Dark }).unwrap();
        assert_eq!(load(&db), Settings { theme: Theme::Dark });
    }

    #[test]
    fn load_falls_back_to_default_when_value_is_corrupt() {
        let db = open_db();
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('theme', 'not-json')",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        assert_eq!(
            load(&db),
            Settings {
                theme: Theme::System
            }
        );
    }

    /// I/O Matrix "Settings hỏng", cột "Log cảnh báo không chứa value": khi
    /// value lưu không parse được, cảnh báo phải nhắc đến việc đó nhưng
    /// tuyệt đối không được chứa giá trị hỏng gốc — dùng một marker riêng để
    /// khẳng định rõ ràng, không suy đoán từ nội dung warn hiện có.
    #[test]
    fn load_logs_warning_without_leaking_corrupt_raw_value() {
        let db = open_db();
        let secret_marker = "SECRET-CORRUPT-VALUE-9f3ac1";
        let corrupt_value = format!("not-json-{secret_marker}");
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('theme', ?1)",
                rusqlite::params![corrupt_value],
            )?;
            Ok(())
        })
        .unwrap();

        let buf = SharedBuf::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(buf.clone())
            .with_ansi(false)
            .finish();

        let settings = tracing::subscriber::with_default(subscriber, || load(&db));

        assert_eq!(
            settings,
            Settings {
                theme: Theme::System
            }
        );

        let captured = buf.contents();
        assert!(
            captured.to_lowercase().contains("warn"),
            "phải có một cảnh báo được ghi khi value hỏng, thực tế log: {captured:?}"
        );
        assert!(
            !captured.contains(secret_marker),
            "cảnh báo không được chứa giá trị hỏng gốc, thực tế log: {captured:?}"
        );
        assert!(!captured.contains(&corrupt_value));
    }

    #[test]
    fn theme_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&Theme::System).unwrap(), "\"system\"");
        assert_eq!(serde_json::to_string(&Theme::Light).unwrap(), "\"light\"");
        assert_eq!(serde_json::to_string(&Theme::Dark).unwrap(), "\"dark\"");
    }
}
