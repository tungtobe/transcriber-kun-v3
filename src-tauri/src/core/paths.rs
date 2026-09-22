//! Helper đường dẫn an toàn — chỉ nhận newtype ID ([`SessionId`]/[`JobId`]),
//! không bao giờ nhận `&str` thô. Vì các ID này chỉ tạo được qua `new()` hay
//! `TryFrom<&str>` đã kiểm UUIDv7 hợp lệ ([`crate::core::id`]), một đường dẫn
//! dựng từ chúng không thể chứa `..`, `/`, hay ký tự path traversal khác.

use std::path::{Path, PathBuf};

use super::id::{JobId, SessionId};

/// Thư mục media của một phiên: `<root>/media/<session-id>`.
pub fn media_dir(root: &Path, session_id: SessionId) -> PathBuf {
    root.join("media").join(session_id.to_string())
}

/// Thư mục staging của một job: `<root>/staging/<job-id>`.
pub fn staging_dir(root: &Path, job_id: JobId) -> PathBuf {
    root.join("staging").join(job_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_dir_nests_under_root_media_by_session_id() {
        let root = Path::new("/data");
        let id = SessionId::new();
        let dir = media_dir(root, id);
        assert_eq!(dir, root.join("media").join(id.to_string()));
        assert!(dir.starts_with(root));
    }

    #[test]
    fn staging_dir_nests_under_root_staging_by_job_id() {
        let root = Path::new("/data");
        let id = JobId::new();
        let dir = staging_dir(root, id);
        assert_eq!(dir, root.join("staging").join(id.to_string()));
        assert!(dir.starts_with(root));
    }
}
