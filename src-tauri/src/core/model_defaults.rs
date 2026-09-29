//! Default Gemini model names shared by `settings` (fallback values for the
//! three persisted model preferences) and `gemini::params` (defaults used by
//! direct Gemini calls). Living in `core` keeps the dependency one-way: two
//! features (`settings`, `gemini`) both depend on `core`, neither depends on
//! the other (Architecture Spine AD-1/AD-13: feature không import lẫn nhau).

/// API `models.list` trả tên dạng `models/gemini-...` (tên tài nguyên), còn
/// mọi đường dẫn REST của app tự ghép `/v1beta/models/{model}:...`. App chỉ
/// lưu và hiển thị tên trần (`gemini-...`); hàm này bỏ tiền tố `models/` nếu
/// có để không bao giờ ghép ra `models/models/...`.
pub fn bare_model_name(name: &str) -> &str {
    let name = name.trim();
    name.strip_prefix("models/").unwrap_or(name)
}

pub const DEFAULT_TRANSCRIBE_MODEL: &str = "gemini-flash-lite-latest";
pub const DEFAULT_MEMO_MODEL: &str = "gemini-flash-lite-latest";
pub const DEFAULT_LIVE_MODEL: &str = "gemini-3.5-live-translate-preview";

#[cfg(test)]
mod tests {
    use super::bare_model_name;

    #[test]
    fn strips_the_models_resource_prefix_only_once_and_only_at_the_start() {
        assert_eq!(bare_model_name("models/gemini-3-flash"), "gemini-3-flash");
        assert_eq!(bare_model_name("gemini-3-flash"), "gemini-3-flash");
        assert_eq!(bare_model_name("  models/gemini-x  "), "gemini-x");
        assert_eq!(bare_model_name("models/models/x"), "models/x");
        assert_eq!(bare_model_name("my-models/x"), "my-models/x");
    }
}
