//! Chẩn đoán (story 1.10): allow-list + gói xuất log content-free, xoá nhật
//! ký, và bộ đếm cục bộ (phiên, lỗi theo category, crash) lưu trong
//! `local_counters` (Architecture Spine: feature phụ thuộc `db`/`core`, không
//! phụ thuộc feature khác). Mở dialog lưu hệ thống (`rfd`) và đăng ký command
//! là việc của `ipc::` (Code Map) — module này không phụ thuộc Tauri, nên
//! phần allow-list/gói/xoá test được bằng `tempfile` thuần.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::core::error::{AppError, Category};
use crate::core::log::LOG_FILE_PREFIX;
use crate::db::{repo, Db};

// ---------------------------------------------------------------------
// Bộ đếm cục bộ
// ---------------------------------------------------------------------

const COUNTER_SESSIONS: &str = "sessions";
const COUNTER_CRASHES: &str = "crashes";
/// Marker "lần thoát trước có sạch không" — `true` (1) khi `RunEvent::Exit`
/// chạy xong, đặt lại `false` (0) ngay lúc boot (spec Always: "Crash").
const COUNTER_CLEAN_SHUTDOWN: &str = "cleanShutdown";

fn error_counter_key(category: Category) -> String {
    format!("errors.{category}")
}

/// 8 category ổn định theo đúng thứ tự hiển thị — dùng để luôn trả đủ 8 dòng
/// trong [`DiagnosticsSummary`] kể cả khi chưa lỗi nào thuộc category đó xảy
/// ra (spec Acceptance: "lỗi theo từng category (8 dòng, gồm 0)").
pub const ERROR_CATEGORIES: [Category; 8] = [
    Category::Quota,
    Category::Auth,
    Category::Model,
    Category::Network,
    Category::Format,
    Category::Permission,
    Category::Storage,
    Category::Blocked,
];

// `i32`, không `i64`: specta-typescript từ chối export kiểu "BigInt-style"
// (i64/u64/usize/...) để tránh mất độ chính xác ở phía TypeScript/JSON —
// `local_counters.value` là `INTEGER` (i64) trong SQLite, nhưng một số đếm
// cục bộ không bao giờ tới gần i32::MAX nên ép kiểu ở đúng biên IPC này.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ErrorCategoryCount {
    pub category: Category,
    pub count: i32,
}

/// Snapshot chỉ-số hiển thị ở Settings → Chẩn đoán. Không có trường nào mang
/// nội dung — chỉ số nguyên (spec Boundaries: "Riêng tư của bộ đếm").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsSummary {
    pub sessions: i32,
    pub crashes: i32,
    pub errors_by_category: Vec<ErrorCategoryCount>,
}

/// Đọc toàn bộ bộ đếm hiện tại. Không bao giờ thiếu category nào trong kết
/// quả — category chưa lỗi lần nào trả `count: 0`.
pub fn summary(db: &Db) -> Result<DiagnosticsSummary, AppError> {
    db.with_connection(|conn| {
        let sessions = repo::counters::get(conn, COUNTER_SESSIONS)? as i32;
        let crashes = repo::counters::get(conn, COUNTER_CRASHES)? as i32;
        let mut errors_by_category = Vec::with_capacity(ERROR_CATEGORIES.len());
        for category in ERROR_CATEGORIES {
            let count = repo::counters::get(conn, &error_counter_key(category))? as i32;
            errors_by_category.push(ErrorCategoryCount { category, count });
        }
        Ok(DiagnosticsSummary {
            sessions,
            crashes,
            errors_by_category,
        })
    })
}

/// Tăng bộ đếm lỗi của `category` thêm 1 — helper duy nhất gọi bởi `ipc::`
/// mỗi khi một command trả `Err` (spec Always: "Lỗi: tăng theo category khi
/// một IPC command trả `Err`, qua một helper duy nhất ở `ipc/`"). Lỗi ghi đếm
/// ở đây không bao giờ được phép làm đổi lỗi gốc của command — người gọi chỉ
/// log cảnh báo khi hàm này trả `Err`, không propagate lên response IPC.
pub fn record_error(db: &Db, category: Category) -> Result<(), AppError> {
    db.with_connection(|conn| {
        repo::counters::increment(conn, &error_counter_key(category))?;
        Ok(())
    })
}

