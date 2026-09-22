//! Core: `AppError`, ID, `Sensitive<T>`, log content-free, đồng hồ, kiểu dữ
//! liệu dùng chung cho mọi tầng (Architecture Spine AD-1: tầng thấp nhất,
//! không phụ thuộc ngược lên `db`/`settings`/`ipc`).

pub mod error;
pub mod id;
pub mod log;
pub mod model_defaults;
pub mod paths;
pub mod sensitive;

pub use error::{AppError, Category, Code};
pub use id::{JobId, SessionId, TranscriptId};
pub use sensitive::Sensitive;
