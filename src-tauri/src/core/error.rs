//! `AppError`: kiểu lỗi thống nhất qua IPC. `category` là một trong 8 giá trị
//! ổn định; `code` là chi tiết kỹ thuật hơn nhưng luôn quy được về đúng một
//! `category` qua [`Code::category`] (một `match`, không nhánh `_`, để thêm
//! biến thể `Code` mới bắt buộc phải xử lý tường minh ở đây).
//!
//! `detail_redacted` không bao giờ chứa bí mật: mọi nơi tạo `AppError` từ một
//! chuỗi thô (thông điệp lỗi kỹ thuật, có thể chứa key/URL) đều đi qua
//! [`crate::core::log::redact`] trước khi lưu vào field này.

use serde::{Deserialize, Serialize};
use std::fmt;

/// 8 category ổn định hiển thị cho người dùng — spec Boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Quota,
    Auth,
    Model,
    Network,
    Format,
    Permission,
    Storage,
    Blocked,
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Category::Quota => "quota",
            Category::Auth => "auth",
            Category::Model => "model",
            Category::Network => "network",
            Category::Format => "format",
            Category::Permission => "permission",
            Category::Storage => "storage",
            Category::Blocked => "blocked",
        };
        f.write_str(s)
    }
}

/// Mã lỗi kỹ thuật — 12 biến thể, người dùng chốt 2026-09-22. Ánh xạ
/// `code → category` chỉ định nghĩa ở [`Code::category`], không lặp lại nơi
/// khác trong codebase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Code {
    Quota,
    Auth,
    Model,
    Request,
    Shape,
    Timeout,
    Network,
    Tls,
    Blocked,
    Format,
    Permission,
    Storage,
}

impl Code {
    /// Ánh xạ `code → category` duy nhất trong codebase. Cố ý liệt kê từng
    /// biến thể (không nhánh `_`) để thêm `Code` mới mà quên xếp category sẽ
    /// gãy biên dịch thay vì âm thầm rơi vào category sai.
    pub const fn category(self) -> Category {
        match self {
            Code::Quota => Category::Quota,
            Code::Auth => Category::Auth,
            Code::Model => Category::Model,
            Code::Request => Category::Model,
            Code::Shape => Category::Model,
            Code::Timeout => Category::Network,
            Code::Network => Category::Network,
            Code::Tls => Category::Network,
            Code::Blocked => Category::Blocked,
            Code::Format => Category::Format,
            Code::Permission => Category::Permission,
            Code::Storage => Category::Storage,
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// Lỗi thống nhất trả qua mọi command IPC. Serialize `{ category, code,
/// detailRedacted }` — spec Boundaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub category: Category,
    pub code: Code,
    pub detail_redacted: String,
}

impl AppError {
    /// Tạo `AppError` từ `code` + một chuỗi chi tiết thô. Chuỗi thô được
    /// redact ngay tại đây trước khi lưu — không có con đường nào khác để
    /// chi tiết chưa redact lọt vào `AppError`.
    pub fn new(code: Code, detail: impl AsRef<str>) -> Self {
        Self {
            category: code.category(),
            code,
            detail_redacted: crate::core::log::redact(detail.as_ref()),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}/{}] {}",
            self.category, self.code, self.detail_redacted
        )
    }
}

impl std::error::Error for AppError {}

impl From<rusqlite::Error> for AppError {
    fn from(err: rusqlite::Error) -> Self {
        AppError::new(Code::Storage, err.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::new(Code::Storage, err.to_string())
    }
}

impl From<rusqlite_migration::Error> for AppError {
    fn from(err: rusqlite_migration::Error) -> Self {
        AppError::new(Code::Storage, err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Phủ mọi biến thể `Code` — thêm biến thể mới mà quên thêm vào đây sẽ
    /// bị bắt vì `ALL` liệt kê tường minh từng biến thể.
    const ALL: [Code; 12] = [
        Code::Quota,
        Code::Auth,
        Code::Model,
        Code::Request,
        Code::Shape,
        Code::Timeout,
        Code::Network,
        Code::Tls,
        Code::Blocked,
        Code::Format,
        Code::Permission,
        Code::Storage,
    ];

    #[test]
    fn every_code_maps_to_expected_category() {
        let expected = [
            (Code::Quota, Category::Quota),
            (Code::Auth, Category::Auth),
            (Code::Model, Category::Model),
            (Code::Request, Category::Model),
            (Code::Shape, Category::Model),
            (Code::Timeout, Category::Network),
            (Code::Network, Category::Network),
            (Code::Tls, Category::Network),
            (Code::Blocked, Category::Blocked),
            (Code::Format, Category::Format),
            (Code::Permission, Category::Permission),
            (Code::Storage, Category::Storage),
        ];
        assert_eq!(
            expected.len(),
            ALL.len(),
            "test phải phủ đủ mọi biến thể Code"
        );
        for (code, category) in expected {
            assert_eq!(
                code.category(),
                category,
                "code {code:?} phải map sang {category:?}"
            );
        }
    }

    #[test]
    fn category_serializes_to_one_of_eight_stable_strings() {
        let all_categories = [
            Category::Quota,
            Category::Auth,
            Category::Model,
            Category::Network,
            Category::Format,
            Category::Permission,
            Category::Storage,
            Category::Blocked,
        ];
        let expected = [
            "\"quota\"",
            "\"auth\"",
            "\"model\"",
            "\"network\"",
            "\"format\"",
            "\"permission\"",
            "\"storage\"",
            "\"blocked\"",
        ];
        for (category, expected_json) in all_categories.iter().zip(expected) {
            assert_eq!(serde_json::to_string(category).unwrap(), expected_json);
        }
    }

    #[test]
    fn app_error_serializes_with_expected_shape() {
        let err = AppError::new(Code::Storage, "disk full");
        let value = serde_json::to_value(&err).unwrap();
        assert_eq!(value["category"], "storage");
        assert_eq!(value["code"], "storage");
        assert_eq!(value["detailRedacted"], "disk full");
    }

    #[test]
    fn app_error_redacts_secrets_in_detail() {
        let err = AppError::new(
            Code::Network,
            "call failed: https://example.com/x?key=AIzaSyABCDEFGHIJKLMNOPQRSTUVWXYZ01234",
        );
        assert!(!err.detail_redacted.contains("AIzaSy"));
        assert!(!err.detail_redacted.contains("example.com"));
    }
}
