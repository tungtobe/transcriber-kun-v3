//! Helper đường dẫn an toàn — chỉ nhận newtype ID ([`SessionId`]/[`JobId`]),
//! không bao giờ nhận `&str` thô. Vì các ID này chỉ tạo được qua `new()` hay
//! `TryFrom<&str>` đã kiểm UUIDv7 hợp lệ ([`crate::core::id`]), một đường dẫn
//! dựng từ chúng không thể chứa `..`, `/`, hay ký tự path traversal khác.
//!
//! Story 2.3: staging nằm dưới `media/.staging/<job-id>` (không phải một cây
//! `staging/` ngang hàng) — một `Db::open`/reconcile chỉ cần duyệt một gốc
//! (`media_root`) để thấy cả Phiên đã publish lẫn phần nháp còn sót, và tên
//! `.staging` không thể khớp bất kỳ `SessionId::try_from` hợp lệ nào (không
//! phải UUIDv7) nên không lẫn với một thư mục Phiên khi duyệt.

use std::path::{Path, PathBuf};

use super::id::{JobId, SessionId};

/// Thư mục gốc chứa mọi Phiên đã publish: `<root>/media`.
pub fn media_root(root: &Path) -> PathBuf {
    root.join("media")
}

/// Thư mục media của một phiên: `<root>/media/<session-id>`.
pub fn media_dir(root: &Path, session_id: SessionId) -> PathBuf {
    media_root(root).join(session_id.to_string())
}

/// Đường dẫn Proxy đã publish của một phiên: `<root>/media/<session-id>/proxy.<ext>`.
/// Tên file cố định `proxy.<ext>` (AD-5, Design Notes) — tên `proxy-<uuid>`
/// chỉ tồn tại trong staging, không bao giờ ở đây.
pub fn proxy_path(root: &Path, session_id: SessionId, ext: &str) -> PathBuf {
    media_dir(root, session_id).join(format!("proxy.{ext}"))
}

/// Thư mục gốc chứa staging của mọi job: `<root>/media/.staging`.
pub fn staging_root(root: &Path) -> PathBuf {
    media_root(root).join(".staging")
}

/// Thư mục staging của một job: `<root>/media/.staging/<job-id>`.
pub fn staging_dir(root: &Path, job_id: JobId) -> PathBuf {
    staging_root(root).join(job_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_root_nests_under_root() {
        let root = Path::new("/data");
        assert_eq!(media_root(root), root.join("media"));
    }

    #[test]
    fn media_dir_nests_under_root_media_by_session_id() {
        let root = Path::new("/data");
        let id = SessionId::new();
        let dir = media_dir(root, id);
        assert_eq!(dir, root.join("media").join(id.to_string()));
        assert!(dir.starts_with(root));
    }

    #[test]
    fn proxy_path_is_a_fixed_name_under_the_session_media_dir() {
        let root = Path::new("/data");
        let id = SessionId::new();
        let path = proxy_path(root, id, "flac");
        assert_eq!(path, media_dir(root, id).join("proxy.flac"));
    }

    #[test]
    fn staging_root_nests_under_media_root_as_a_dotted_name() {
        let root = Path::new("/data");
        assert_eq!(staging_root(root), root.join("media").join(".staging"));
    }

    #[test]
    fn staging_dir_nests_under_staging_root_by_job_id() {
        let root = Path::new("/data");
        let id = JobId::new();
        let dir = staging_dir(root, id);
        assert_eq!(
            dir,
            root.join("media").join(".staging").join(id.to_string())
        );
        assert!(dir.starts_with(root));
    }

    #[test]
    fn staging_dir_name_never_collides_with_a_valid_session_id() {
        // `.staging` không parse được thành `SessionId` (không phải UUIDv7)
        // -- reconcile dựa vào tính chất này để không lẫn thư mục staging
        // với một Phiên lạ khi duyệt `media_root`.
        assert!(SessionId::try_from(".staging").is_err());
    }
}