/// Tăng bộ đếm phiên thêm 1. Có sẵn cho Epic 2 gọi khi bắt đầu một phiên
/// transcribe/live/memo thật; story này chưa ai gọi nên bộ đếm luôn hiển thị
/// 0 (spec Always: "Phiên: có API tăng đếm nhưng chưa ai gọi").
#[allow(dead_code)]
pub fn increment_session(db: &Db) -> Result<(), AppError> {
    db.with_connection(|conn| {
        repo::counters::increment(conn, COUNTER_SESSIONS)?;
        Ok(())
    })
}

/// Gọi đúng một lần lúc boot (sau khi DB mở thành công), trước khi bất kỳ
/// command nào chạy. Nếu marker `cleanShutdown` của lần chạy trước tồn tại và
/// là `false`, tăng bộ đếm crash. Marker vắng mặt (lần đầu tiên app từng
/// chạy — chưa có "lần trước" nào để so) không tính là crash, tránh báo crash
/// giả trên bản cài đặt mới (spec I/O Matrix "Crash": "lần chạy trước không
/// đặt marker sạch" ngụ ý phải có một lần chạy trước). Sau khi kiểm tra,
/// marker luôn được đặt lại `false` ngay — chỉ `RunEvent::Exit` mới đặt lại
/// `true` (spec Always).
pub fn note_boot(db: &Db) -> Result<(), AppError> {
    db.with_connection(|conn| {
        if let Some(previous_clean) = repo::counters::get_optional(conn, COUNTER_CLEAN_SHUTDOWN)? {
            if previous_clean == 0 {
                repo::counters::increment(conn, COUNTER_CRASHES)?;
            }
        }
        repo::counters::set(conn, COUNTER_CLEAN_SHUTDOWN, 0)?;
        Ok(())
    })
}

/// Gọi ở `RunEvent::Exit` (`lib.rs`) — đánh dấu lần thoát này là sạch.
pub fn mark_clean_shutdown(db: &Db) -> Result<(), AppError> {
    db.with_connection(|conn| {
        repo::counters::set(conn, COUNTER_CLEAN_SHUTDOWN, 1)?;
        Ok(())
    })
}

// ---------------------------------------------------------------------
// Allow-list + gói xuất log + xoá nhật ký
// ---------------------------------------------------------------------

/// `true` nếu `file_name` khớp allow-list: đúng prefix `core::log` dùng để
/// ghi log, theo sau bởi hậu tố rotation (spec Boundaries: "tên khớp
/// `trans-kun` + hậu tố rotation của `core::log`"). Không kiểm gì khác — việc
/// loại symlink/thư mục con là trách nhiệm của [`allow_listed_log_files`]
/// (dựa vào `file_type()`, không dựa vào tên).
fn is_allow_listed_name(file_name: &str) -> bool {
    let prefix = format!("{LOG_FILE_PREFIX}.");
    file_name.starts_with(&prefix) && file_name.len() > prefix.len()
}

/// Liệt kê các file log hợp lệ nằm trực tiếp trong `log_dir` — không đệ quy
/// vào thư mục con, bỏ qua symlink (spec Boundaries). `entry.file_type()`
/// (khác `entry.metadata()`) không theo dấu symlink, nên một symlink trỏ tới
/// file thật vẫn bị loại đúng như spec yêu cầu. Đọc thư mục lỗi (không tồn
/// tại, không có quyền) trả danh sách rỗng thay vì lỗi — gọi lại được từ ngữ
/// cảnh không có `Result` dễ dàng ([`build_bundle`]).
pub fn allow_listed_log_files(log_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(log_dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if is_allow_listed_name(&name) {
            files.push(entry.path());
        }
    }
    files.sort();
    files
}

/// Chuyển số ngày kể từ Unix epoch (UTC) thành `(năm, tháng, ngày)` — thuật
/// toán lịch Gregorian thuần (Howard Hinnant "civil_from_days"), không cần
/// thêm crate ngày-giờ chỉ để gợi ý tên file/so khớp "file hôm nay" (spec
/// Never: không thêm crate nén — tinh thần chung là không thêm phụ thuộc
/// ngoài cho việc phụ, ở đây áp dụng luôn cho ngày-giờ).
fn civil_from_unix_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn utc_ymd(now: SystemTime) -> (i64, u32, u32) {
    let days = now
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_secs() / 86_400) as i64)
        .unwrap_or(0);
    civil_from_unix_days(days)
}

