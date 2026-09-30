//! Settings: service typed sở hữu cấu hình app, có mặc định cho từng khoá,
//! đọc/ghi qua `db::repo::settings` (giá trị lưu dạng JSON text), phát event
//! khi đổi (AD-8). Mỗi khoá được parse/fallback độc lập để một giá trị hỏng
//! không làm mất các preference còn lại.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::core::error::{AppError, Code};
use crate::core::model_defaults::{
    DEFAULT_LIVE_MODEL, DEFAULT_MEMO_MODEL, DEFAULT_TRANSCRIBE_MODEL,
};
use crate::db::{repo, Db};

pub mod recommended;

/// `theme: 'system' | 'light' | 'dark'`, mặc định `system` (spec Decisions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// Preference ngôn ngữ UI. `System` resolve ở frontend từ locale của WebView;
/// Rust chỉ sở hữu giá trị persisted và không đoán locale hệ điều hành.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum UiLanguage {
    #[default]
    System,
    Vi,
    En,
    Ja,
}

/// Ngôn ngữ transcribe (story 2.6, không phải ngôn ngữ UI): `Auto` giữ
/// nguyên hành vi tự phát hiện hiện có (prompt/request JSON byte-for-byte
/// không đổi — spec Always); `Ja|Vi|En` thêm một câu chỉ định ngôn ngữ chính
/// vào prompt (spec `transcribe::adapter::build_general_request`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum TranscribeLanguage {
    #[default]
    Auto,
    Ja,
    Vi,
    En,
}

impl TranscribeLanguage {
    /// Mã lưu vào `transcripts.language` -- `None` (NULL) cho `auto` (spec
    /// Always: "`transcripts.language` lưu mã ngôn ngữ đã chụp (`NULL` khi
    /// `auto`)").
    pub fn as_code(self) -> Option<&'static str> {
        match self {
            TranscribeLanguage::Auto => None,
            TranscribeLanguage::Ja => Some("ja"),
            TranscribeLanguage::Vi => Some("vi"),
            TranscribeLanguage::En => Some("en"),
        }
    }
}

/// Ngôn ngữ đích dịch realtime của Live (story 5.1): `None` = "Không dịch".
/// Độc lập với ngôn ngữ UI và ngôn ngữ transcribe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum LiveTarget {
    #[default]
    None,
    Ja,
    Vi,
    En,
}

impl LiveTarget {
    /// Mã `targetLanguageCode` gửi cho Gemini; `None` khi không dịch.
    pub fn as_code(self) -> Option<&'static str> {
        match self {
            LiveTarget::None => None,
            LiveTarget::Ja => Some("ja"),
            LiveTarget::Vi => Some("vi"),
            LiveTarget::En => Some("en"),
        }
    }
}

/// `chunkMinutes` mặc định (spec Approach: "mặc định 5") -- cũng là giá trị
/// `load` fallback về khi khoá thiếu hoặc hỏng.
const DEFAULT_CHUNK_MINUTES: u32 = 5;

/// Trần trên của `chunkMinutes` (spec Boundaries Always P0 review: "là số
/// nguyên trong `1..=60`") -- một giá trị lớn hơn từng bị `Chunker::new`
/// hiểu là "cấp phát trước ngần này mẫu" và có thể abort tiến trình
/// (`Vec::with_capacity` OOM) trước khi kịp validate lại ở `media::chunk`.
const MAX_CHUNK_MINUTES: u32 = 60;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub theme: Theme,
    pub ui_language: UiLanguage,
    pub onboarding_completed: bool,
    /// The only consent version the user has accepted. `0` means never
    /// accepted; this is intentionally independent from `consent_declined`.
    pub consent_accepted_version: u32,
    /// A separate decision bit lets the router distinguish a deliberate
    /// decline from a first-run pending state.
    pub consent_declined: bool,
    /// Free-text model name used by file transcription. Never validated
    /// against a loaded model list (spec Always: "Tên không có trong danh
    /// sách đã tải vẫn lưu được"); only non-empty-after-trim is enforced.
    pub transcribe_model: String,
    /// Free-text model name used by Live.
    pub live_model: String,
    /// Free-text model name used by Memo.
    pub memo_model: String,
    /// Độ dài Chunk khi transcribe file (phút), số nguyên >= 1 (spec
    /// Boundaries Always). Job chụp giá trị này lúc nhận Job
    /// (`transcribe_start`/`transcribe_rerun`) -- đổi Settings giữa chừng
    /// không ảnh hưởng Job đang chạy/chờ.
    pub chunk_minutes: u32,
    /// Offset cộng vào timestamp hiển thị/export (giây), số nguyên >= 0 --
    /// không có offset âm (spec quyết định). Thuần hiển thị: không bao giờ
    /// ghi vào Segment/DB (spec Always) -- chỉ đọc qua `src/lib/time.ts` ở
    /// frontend.
    pub timestamp_offset_sec: u32,
    /// Ngôn ngữ transcribe file/Live (không phải ngôn ngữ UI). Job chụp cùng
    /// lúc với `chunk_minutes`/`model`.
    pub transcribe_language: TranscribeLanguage,
    /// Target dịch realtime mặc định của Live (nhóm Settings "Live").
    pub live_target: LiveTarget,
}

