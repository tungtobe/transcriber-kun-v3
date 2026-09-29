//! Log content-free (FR-41, AR-29): mọi dòng log đi qua [`redact`] trước khi
//! chạm đĩa. `redact` là hàm thuần (không I/O) để test độc lập; phần khởi
//! tạo `tracing` ghi file xoay vòng hằng ngày, giữ tối đa 7 file, nằm ở
//! [`init_file_logging`] và bọc writer thật bằng [`RedactingWriter`] để
//! redaction áp dụng vô điều kiện, không phụ thuộc người viết `tracing::info!`
//! có nhớ tự redact hay không.

use std::io::{self, Write};
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_appender::rolling::{Builder as RollingBuilder, Rotation};

const MAX_LOG_FILES: usize = 7;
/// Prefix chung cho tên file log — `diagnostics::` dùng lại hằng này để xây
/// allow-list xuất/xoá log (spec Boundaries: "tên khớp `trans-kun` + hậu tố
/// rotation của `core::log`"), không định nghĩa lại một chuỗi thứ hai.
pub const LOG_FILE_PREFIX: &str = "trans-kun";

/// Các pattern bí mật cần thay bằng `[redacted]`, áp theo thứ tự. URL/khoá là
/// một khối duy nhất bị thay nguyên vẹn; header `authorization` giữ lại tên
/// trường, chỉ thay phần giá trị.
static REDACTIONS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        // Header authorization (bất kỳ hoa/thường), thay phần giá trị sau dấu
        // `:`/`=` tới khi gặp dấu ngoặc/nháy/phẩy/xuống dòng hoặc hết chuỗi.
        (
            Regex::new(r#"(?i)(authorization\s*[:=]\s*)[^"',\r\n}]+"#).unwrap(),
            "${1}[redacted]",
        ),
        // Google API key dạng `AIza...`.
        (
            Regex::new(r"AIza[0-9A-Za-z_\-]{10,}").unwrap(),
            "[redacted]",
        ),
        // OAuth/refresh token dạng `AQ....`.
        (
            Regex::new(r"AQ\.[0-9A-Za-z_\-.]{5,}").unwrap(),
            "[redacted]",
        ),
        // Toàn bộ URL http(s), có thể mang query chứa key.
        (Regex::new(r#"https?://[^\s"'<>]+"#).unwrap(), "[redacted]"),
    ]
});

/// Hàm thuần: thay mọi bí mật nhận diện được trong `input` bằng `[redacted]`.
/// Không I/O — test gọi trực tiếp hàm này với từng mẫu trong I/O Matrix.
pub fn redact(input: &str) -> String {
    let mut out = input.to_string();
    for (pattern, replacement) in REDACTIONS.iter() {
        out = pattern.replace_all(&out, *replacement).into_owned();
    }
    out
}

/// `Write` bọc một writer thật, redact toàn bộ nội dung của mỗi lần `write`
/// trước khi chuyển tiếp — vô điều kiện, không cần người gọi tự nhớ redact.
struct RedactingWriter<W> {
    inner: W,
}

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(buf);
        let redacted = redact(&text);
        self.inner.write_all(redacted.as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Dựng writer xoay vòng hằng ngày (giữ tối đa [`MAX_LOG_FILES`] file) trong
/// `log_dir`, đã bọc redaction, kèm `WorkerGuard` của nó — tách khỏi
/// [`init_file_logging`] để dùng chung cho cả đường cài global subscriber
/// (production, `ipc::boot`) lẫn đường cài subscriber phạm vi cục bộ (test,
/// qua `tracing::subscriber::with_default`), tránh test phụ thuộc vào việc
/// ai thắng cuộc đua `try_init` subscriber toàn cục.
fn build_non_blocking_writer(log_dir: &Path) -> io::Result<(NonBlocking, WorkerGuard)> {
    std::fs::create_dir_all(log_dir)?;

    let appender = RollingBuilder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .max_log_files(MAX_LOG_FILES)
        .build(log_dir)
        .map_err(io::Error::other)?;

    let writer = RedactingWriter { inner: appender };
    Ok(tracing_appender::non_blocking(writer))
}

/// Bộ lọc mặc định: code của app ghi từ mức DEBUG (chỉ metadata — trạng thái,
/// mã lỗi, đếm; không transcript/audio/khoá, và mọi dòng vẫn qua [`redact`]),
/// thư viện ngoài từ INFO, `symphonia` từ WARN vì rất ồn khi probe file.
/// Đặt biến môi trường `RUST_LOG` (ví dụ `RUST_LOG=trace` hoặc
/// `RUST_LOG=trans_kun_lib::gemini=trace,info`) để ghi đè khi cần debug sâu.
const DEFAULT_LOG_FILTER: &str = "info,trans_kun_lib=debug,symphonia=warn";

fn default_env_filter() -> tracing_subscriber::EnvFilter {
    tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(DEFAULT_LOG_FILTER))
}