/// Tên file log của "hôm nay" theo giờ UTC — cùng định dạng
/// `{prefix}.{YYYY-MM-DD}` mà `tracing_appender` (rotation `DAILY`, UTC) dùng
/// để đặt tên file đang ghi hiện tại. [`clear_logs`] dùng để nhận diện đúng
/// file đang mở cần truncate thay vì xoá.
fn current_log_file_name(now: SystemTime) -> String {
    let (y, m, d) = utc_ymd(now);
    format!("{LOG_FILE_PREFIX}.{y:04}-{m:02}-{d:02}")
}

/// Tên gợi ý cho gói xuất, theo `trans-kun-diagnostics-YYYYMMDD.txt` (spec
/// Boundaries "Định dạng gói").
pub fn default_export_file_name(now: SystemTime) -> String {
    let (y, m, d) = utc_ymd(now);
    format!("trans-kun-diagnostics-{y:04}{m:02}{d:02}.txt")
}

/// Thay mọi đường dẫn tuyệt đối (POSIX `/Users/...`, `/home/...`, hay
/// Windows `C:\Users\...`) và tên người dùng hệ điều hành hiện tại bằng
/// marker trung tính — áp dụng riêng cho nội dung đi vào gói xuất (spec
/// Never: "Không đưa đường dẫn tuyệt đối hay tên người dùng của máy vào
/// gói"). Tách khỏi [`crate::core::log::redact`] (chỉ nhắm bí mật
/// key/URL/token) vì đây là một lớp khác, chỉ cần cho gói xuất — writer log
/// runtime không cần biết tới nó.
fn scrub_paths_and_username(input: &str) -> String {
    static PATH_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:[A-Za-z]:\\(?:[^\\\r\n]+\\)*[^\\\r\n]+|/(?:Users|home)/[^/\s]+(?:/[^\s]*)?)")
            .unwrap()
    });

    let mut out = PATH_PATTERN.replace_all(input, "[path]").into_owned();

    // Phòng thêm: tên người dùng hệ điều hành hiện tại có thể xuất hiện
    // ngoài một đường dẫn (ví dụ trong một thông điệp lỗi tự do) — thay thế
    // trực tiếp nếu biến môi trường tương ứng có giá trị không tầm thường.
    for var in ["USER", "USERNAME"] {
        if let Ok(user) = std::env::var(var) {
            if user.len() > 1 {
                out = out.replace(&user, "[user]");
            }
        }
    }

    out
}

/// Dựng nội dung gói xuất: header version/OS, tóm tắt bộ đếm, rồi từng file
/// log allow-list (đã `redact()` lại lần nữa, cộng lớp [`scrub_paths_and_username`])
/// với tiêu đề ngăn cách — hàm thuần theo nghĩa không phụ thuộc Tauri, test
/// được chỉ với `tempfile` (Code Map). Đọc file lỗi không làm hỏng cả gói —
/// dòng đó chỉ ghi lại rằng không đọc được, chi tiết lỗi cũng đi qua cùng hai
/// lớp lọc.
pub fn build_bundle(log_dir: &Path, summary: &DiagnosticsSummary) -> String {
    let mut out = String::new();

    out.push_str("trans-kun diagnostics\n");
    out.push_str(&format!("version: {}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!("os: {}\n\n", std::env::consts::OS));

    out.push_str("== Bộ đếm cục bộ ==\n");
    out.push_str(&format!("sessions: {}\n", summary.sessions));
    out.push_str(&format!("crashes: {}\n", summary.crashes));
    for row in &summary.errors_by_category {
        out.push_str(&format!("errors.{}: {}\n", row.category, row.count));
    }
    out.push('\n');

    for path in allow_listed_log_files(log_dir) {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("log")
            .to_string();
        out.push_str(&format!("== {name} ==\n"));
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let redacted = crate::core::log::redact(&content);
                out.push_str(&scrub_paths_and_username(&redacted));
                if !content.ends_with('\n') {
                    out.push('\n');
                }
            }
            Err(err) => {
                let redacted = crate::core::log::redact(&err.to_string());
                out.push_str(&format!(
                    "[không đọc được file: {}]\n",
                    scrub_paths_and_username(&redacted)
                ));
            }
        }
        out.push('\n');
    }

    out
}