/// Derived manually (not `#[derive(Default)]`) so the three model fields
/// default to the shared Gemini defaults instead of an empty string — an
/// empty string is rejected by [`save`], so `Settings::default()` must
/// already be a value `save` accepts (spec: "mặc định lấy từ
/// `gemini::params::DEFAULT_*` (hoặc ... định nghĩa default trong
/// `core`/settings)").
impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            ui_language: UiLanguage::default(),
            onboarding_completed: false,
            consent_accepted_version: 0,
            consent_declined: false,
            transcribe_model: DEFAULT_TRANSCRIBE_MODEL.to_string(),
            live_model: DEFAULT_LIVE_MODEL.to_string(),
            memo_model: DEFAULT_MEMO_MODEL.to_string(),
            chunk_minutes: DEFAULT_CHUNK_MINUTES,
            timestamp_offset_sec: 0,
            transcribe_language: TranscribeLanguage::default(),
            live_target: LiveTarget::default(),
        }
    }
}

/// Phát khi `save` ghi bền thành công — đúng một lần, mang giá trị mới toàn
/// bộ (không phát khi ghi lỗi, spec I/O Matrix).
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct SettingsChanged(pub Settings);

const KEY_THEME: &str = "theme";
const KEY_UI_LANGUAGE: &str = "uiLanguage";
const KEY_ONBOARDING_COMPLETED: &str = "onboardingCompleted";
const KEY_CONSENT_ACCEPTED_VERSION: &str = "consentAcceptedVersion";
const KEY_CONSENT_DECLINED: &str = "consentDeclined";
const KEY_TRANSCRIBE_MODEL: &str = "transcribeModel";
const KEY_LIVE_MODEL: &str = "liveModel";
const KEY_MEMO_MODEL: &str = "memoModel";
const KEY_CHUNK_MINUTES: &str = "chunkMinutes";
const KEY_TIMESTAMP_OFFSET_SEC: &str = "timestampOffsetSec";
const KEY_TRANSCRIBE_LANGUAGE: &str = "transcribeLanguage";
const KEY_LIVE_TARGET: &str = "liveTarget";

/// Shared by `load`'s three model branches: missing key, corrupt JSON, and a
/// parsed-but-blank string (e.g. a hand-edited DB row) all fall back to
/// `default` the same way (spec: "load fallback về mặc định khi thiếu hoặc
/// hỏng").
fn load_model_field(
    raw: &std::collections::HashMap<String, String>,
    key: &str,
    default: &str,
) -> String {
    match raw.get(key) {
        None => default.to_string(),
        Some(value) => match serde_json::from_str::<String>(value) {
            Ok(parsed) if !parsed.trim().is_empty() => {
                crate::core::model_defaults::bare_model_name(&parsed).to_string()
            }
            _ => {
                tracing::warn!(key, "giá trị settings không parse được, dùng mặc định");
                default.to_string()
            }
        },
    }
}

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

    load_from_raw(&raw)
}

