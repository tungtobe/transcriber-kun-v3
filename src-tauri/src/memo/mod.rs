//! Memo: sinh memo từ template dựa trên transcript, request đơn lẻ không vào
//! hàng đợi Job.
//!
//! Story 3.6 thêm quản lý Template memo (`templates`/`defaults`). Story 3.7
//! (`generate`) thêm sinh memo thật: chụp đầu vào, dựng prompt, gọi
//! `gemini::GeminiGateway::post_job_observed_for(ModelKind::Memo, ...)`,
//! cache kết quả trong `memos` (spec Boundaries Always) -- chống trùng
//! request/huỷ theo cặp (Phiên, Template) thuộc registry ở `ipc::`, không ở
//! đây (spec: "registry trong `AppState`, chỉ `ipc/` điều phối").

pub mod defaults;
pub mod generate;
pub mod templates;