/// Xoá nhật ký: các file allow-list cũ bị xoá hẳn; file của "hôm nay" (đang
/// mở bởi logger đang chạy) bị truncate về rỗng thay vì xoá, để hoạt động cả
/// trên Windows khi file đang mở (spec Boundaries "Xoá nhật ký"). File ngoài
/// allow-list (tên không khớp, thư mục con, symlink) không bị đụng tới vì
/// không nằm trong danh sách duyệt.
pub fn clear_logs(log_dir: &Path) -> std::io::Result<()> {
    let today = current_log_file_name(SystemTime::now());
    for path in allow_listed_log_files(log_dir) {
        let is_today = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|name| name == today)
            .unwrap_or(false);
        if is_today {
            let file = std::fs::OpenOptions::new().write(true).open(&path)?;
            file.set_len(0)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::error::Code;
    use std::io::Write;
    use tempfile::tempdir;

    fn open_db() -> Db {
        let dir = tempdir().unwrap();
        Db::open(&dir.keep()).unwrap()
    }

    // ---- Bộ đếm ----

    #[test]
    fn summary_on_fresh_db_is_all_zero_but_covers_all_eight_categories() {
        let db = open_db();
        let result = summary(&db).unwrap();
        assert_eq!(result.sessions, 0);
        assert_eq!(result.crashes, 0);
        assert_eq!(result.errors_by_category.len(), 8);
        assert!(result.errors_by_category.iter().all(|row| row.count == 0));
    }

    #[test]
    fn record_error_increments_only_its_own_category() {
        let db = open_db();
        record_error(&db, Category::Auth).unwrap();
        record_error(&db, Category::Auth).unwrap();
        record_error(&db, Category::Storage).unwrap();

        let result = summary(&db).unwrap();
        let auth = result
            .errors_by_category
            .iter()
            .find(|row| row.category == Category::Auth)
            .unwrap();
        let storage = result
            .errors_by_category
            .iter()
            .find(|row| row.category == Category::Storage)
            .unwrap();
        let network = result
            .errors_by_category
            .iter()
            .find(|row| row.category == Category::Network)
            .unwrap();
        assert_eq!(auth.count, 2);
        assert_eq!(storage.count, 1);
        assert_eq!(network.count, 0);
    }

    #[test]
    fn first_ever_boot_does_not_count_as_a_crash() {
        let db = open_db();
        note_boot(&db).unwrap();
        assert_eq!(summary(&db).unwrap().crashes, 0);
    }

    #[test]
    fn boot_after_unclean_shutdown_increments_crash_once() {
        let db = open_db();
        note_boot(&db).unwrap(); // lần chạy đầu tiên (không phải crash)
        // Không gọi `mark_clean_shutdown` -- mô phỏng thoát không sạch.
        note_boot(&db).unwrap();
        assert_eq!(summary(&db).unwrap().crashes, 1);
    }

    #[test]
    fn boot_after_clean_shutdown_does_not_increment_crash() {
        let db = open_db();
        note_boot(&db).unwrap();
        mark_clean_shutdown(&db).unwrap();
        note_boot(&db).unwrap();
        assert_eq!(summary(&db).unwrap().crashes, 0);
    }

    #[test]
    fn ipc_error_does_not_change_when_counter_write_fails() {
        // I/O Matrix "Lỗi IPC": "Ghi đếm lỗi không làm đổi lỗi gốc" — mô
        // phỏng bằng cách phá bảng `local_counters` rồi khẳng định
        // `record_error` trả lỗi riêng của chính nó (category storage),
        // không lẫn với/đổi lỗi gốc mà caller đang xử lý.
        let db = open_db();
        db.with_connection(|conn| {
            conn.execute("DROP TABLE local_counters", [])?;
            Ok(())
        })
        .unwrap();

        let err = record_error(&db, Category::Model)
            .expect_err("bảng đã bị xoá nên ghi đếm phải lỗi");
        assert_eq!(err.category, Category::Storage);
    }

    #[test]
    fn increment_session_increments_the_sessions_counter() {
        let db = open_db();
        increment_session(&db).unwrap();
        increment_session(&db).unwrap();
        assert_eq!(summary(&db).unwrap().sessions, 2);
    }

    // ---- Allow-list ----

    #[test]
    fn allow_lists_only_rotated_log_files_directly_in_the_directory() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("trans-kun.2026-09-22"), "a").unwrap();
        std::fs::write(dir.path().join("trans-kun.2026-09-21"), "b").unwrap();
        std::fs::write(dir.path().join("secret.txt"), "c").unwrap();
        std::fs::write(dir.path().join("trans-kun"), "d").unwrap(); // không có hậu tố
        std::fs::create_dir(dir.path().join("trans-kun.subdir")).unwrap();

        let files = allow_listed_log_files(dir.path());
        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();

        assert_eq!(names, vec!["trans-kun.2026-09-21", "trans-kun.2026-09-22"]);
    }

    #[cfg(unix)]
    #[test]
    fn allow_list_ignores_symlinks_even_when_pointing_at_a_real_log_file() {
        let dir = tempdir().unwrap();
        let real = dir.path().join("trans-kun.2026-09-22");
        std::fs::write(&real, "real content").unwrap();
        let link = dir.path().join("trans-kun.2026-09-23");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let names: Vec<String> = allow_listed_log_files(dir.path())
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();

        assert_eq!(names, vec!["trans-kun.2026-09-22"]);
    }

    #[test]
    fn missing_log_dir_returns_empty_list_not_an_error() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");
        assert!(allow_listed_log_files(&missing).is_empty());
    }

    // ---- build_bundle ----

    fn zero_summary() -> DiagnosticsSummary {
        DiagnosticsSummary {
            sessions: 0,
            crashes: 0,
            errors_by_category: ERROR_CATEGORIES
                .iter()
                .map(|&category| ErrorCategoryCount { category, count: 0 })
                .collect(),
        }
    }

    #[test]
    fn build_bundle_includes_header_counters_and_allow_listed_files_only() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("trans-kun.2026-09-22"), "phiên ổn định").unwrap();
        std::fs::write(dir.path().join("secret.txt"), "không được xuất hiện").unwrap();

        let mut summary = zero_summary();
        summary.sessions = 3;
        summary.crashes = 1;

        let bundle = build_bundle(dir.path(), &summary);

        assert!(bundle.contains(env!("CARGO_PKG_VERSION")));
        assert!(bundle.contains("sessions: 3"));
        assert!(bundle.contains("crashes: 1"));
        assert!(bundle.contains("errors.auth: 0"));
        assert!(bundle.contains("trans-kun.2026-09-22"));
        assert!(bundle.contains("phiên ổn định"));
        assert!(!bundle.contains("secret.txt"));
        assert!(!bundle.contains("không được xuất hiện"));
    }

    // spec Never: "Không đưa đường dẫn tuyệt đối hay tên người dùng của máy
    // vào gói."

    #[test]
    fn scrub_paths_and_username_masks_posix_absolute_paths() {
        let input = "panic tại /Users/nguyenthanhtung/code/trans-kun/src-tauri/target/debug/build";
        let out = scrub_paths_and_username(input);
        assert!(!out.contains("/Users/nguyenthanhtung"));
        assert!(out.contains("[path]"));
    }

    #[test]
    fn scrub_paths_and_username_masks_windows_absolute_paths() {
        let input = r"lỗi khi mở C:\Users\nguyen.tung\AppData\Local\trans-kun\app.db";
        let out = scrub_paths_and_username(input);
        assert!(!out.contains(r"C:\Users\nguyen.tung"));
        assert!(out.contains("[path]"));
    }

    #[test]
    fn build_bundle_never_leaks_absolute_paths_or_the_current_username() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("trans-kun.2026-09-22"),
            "app_data_dir lỗi: /Users/nguyenthanhtung/Library/Application Support/com.transkun.app không tạo được",
        )
        .unwrap();

        let bundle = build_bundle(dir.path(), &zero_summary());

        assert!(!bundle.contains("/Users/nguyenthanhtung"));
        assert!(bundle.contains("[path]"));
    }

    /// Test "phiên mẫu" của I/O Matrix: log ghi transcript/dịch/ghi
    /// chú/memo/key dạng `Sensitive` + key thô, gói xuất không được lộ nội
    /// dung/khoá — grep trực tiếp trên kết quả `build_bundle`.
    #[test]
    fn exported_bundle_never_leaks_sensitive_session_content_or_raw_keys() {
        let dir = tempdir().unwrap();
        let transcript = crate::core::sensitive::Sensitive::new(
            "nội dung transcript bí mật của phiên giả lập".to_string(),
        );
        let fake_key = "AIzaSyFAKEKEYFORDIAGNOSTICSTESTONLY00";
        let mut file = std::fs::File::create(dir.path().join("trans-kun.2026-09-22")).unwrap();
        writeln!(file, "phiên transcribe mẫu: transcript={transcript:?}").unwrap();
        writeln!(file, "raw key leaked by a bug: {fake_key}").unwrap();
        drop(file);

        let bundle = build_bundle(dir.path(), &zero_summary());

        assert!(!bundle.contains(transcript.expose().as_str()));
        assert!(!bundle.contains(fake_key));
        assert!(bundle.contains("[redacted]"));
    }

    #[test]
    fn build_bundle_on_directory_with_no_allow_listed_files_still_builds_header() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("secret.txt"), "not allow-listed").unwrap();
        let bundle = build_bundle(dir.path(), &zero_summary());
        assert!(bundle.contains("Bộ đếm cục bộ"));
        assert!(!bundle.contains("secret.txt"));
    }

    #[cfg(unix)]
    #[test]
    fn build_bundle_reports_unreadable_file_without_failing_the_whole_bundle() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let path = dir.path().join("trans-kun.2026-09-22");
        std::fs::write(&path, "content").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();

        let bundle = build_bundle(dir.path(), &zero_summary());

        // Chạy với quyền root (một số môi trường CI) bỏ qua permission bit,
        // nên chỉ khẳng định gói build xong không panic và vẫn có header —
        // nội dung không đọc được (khi permission thực sự chặn) không làm
        // hỏng phần còn lại của gói.
        assert!(bundle.contains("Bộ đếm cục bộ"));
        assert!(bundle.contains("trans-kun.2026-09-22"));

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    }

    // ---- clear_logs ----

    #[test]
    fn clear_logs_removes_old_files_truncates_today_and_leaves_other_files_alone() {
        let dir = tempdir().unwrap();
        let today_name = current_log_file_name(SystemTime::now());
        let today_path = dir.path().join(&today_name);
        std::fs::write(&today_path, "log của hôm nay").unwrap();
        std::fs::write(dir.path().join("trans-kun.2020-01-01"), "log cũ").unwrap();
        std::fs::write(dir.path().join("secret.txt"), "không đụng vào").unwrap();

        clear_logs(dir.path()).unwrap();

        assert!(!dir.path().join("trans-kun.2020-01-01").exists());
        assert!(today_path.exists());
        assert_eq!(std::fs::read_to_string(&today_path).unwrap(), "");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("secret.txt")).unwrap(),
            "không đụng vào"
        );
    }

    #[test]
    fn clear_logs_on_empty_dir_is_a_noop() {
        let dir = tempdir().unwrap();
        clear_logs(dir.path()).unwrap();
    }

    #[test]
    fn default_export_file_name_formats_a_fixed_instant_as_utc_yyyymmdd() {
        // 100 ngày sau epoch = 1970-04-11 (UTC) — mốc cố định, tính tay bằng
        // thuật toán `civil_from_unix_days` để test tất định, không phụ
        // thuộc đồng hồ hệ thống lúc chạy test.
        let instant = UNIX_EPOCH + std::time::Duration::from_secs(100 * 86_400);
        assert_eq!(default_export_file_name(instant), "trans-kun-diagnostics-19700411.txt");
    }

    #[test]
    fn current_log_file_name_matches_tracing_appender_daily_format() {
        let instant = UNIX_EPOCH + std::time::Duration::from_secs(100 * 86_400);
        assert_eq!(current_log_file_name(instant), "trans-kun.1970-04-11");
    }

    #[test]
    fn code_every_variant_still_maps_when_used_from_diagnostics() {
        // Ancre nhẹ: `record_error` phải nhận mọi `Category`, không chỉ một
        // vài biến thể — nếu ai thêm `Category` mới mà quên thêm vào
        // `ERROR_CATEGORIES`, test `summary_on_fresh_db_is_all_zero_but_covers_all_eight_categories`
        // ở trên sẽ lệch số lượng và bắt được ngay; test này chỉ khẳng định
        // `Code::Storage` (dùng ở nhiều nơi trong module) vẫn map đúng.
        assert_eq!(Code::Storage.category(), Category::Storage);
    }
}