/// Parse a raw settings snapshot. The recommendation service uses this from
/// the same SQLite transaction that reads template state, so its preview
/// revision always describes one consistent local snapshot.
pub(crate) fn load_from_raw(raw: &std::collections::HashMap<String, String>) -> Settings {
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

    let ui_language = match raw.get(KEY_UI_LANGUAGE) {
        None => UiLanguage::default(),
        Some(value) => serde_json::from_str::<UiLanguage>(value).unwrap_or_else(|_| {
            tracing::warn!(
                key = KEY_UI_LANGUAGE,
                "giá trị settings không parse được, dùng mặc định"
            );
            UiLanguage::default()
        }),
    };

    let onboarding_completed = match raw.get(KEY_ONBOARDING_COMPLETED) {
        None => false,
        Some(value) => serde_json::from_str::<bool>(value).unwrap_or_else(|_| {
            tracing::warn!(
                key = KEY_ONBOARDING_COMPLETED,
                "giá trị settings không parse được, dùng mặc định"
            );
            false
        }),
    };

    let consent_accepted_version = match raw.get(KEY_CONSENT_ACCEPTED_VERSION) {
        None => 0,
        Some(value) => serde_json::from_str::<u32>(value).unwrap_or_else(|_| {
            tracing::warn!(
                key = KEY_CONSENT_ACCEPTED_VERSION,
                "giá trị settings không parse được, dùng mặc định"
            );
            0
        }),
    };

    let consent_declined = match raw.get(KEY_CONSENT_DECLINED) {
        None => false,
        Some(value) => serde_json::from_str::<bool>(value).unwrap_or_else(|_| {
            tracing::warn!(
                key = KEY_CONSENT_DECLINED,
                "giá trị settings không parse được, dùng mặc định"
            );
            false
        }),
    };

    let transcribe_model = load_model_field(&raw, KEY_TRANSCRIBE_MODEL, DEFAULT_TRANSCRIBE_MODEL);
    let live_model = load_model_field(&raw, KEY_LIVE_MODEL, DEFAULT_LIVE_MODEL);
    let memo_model = load_model_field(&raw, KEY_MEMO_MODEL, DEFAULT_MEMO_MODEL);

    // `chunk_minutes` fallback độc lập: một giá trị parse được nhưng ngoài
    // `1..=MAX_CHUNK_MINUTES` (không thể xảy ra qua `save`, nhưng có thể qua
    // hàng bị sửa tay) fallback giống hệt JSON hỏng (spec Boundaries Always:
    // "chunkMinutes là số nguyên trong 1..=60").
    let chunk_minutes = match raw.get(KEY_CHUNK_MINUTES) {
        None => DEFAULT_CHUNK_MINUTES,
        Some(value) => match serde_json::from_str::<u32>(value) {
            Ok(parsed) if (1..=MAX_CHUNK_MINUTES).contains(&parsed) => parsed,
            _ => {
                tracing::warn!(
                    key = KEY_CHUNK_MINUTES,
                    "giá trị settings không parse được, dùng mặc định"
                );
                DEFAULT_CHUNK_MINUTES
            }
        },
    };

    // `u32` đã tự loại âm ở kiểu -- không cần kiểm ngưỡng thêm, chỉ fallback
    // khi JSON hỏng.
    let timestamp_offset_sec = match raw.get(KEY_TIMESTAMP_OFFSET_SEC) {
        None => 0,
        Some(value) => serde_json::from_str::<u32>(value).unwrap_or_else(|_| {
            tracing::warn!(
                key = KEY_TIMESTAMP_OFFSET_SEC,
                "giá trị settings không parse được, dùng mặc định"
            );
            0
        }),
    };

    let transcribe_language = match raw.get(KEY_TRANSCRIBE_LANGUAGE) {
        None => TranscribeLanguage::default(),
        Some(value) => serde_json::from_str::<TranscribeLanguage>(value).unwrap_or_else(|_| {
            tracing::warn!(
                key = KEY_TRANSCRIBE_LANGUAGE,
                "giá trị settings không parse được, dùng mặc định"
            );
            TranscribeLanguage::default()
        }),
    };

    let live_target = match raw.get(KEY_LIVE_TARGET) {
        None => LiveTarget::default(),
        Some(value) => serde_json::from_str::<LiveTarget>(value).unwrap_or_else(|_| {
            tracing::warn!(
                key = KEY_LIVE_TARGET,
                "giá trị settings không parse được, dùng mặc định"
            );
            LiveTarget::default()
        }),
    };

    Settings {
        theme,
        ui_language,
        onboarding_completed,
        consent_accepted_version,
        consent_declined,
        transcribe_model,
        live_model,
        memo_model,
        chunk_minutes,
        timestamp_offset_sec,
        transcribe_language,
        live_target,
    }
}

