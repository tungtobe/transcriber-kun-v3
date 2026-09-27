//! Memo: sinh memo từ template dựa trên transcript, request đơn lẻ không vào hàng đợi (chưa cài đặt).
//!
//! Story 3.6 thêm quản lý Template memo (`templates`/`defaults`) -- sinh memo
//! thật (gọi Gemini) vẫn chưa cài đặt (spec Never: "Không sinh memo hay gọi
//! Gemini ở story này").

pub mod defaults;
pub mod templates;
