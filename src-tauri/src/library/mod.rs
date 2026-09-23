//! Library: Phiên, Tag, Ghi chú, export — quản lý nội dung đã có trong thư viện.
//! Story 2.3: `store` chịu trách nhiệm publish/commit một Phiên file bền vững
//! (đường staging → publish Proxy → một transaction DB) và reconcile lúc
//! boot; các mặt còn lại của Library (Tag, Ghi chú, export) chưa cài đặt.

pub mod store;