/// Từ chối một trường model rỗng hoặc chỉ khoảng trắng trước khi ghi (spec
/// Always: "Rust `save` cũng từ chối rỗng (`format`)") — kiểm tra độc lập
/// với validation inline ở frontend, không tin tưởng một mình phía UI.
fn require_non_blank_model(field: &str, value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::new(
            Code::Format,
            format!("{field} không được để trống"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_recommended_model(field: &str, value: &str) -> Result<String, AppError> {
    require_non_blank_model(field, value)?;
    Ok(crate::core::model_defaults::bare_model_name(value).to_string())
}

/// Từ chối `chunkMinutes` ngoài `1..=MAX_CHUNK_MINUTES` trước khi ghi (spec
/// Boundaries Always P0 review: "chunkMinutes là số nguyên trong 1..=60 ...
/// Rust `settings::save` từ chối các giá trị ngoài khoảng (`Code::Format`)").
/// `timestampOffsetSec` không cần kiểm tương tự -- kiểu `u32` đã loại âm.
pub(crate) fn require_min_chunk_minutes(value: u32) -> Result<(), AppError> {
    if !(1..=MAX_CHUNK_MINUTES).contains(&value) {
        return Err(AppError::new(
            Code::Format,
            format!("chunkMinutes phải là số nguyên từ 1 đến {MAX_CHUNK_MINUTES}"),
        ));
    }
    Ok(())
}

/// Ghi toàn bộ settings trong một transaction (spec Boundaries). Lỗi ghi trả
/// `AppError` category `storage`; người gọi (ipc) chỉ phát `SettingsChanged`
/// khi hàm này trả `Ok`.
pub fn save(db: &Db, settings: &Settings) -> Result<(), AppError> {
    let mut normalized = settings.clone();
    for model in [
        &mut normalized.transcribe_model,
        &mut normalized.live_model,
        &mut normalized.memo_model,
    ] {
        *model = crate::core::model_defaults::bare_model_name(model).to_string();
    }
    let settings = &normalized;
    require_non_blank_model(KEY_TRANSCRIBE_MODEL, &settings.transcribe_model)?;
    require_non_blank_model(KEY_LIVE_MODEL, &settings.live_model)?;
    require_non_blank_model(KEY_MEMO_MODEL, &settings.memo_model)?;
    require_min_chunk_minutes(settings.chunk_minutes)?;

    let theme_json = serde_json::to_string(&settings.theme)
        .map_err(|err| AppError::new(Code::Format, err.to_string()))?;
    let ui_language_json = serde_json::to_string(&settings.ui_language)
        .map_err(|err| AppError::new(Code::Format, err.to_string()))?;
    let onboarding_completed_json = serde_json::to_string(&settings.onboarding_completed)
        .map_err(|err| AppError::new(Code::Format, err.to_string()))?;
    let consent_accepted_version_json =
        serde_json::to_string(&settings.consent_accepted_version)
            .map_err(|err| AppError::new(Code::Format, err.to_string()))?;
    let consent_declined_json = serde_json::to_string(&settings.consent_declined)
        .map_err(|err| AppError::new(Code::Format, err.to_string()))?;
    let transcribe_model_json = serde_json::to_string(&settings.transcribe_model)
        .map_err(|err| AppError::new(Code::Format, err.to_string()))?;
    let live_model_json = serde_json::to_string(&settings.live_model)
        .map_err(|err| AppError::new(Code::Format, err.to_string()))?;
    let memo_model_json = serde_json::to_string(&settings.memo_model)
        .map_err(|err| AppError::new(Code::Format, err.to_string()))?;
    let chunk_minutes_json = serde_json::to_string(&settings.chunk_minutes)
        .map_err(|err| AppError::new(Code::Format, err.to_string()))?;
    let timestamp_offset_sec_json = serde_json::to_string(&settings.timestamp_offset_sec)
        .map_err(|err| AppError::new(Code::Format, err.to_string()))?;
    let transcribe_language_json = serde_json::to_string(&settings.transcribe_language)
        .map_err(|err| AppError::new(Code::Format, err.to_string()))?;

    let live_target_json = serde_json::to_string(&settings.live_target)
        .map_err(|err| AppError::new(Code::Format, err.to_string()))?;

    db.with_connection(|conn| {
        Ok(repo::settings::upsert_many(
            conn,
            &[
                (KEY_THEME, theme_json),
                (KEY_UI_LANGUAGE, ui_language_json),
                (KEY_ONBOARDING_COMPLETED, onboarding_completed_json),
                (KEY_CONSENT_ACCEPTED_VERSION, consent_accepted_version_json),
                (KEY_CONSENT_DECLINED, consent_declined_json),
                (KEY_TRANSCRIBE_MODEL, transcribe_model_json),
                (KEY_LIVE_MODEL, live_model_json),
                (KEY_MEMO_MODEL, memo_model_json),
                (KEY_CHUNK_MINUTES, chunk_minutes_json),
                (KEY_TIMESTAMP_OFFSET_SEC, timestamp_offset_sec_json),
                (KEY_TRANSCRIBE_LANGUAGE, transcribe_language_json),
                (KEY_LIVE_TARGET, live_target_json),
            ],
        )?)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::error::Category;
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

    fn default_settings_with_theme(theme: Theme) -> Settings {
        Settings {
            theme,
            ..Default::default()
        }
    }

    #[test]
    fn load_on_empty_table_returns_defaults() {
        let db = open_db();
        assert_eq!(load(&db), default_settings_with_theme(Theme::System));
    }

    #[test]
    fn save_then_load_round_trips() {
        let db = open_db();
        let expected = Settings {
            theme: Theme::Dark,
            ui_language: UiLanguage::Ja,
            onboarding_completed: true,
            ..Default::default()
        };
        save(&db, &expected).unwrap();
        assert_eq!(load(&db), expected);
    }

    #[test]
    fn save_stores_bare_model_names_without_the_models_resource_prefix() {
        let db = open_db();
        let settings = Settings {
            transcribe_model: "models/gemini-3-flash".to_string(),
            live_model: "models/gemini-live-x".to_string(),
            memo_model: "gemini-plain".to_string(),
            ..Default::default()
        };
        save(&db, &settings).unwrap();
        let loaded = load(&db);
        assert_eq!(loaded.transcribe_model, "gemini-3-flash");
        assert_eq!(loaded.live_model, "gemini-live-x");
        assert_eq!(loaded.memo_model, "gemini-plain");
    }

    #[test]
    fn load_strips_the_prefix_from_values_saved_before_names_were_normalized() {
        let db = open_db();
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('transcribeModel', '\"models/gemini-old\"')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        assert_eq!(load(&db).transcribe_model, "gemini-old");
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

        assert_eq!(load(&db), default_settings_with_theme(Theme::System));
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

        assert_eq!(settings, default_settings_with_theme(Theme::System));

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

    #[test]
    fn ui_language_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&UiLanguage::System).unwrap(),
            "\"system\""
        );
        assert_eq!(serde_json::to_string(&UiLanguage::Vi).unwrap(), "\"vi\"");
        assert_eq!(serde_json::to_string(&UiLanguage::En).unwrap(), "\"en\"");
        assert_eq!(serde_json::to_string(&UiLanguage::Ja).unwrap(), "\"ja\"");
    }

    #[test]
    fn corrupt_language_falls_back_without_losing_other_fields() {
        let db = open_db();
        save(
            &db,
            &Settings {
                theme: Theme::Dark,
                ui_language: UiLanguage::Vi,
                onboarding_completed: true,
                ..Default::default()
            },
        )
        .unwrap();
        db.with_connection(|conn| {
            conn.execute(
                "UPDATE settings SET value = 'not-json' WHERE key = 'uiLanguage'",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        assert_eq!(
            load(&db),
            Settings {
                theme: Theme::Dark,
                ui_language: UiLanguage::System,
                onboarding_completed: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn legacy_rows_default_missing_language_and_onboarding_only() {
        let db = open_db();
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('theme', '\"dark\"')",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        assert_eq!(
            load(&db),
            Settings {
                theme: Theme::Dark,
                ui_language: UiLanguage::System,
                onboarding_completed: false,
                ..Default::default()
            }
        );
    }

    #[test]
    fn corrupt_onboarding_falls_back_without_losing_theme_or_language() {
        let db = open_db();
        save(
            &db,
            &Settings {
                theme: Theme::Light,
                ui_language: UiLanguage::Ja,
                onboarding_completed: true,
                ..Default::default()
            },
        )
        .unwrap();
        db.with_connection(|conn| {
            conn.execute(
                "UPDATE settings SET value = 'not-json' WHERE key = 'onboardingCompleted'",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        assert_eq!(
            load(&db),
            Settings {
                theme: Theme::Light,
                ui_language: UiLanguage::Ja,
                onboarding_completed: false,
                ..Default::default()
            }
        );
    }

    #[test]
    fn consent_round_trip_is_independent_from_completion() {
        let db = open_db();
        let expected = Settings {
            theme: Theme::Dark,
            ui_language: UiLanguage::En,
            onboarding_completed: false,
            consent_accepted_version: 1,
            consent_declined: false,
            ..Default::default()
        };

        save(&db, &expected).unwrap();

        assert_eq!(load(&db), expected);
    }

    #[test]
    fn corrupt_consent_fields_fall_back_without_losing_other_fields() {
        let db = open_db();
        save(
            &db,
            &Settings {
                theme: Theme::Light,
                ui_language: UiLanguage::Ja,
                onboarding_completed: true,
                consent_accepted_version: 1,
                consent_declined: true,
                ..Default::default()
            },
        )
        .unwrap();
        db.with_connection(|conn| {
            conn.execute(
                "UPDATE settings SET value = 'not-json' WHERE key = 'consentAcceptedVersion'",
                [],
            )?;
            conn.execute(
                "UPDATE settings SET value = 'not-json' WHERE key = 'consentDeclined'",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        assert_eq!(
            load(&db),
            Settings {
                theme: Theme::Light,
                ui_language: UiLanguage::Ja,
                onboarding_completed: true,
                consent_accepted_version: 0,
                consent_declined: false,
                ..Default::default()
            }
        );
    }

    #[test]
    fn legacy_rows_default_missing_consent_without_changing_completion() {
        let db = open_db();
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('onboardingCompleted', 'true')",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        assert_eq!(
            load(&db),
            Settings {
                onboarding_completed: true,
                ..Default::default()
            }
        );
    }

    // Story 1.9 I/O Matrix "Settings cũ": DB thiếu khoá model -> load trả
    // mặc định `gemini::params`/`core::model_defaults`, không lỗi.
    #[test]
    fn default_settings_use_gemini_model_defaults() {
        let defaults = Settings::default();
        assert_eq!(defaults.transcribe_model, DEFAULT_TRANSCRIBE_MODEL);
        assert_eq!(defaults.live_model, DEFAULT_LIVE_MODEL);
        assert_eq!(defaults.memo_model, DEFAULT_MEMO_MODEL);
    }

    #[test]
    fn load_on_empty_table_defaults_model_fields() {
        let db = open_db();
        let loaded = load(&db);
        assert_eq!(loaded.transcribe_model, DEFAULT_TRANSCRIBE_MODEL);
        assert_eq!(loaded.live_model, DEFAULT_LIVE_MODEL);
        assert_eq!(loaded.memo_model, DEFAULT_MEMO_MODEL);
    }

    #[test]
    fn model_fields_round_trip_a_custom_name_not_in_any_loaded_list() {
        let db = open_db();
        let expected = Settings {
            transcribe_model: "my-custom-model".to_string(),
            live_model: "another-custom-model".to_string(),
            memo_model: "third-custom-model".to_string(),
            ..Default::default()
        };

        save(&db, &expected).unwrap();

        assert_eq!(load(&db), expected);
    }

    #[test]
    fn save_rejects_empty_or_whitespace_only_transcribe_model() {
        let db = open_db();
        let err = save(
            &db,
            &Settings {
                transcribe_model: "   ".to_string(),
                ..Default::default()
            },
        )
        .expect_err("model rỗng (hoặc chỉ khoảng trắng) phải bị từ chối");

        assert_eq!(err.category, Category::Format);
    }

    #[test]
    fn save_rejects_empty_live_model() {
        let db = open_db();
        let err = save(
            &db,
            &Settings {
                live_model: String::new(),
                ..Default::default()
            },
        )
        .expect_err("liveModel rỗng phải bị từ chối");

        assert_eq!(err.category, Category::Format);
    }

    #[test]
    fn save_rejects_empty_memo_model() {
        let db = open_db();
        let err = save(
            &db,
            &Settings {
                memo_model: String::new(),
                ..Default::default()
            },
        )
        .expect_err("memoModel rỗng phải bị từ chối");

        assert_eq!(err.category, Category::Format);
    }

    #[test]
    fn save_rejecting_an_empty_model_does_not_write_any_row() {
        let db = open_db();
        // A prior valid save must not be overwritten by a rejected one (spec
        // Boundaries parity with existing settings: reject before touching
        // the DB at all).
        save(&db, &Settings::default()).unwrap();

        let err = save(
            &db,
            &Settings {
                transcribe_model: String::new(),
                ..Default::default()
            },
        )
        .expect_err("phải từ chối trước khi ghi");
        assert_eq!(err.category, Category::Format);

        assert_eq!(load(&db), Settings::default());
    }

    #[test]
    fn corrupt_model_fields_fall_back_without_losing_other_fields() {
        let db = open_db();
        save(
            &db,
            &Settings {
                theme: Theme::Dark,
                transcribe_model: "custom-transcribe".to_string(),
                live_model: "custom-live".to_string(),
                memo_model: "custom-memo".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        db.with_connection(|conn| {
            conn.execute(
                "UPDATE settings SET value = 'not-json' WHERE key = 'transcribeModel'",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        assert_eq!(
            load(&db),
            Settings {
                theme: Theme::Dark,
                transcribe_model: DEFAULT_TRANSCRIBE_MODEL.to_string(),
                live_model: "custom-live".to_string(),
                memo_model: "custom-memo".to_string(),
                ..Default::default()
            }
        );
    }

    #[test]
    fn blank_model_row_falls_back_to_default_like_corrupt_json() {
        // A row that parses fine as JSON but is blank after trim (e.g. a
        // hand-edited DB) must fall back the same way as a parse failure —
        // `save` can never write it, but `load` must still be defensive.
        let db = open_db();
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('liveModel', '\"   \"')",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        assert_eq!(load(&db).live_model, DEFAULT_LIVE_MODEL);
    }

    #[test]
    fn legacy_rows_default_missing_model_fields_without_changing_others() {
        let db = open_db();
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('theme', '\"dark\"')",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        let loaded = load(&db);
        assert_eq!(loaded.theme, Theme::Dark);
        assert_eq!(loaded.transcribe_model, DEFAULT_TRANSCRIBE_MODEL);
        assert_eq!(loaded.live_model, DEFAULT_LIVE_MODEL);
        assert_eq!(loaded.memo_model, DEFAULT_MEMO_MODEL);
    }

    // Story 2.6: `chunkMinutes`/`timestampOffsetSec`/`transcribeLanguage`.

    #[test]
    fn default_settings_use_five_minute_chunks_zero_offset_and_auto_language() {
        let defaults = Settings::default();
        assert_eq!(defaults.chunk_minutes, 5);
        assert_eq!(defaults.timestamp_offset_sec, 0);
        assert_eq!(defaults.transcribe_language, TranscribeLanguage::Auto);
    }

    #[test]
    fn transcribe_language_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&TranscribeLanguage::Auto).unwrap(),
            "\"auto\""
        );
        assert_eq!(
            serde_json::to_string(&TranscribeLanguage::Ja).unwrap(),
            "\"ja\""
        );
        assert_eq!(
            serde_json::to_string(&TranscribeLanguage::Vi).unwrap(),
            "\"vi\""
        );
        assert_eq!(
            serde_json::to_string(&TranscribeLanguage::En).unwrap(),
            "\"en\""
        );
    }

    #[test]
    fn transcribe_language_as_code_is_null_only_for_auto() {
        assert_eq!(TranscribeLanguage::Auto.as_code(), None);
        assert_eq!(TranscribeLanguage::Ja.as_code(), Some("ja"));
        assert_eq!(TranscribeLanguage::Vi.as_code(), Some("vi"));
        assert_eq!(TranscribeLanguage::En.as_code(), Some("en"));
    }

    #[test]
    fn chunking_fields_round_trip() {
        let db = open_db();
        let expected = Settings {
            chunk_minutes: 3,
            timestamp_offset_sec: 3_600,
            transcribe_language: TranscribeLanguage::Ja,
            ..Default::default()
        };

        save(&db, &expected).unwrap();

        assert_eq!(load(&db), expected);
    }

    #[test]
    fn save_rejects_zero_chunk_minutes() {
        let db = open_db();
        let err = save(
            &db,
            &Settings {
                chunk_minutes: 0,
                ..Default::default()
            },
        )
        .expect_err("chunkMinutes = 0 phải bị từ chối");

        assert_eq!(err.category, Category::Format);
    }

    #[test]
    fn save_rejecting_zero_chunk_minutes_does_not_write_any_row() {
        let db = open_db();
        let previously_saved = Settings {
            chunk_minutes: 7,
            ..Default::default()
        };
        save(&db, &previously_saved).unwrap();

        let err = save(
            &db,
            &Settings {
                chunk_minutes: 0,
                ..Default::default()
            },
        )
        .expect_err("phải từ chối trước khi ghi");
        assert_eq!(err.category, Category::Format);

        assert_eq!(load(&db), previously_saved);
    }

    #[test]
    fn corrupt_chunk_minutes_falls_back_without_losing_other_fields() {
        let db = open_db();
        save(
            &db,
            &Settings {
                theme: Theme::Dark,
                chunk_minutes: 12,
                timestamp_offset_sec: 42,
                transcribe_language: TranscribeLanguage::Vi,
                ..Default::default()
            },
        )
        .unwrap();
        db.with_connection(|conn| {
            conn.execute(
                "UPDATE settings SET value = 'not-json' WHERE key = 'chunkMinutes'",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        assert_eq!(
            load(&db),
            Settings {
                theme: Theme::Dark,
                chunk_minutes: 5,
                timestamp_offset_sec: 42,
                transcribe_language: TranscribeLanguage::Vi,
                ..Default::default()
            }
        );
    }

    /// A hand-edited row of `0` parses fine as a `u32` but is below the
    /// `>= 1` floor -- `load` must fall back exactly like a parse failure
    /// (spec Boundaries: "`chunkMinutes` là số nguyên >= 1"), since `save`
    /// can never itself write such a row.
    #[test]
    fn zero_chunk_minutes_row_falls_back_to_default_like_corrupt_json() {
        let db = open_db();
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('chunkMinutes', '0')",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        assert_eq!(load(&db).chunk_minutes, 5);
    }

    // P0 review fix: `chunkMinutes` now has an upper bound too, closing the
    // hole where a huge stored/typed value reached `Chunker::new` and could
    // abort the process via an oversized `Vec::with_capacity` (spec I/O
    // Matrix "Save chunk 61" / "Load corrupt chunk" / "Save chunk 60").

    #[test]
    fn save_rejects_chunk_minutes_above_max() {
        let db = open_db();
        let err = save(
            &db,
            &Settings {
                chunk_minutes: 61,
                ..Default::default()
            },
        )
        .expect_err("chunkMinutes = 61 phải bị từ chối");

        assert_eq!(err.category, Category::Format);
    }

    #[test]
    fn save_rejecting_chunk_minutes_above_max_does_not_write_any_row() {
        let db = open_db();
        let previously_saved = Settings {
            chunk_minutes: 7,
            ..Default::default()
        };
        save(&db, &previously_saved).unwrap();

        let err = save(
            &db,
            &Settings {
                chunk_minutes: 61,
                ..Default::default()
            },
        )
        .expect_err("phải từ chối trước khi ghi");
        assert_eq!(err.category, Category::Format);

        assert_eq!(load(&db), previously_saved);
    }

    #[test]
    fn save_accepts_max_chunk_minutes() {
        let db = open_db();
        let expected = Settings {
            chunk_minutes: 60,
            ..Default::default()
        };

        save(&db, &expected).unwrap();

        assert_eq!(load(&db), expected);
    }

    /// A hand-edited row of `5000` parses fine as a `u32` but is above the
    /// new ceiling -- `load` must fall back exactly like a parse failure or a
    /// too-small value (spec I/O Matrix "Load corrupt chunk: DB row 5000 ->
    /// chunk_minutes = 5").
    #[test]
    fn oversized_chunk_minutes_row_falls_back_to_default_like_corrupt_json() {
        let db = open_db();
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('chunkMinutes', '5000')",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        assert_eq!(load(&db).chunk_minutes, 5);
    }

    #[test]
    fn corrupt_timestamp_offset_sec_falls_back_without_losing_other_fields() {
        let db = open_db();
        save(
            &db,
            &Settings {
                timestamp_offset_sec: 120,
                transcribe_language: TranscribeLanguage::En,
                ..Default::default()
            },
        )
        .unwrap();
        db.with_connection(|conn| {
            conn.execute(
                "UPDATE settings SET value = 'not-json' WHERE key = 'timestampOffsetSec'",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        assert_eq!(
            load(&db),
            Settings {
                timestamp_offset_sec: 0,
                transcribe_language: TranscribeLanguage::En,
                ..Default::default()
            }
        );
    }

    // Story 5.1: `liveTarget`.
    #[test]
    fn live_target_defaults_to_none_and_round_trips() {
        let db = open_db();
        assert_eq!(load(&db).live_target, LiveTarget::None);
        for target in [
            LiveTarget::Vi,
            LiveTarget::En,
            LiveTarget::Ja,
            LiveTarget::None,
        ] {
            save(
                &db,
                &Settings {
                    live_target: target,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(load(&db).live_target, target);
        }
    }

    #[test]
    fn live_target_serializes_lowercase_and_maps_to_codes() {
        assert_eq!(
            serde_json::to_string(&LiveTarget::None).unwrap(),
            "\"none\""
        );
        assert_eq!(serde_json::to_string(&LiveTarget::Vi).unwrap(), "\"vi\"");
        assert_eq!(LiveTarget::None.as_code(), None);
        assert_eq!(LiveTarget::Ja.as_code(), Some("ja"));
        assert_eq!(LiveTarget::En.as_code(), Some("en"));
    }

    #[test]
    fn corrupt_live_target_falls_back_without_losing_other_fields() {
        let db = open_db();
        save(
            &db,
            &Settings {
                live_target: LiveTarget::Vi,
                transcribe_language: TranscribeLanguage::Ja,
                ..Default::default()
            },
        )
        .unwrap();
        db.with_connection(|conn| {
            conn.execute(
                "UPDATE settings SET value = '\"auto\"' WHERE key = 'liveTarget'",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        let loaded = load(&db);
        assert_eq!(loaded.live_target, LiveTarget::None);
        assert_eq!(loaded.transcribe_language, TranscribeLanguage::Ja);
    }

    #[test]
    fn corrupt_transcribe_language_falls_back_without_losing_other_fields() {
        let db = open_db();
        save(
            &db,
            &Settings {
                chunk_minutes: 9,
                transcribe_language: TranscribeLanguage::Ja,
                ..Default::default()
            },
        )
        .unwrap();
        db.with_connection(|conn| {
            conn.execute(
                "UPDATE settings SET value = 'not-json' WHERE key = 'transcribeLanguage'",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        assert_eq!(
            load(&db),
            Settings {
                chunk_minutes: 9,
                transcribe_language: TranscribeLanguage::Auto,
                ..Default::default()
            }
        );
    }

    /// An unknown language string (e.g. a future value from a newer build,
    /// or a hand-edited row) must fall back like any other corrupt value,
    /// never panic or propagate an error (spec I/O Matrix "Khoá hỏng").
    #[test]
    fn unknown_transcribe_language_string_falls_back_to_auto() {
        let db = open_db();
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('transcribeLanguage', '\"ko\"')",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        assert_eq!(load(&db).transcribe_language, TranscribeLanguage::Auto);
    }

    #[test]
    fn legacy_rows_default_missing_chunking_fields_without_changing_others() {
        let db = open_db();
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('theme', '\"dark\"')",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        let loaded = load(&db);
        assert_eq!(loaded.theme, Theme::Dark);
        assert_eq!(loaded.chunk_minutes, 5);
        assert_eq!(loaded.timestamp_offset_sec, 0);
        assert_eq!(loaded.transcribe_language, TranscribeLanguage::Auto);
    }
}
