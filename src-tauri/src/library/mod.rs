//! Library: Phiên, Tag, Ghi chú, export — quản lý nội dung đã có trong thư viện.
//! `store` owns durable session data; `export` renders the selected transcript
//! without granting file access to the WebView.

pub mod export;
pub mod notes;
pub mod store;
pub mod tags;