/// Khởi tạo `tracing` ghi ra file xoay vòng hằng ngày trong `log_dir`, giữ
/// tối đa [`MAX_LOG_FILES`] file, qua lớp redaction, và cài làm subscriber
/// toàn cục của app. Trả về `WorkerGuard`: người gọi phải giữ giá trị này
/// sống suốt vòng đời app (drop sớm sẽ ngắt worker ghi log không đồng bộ),
/// thường bằng cách cất vào Tauri managed state — xem `ipc::boot`.
///
/// Nhiều lần gọi trong cùng một process (ví dụ nhiều test) sẽ không panic:
/// từ lần thứ hai trở đi hàm này bỏ qua việc set global subscriber (đã có
/// subscriber từ lần gọi trước) nhưng vẫn trả về guard hợp lệ trỏ tới file
/// log riêng của lần gọi đó. Test muốn chắc chắn sự kiện của chính mình đi
/// đúng vào file của mình (không phụ thuộc ai thắng cuộc đua global) nên
/// dùng thẳng [`build_non_blocking_writer`] + `tracing::subscriber::with_default`
/// thay vì hàm này — xem `tests::fake_session_log_never_leaks_raw_secrets_to_disk`.
pub fn init_file_logging(log_dir: &Path) -> io::Result<WorkerGuard> {
    let (non_blocking, guard) = build_non_blocking_writer(log_dir)?;

    // `try_init` thay vì `init`: gọi lại (nhiều lần boot giả lập trong cùng
    // process, ví dụ test) sẽ trả `Err` thay vì panic — bỏ qua lỗi đó vì
    // subscriber toàn cục vẫn dùng writer của lần init đầu tiên, không ảnh
    // hưởng việc guard mới ở đây có ghi được ra file riêng của nó
    // (`non_blocking` ghi trực tiếp, không qua subscriber toàn cục).
    let _ = tracing_subscriber::fmt()
        .with_env_filter(default_env_filter())
        .with_writer(non_blocking)
        .with_ansi(false)
        .try_init();

    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_google_api_key() {
        let input = "gemini call failed with key AIzaSyABCDEFGHIJKLMNOPQRSTUVWXYZ01234";
        let out = redact(input);
        assert!(!out.contains("AIzaSy"));
        assert!(out.contains("[redacted]"));
    }

    #[test]
    fn redacts_aq_dot_token() {
        let input = "refresh token AQ.Ab8_exampleTokenValue123 expired";
        let out = redact(input);
        assert!(!out.contains("AQ."));
        assert!(out.contains("[redacted]"));
    }

    #[test]
    fn redacts_full_url_including_query_key() {
        let input = "GET https://generativelanguage.googleapis.com/v1/models?key=AIzaSySECRETVALUEHERE0000 -> 200";
        let out = redact(input);
        assert!(!out.contains("googleapis.com"));
        assert!(!out.contains("AIzaSy"));
        assert!(out.contains("[redacted]"));
    }

    #[test]
    fn redacts_authorization_header_value_keeps_field_name() {
        let input = "request headers: authorization: Bearer super-secret-token-value";
        let out = redact(input);
        assert!(out.to_lowercase().contains("authorization: [redacted]"));
        assert!(!out.contains("super-secret-token-value"));
    }

    #[test]
    fn leaves_plain_text_untouched() {
        let input = "phiên đã kết thúc thành công, không có gì nhạy cảm ở đây";
        assert_eq!(redact(input), input);
    }

    #[test]
    fn redacts_all_secret_kinds_in_one_line() {
        let input = "err: authorization: Bearer abcxyz , url https://x.test/y?key=AIzaSyZZZZZZZZZZZZZZZZZZZZZZZZ01 , token AQ.refreshTokenValue123";
        let out = redact(input);
        assert!(!out.contains("Bearer abcxyz"));
        assert!(!out.contains("x.test"));
        assert!(!out.contains("AIzaSy"));
        assert!(!out.contains("AQ."));
    }

    /// Test "phiên mẫu" (spec Tasks): dựng một phiên log thật (không gọi
    /// `redact` trực tiếp) ghi transcript giả qua `Sensitive` và một chuỗi
    /// lỗi thô chứa key/URL/token/authorization header, rồi grep mọi file
    /// log sinh ra trong thư mục tạm và khẳng định không còn chuỗi gốc nào.
    /// Đây là bài kiểm tra đầu-cuối cho lớp redaction ở writer, không phải
    /// cho hàm `redact` thuần (đã có các test riêng ở trên).
    ///
    /// Cài subscriber phạm vi cục bộ qua `tracing::subscriber::with_default`
    /// (không phải `init_file_logging`/global): `cargo test` chạy nhiều test
    /// song song trong cùng process, và subscriber toàn cục chỉ set được một
    /// lần (`try_init` các lần sau là no-op) — nếu test này gọi
    /// `init_file_logging` mà một test khác đã thắng cuộc đua global trước,
    /// hai dòng `tracing::info!`/`error!` dưới đây sẽ lặng lẽ bay vào file
    /// log của test kia, còn file của chính test này rỗng, và assert
    /// "không chứa bí mật" sẽ đúng một cách vô nghĩa (vacuously). Subscriber
    /// cục bộ đảm bảo hai dòng log chắc chắn đi vào đúng writer của test này.
    #[test]
    fn fake_session_log_never_leaks_raw_secrets_to_disk() {
        let dir = tempfile::tempdir().expect("tạo thư mục tạm cho log phải thành công");
        let (non_blocking, guard) =
            build_non_blocking_writer(dir.path()).expect("khởi tạo log phải thành công");
        let subscriber = tracing_subscriber::fmt()
            .with_writer(non_blocking)
            .with_ansi(false)
            .finish();

        let transcript = crate::core::sensitive::Sensitive::new(
            "Đây là nội dung transcript bí mật của cuộc họp giả lập, tuyệt đối không được lộ ra log."
                .to_string(),
        );
        let fake_api_key = "AIzaSyFAKEKEYFORLOGTESTONLYDONOTUSE00";
        let fake_oauth_token = "AQ.Ab8_fakeOauthTokenForLogTestOnly123";
        let fake_url =
            format!("https://generativelanguage.googleapis.com/v1/models?key={fake_api_key}");

        // Ghi qua `Sensitive` (Debug/Display tự redact, không cần biết gì về
        // lớp writer) và ghi một chuỗi lỗi thô (mô phỏng lỗi kỹ thuật ném
        // thẳng string chứa bí mật) để kiểm layer writer tự redact vô điều
        // kiện, không phụ thuộc người viết log có tự gọi `redact` hay không.
        // `with_default` đảm bảo cả hai dòng này chắc chắn đi qua
        // `subscriber` (và writer của nó) ở trên, không đi đâu khác.
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(transcript = ?transcript, "phiên transcribe mẫu hoàn tất");
            tracing::error!(
                "gọi Gemini thất bại: url={fake_url} , authorization: Bearer {fake_oauth_token}"
            );
        });

        drop(guard);

        let mut all_log_content = String::new();
        for entry in std::fs::read_dir(dir.path()).expect("đọc thư mục log tạm") {
            let entry = entry.expect("đọc entry thư mục log tạm");
            if entry.path().is_file() {
                all_log_content
                    .push_str(&std::fs::read_to_string(entry.path()).unwrap_or_default());
            }
        }

        assert!(
            !all_log_content.is_empty(),
            "phải có ít nhất một dòng log được ghi ra file"
        );
        assert!(
            all_log_content.contains("[redacted]"),
            "phải thấy marker [redacted] trong file log, thực tế: {all_log_content:?}"
        );
        assert!(!all_log_content.contains(transcript.expose().as_str()));
        assert!(!all_log_content.contains(fake_api_key));
        assert!(!all_log_content.contains(fake_oauth_token));
        assert!(!all_log_content.contains("generativelanguage.googleapis.com"));
    }
}
