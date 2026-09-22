//! `Sensitive<T>`: bọc dữ liệu nhạy cảm (transcript, dịch, ghi chú, memo, key,
//! prompt — Architecture Spine) để `{:?}`/`{}` luôn in `[redacted]`, không
//! bao giờ vô tình lọt vào log qua `tracing::debug!("{:?}", value)` hay
//! tương tự. Truy cập giá trị thật chỉ qua [`Sensitive::expose`], một cái tên
//! đủ chướng mắt để người đọc code phải dừng lại kiểm tra chỗ dùng.

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct Sensitive<T>(T);

impl<T> Sensitive<T> {
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    /// Lấy giá trị thật ra khỏi lớp bọc — dùng khi thật sự cần giá trị (gọi
    /// API, so khớp), không bao giờ dùng để in log.
    pub fn expose(&self) -> &T {
        &self.0
    }

    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> fmt::Debug for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

impl<T> fmt::Display for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

impl<T> From<T> for Sensitive<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_inner_value() {
        let secret = Sensitive::new("AIzaSyABCDEFGHIJKLMNOPQRSTUVWXYZ01234".to_string());
        assert_eq!(format!("{secret:?}"), "[redacted]");
    }

    #[test]
    fn display_never_shows_inner_value() {
        let secret = Sensitive::new("top secret transcript content".to_string());
        assert_eq!(format!("{secret}"), "[redacted]");
    }

    #[test]
    fn expose_returns_real_value() {
        let secret = Sensitive::new(42);
        assert_eq!(*secret.expose(), 42);
    }
}
