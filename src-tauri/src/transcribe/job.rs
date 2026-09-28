//! Kiểu dữ liệu cho `JobRegistry` (story 2.4): trạng thái một Job, event phát
//! qua `jobs_subscribe`, và kết quả `jobs_cancel`. Không chứa logic actor —
//! xem `registry.rs`.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::core::error::AppError;
use crate::core::id::{JobId, SessionId};

/// Trạng thái hiển thị của một Job còn trong registry. Một Job biến mất khỏi
/// registry (và khỏi mọi `JobSnapshot` sau đó) ngay khi nó kết thúc — thành
/// công, lỗi, hay bị huỷ — nên không có biến thể "hoàn tất"/"lỗi"/"đã huỷ"
/// ở đây (spec Boundaries: "Không bảng `jobs`, không khôi phục Job sau
/// restart").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Queued,
    Running,
}

/// Loại Job trong cùng một hàng đợi `JobRegistry` (story 2.5): transcribe
/// một file mới, hoặc Chạy lại (vá vùng thiếu/toàn bộ) transcript `primary`
/// của một Phiên đã có (spec Approach: "Thêm Job loại Chạy lại vào đúng
/// hàng đợi `JobRegistry`"). Không phải bảng riêng, không hàng đợi thứ hai —
/// chỉ một cờ trên `JobSnapshot` để UI phân biệt hiển thị.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum JobKind {
    Transcribe,
    Rerun,
    Retranscribe,
}

/// Ảnh chụp một Job tại một thời điểm — đủ để UI vẽ "32 / 90 phút · 36 %",
/// Chunk hiện tại, key thứ mấy, số lần thử, và cờ "đang chờ quota" (spec
/// Tasks: trang Session).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct JobSnapshot {
    pub job_id: JobId,
    pub session_id: SessionId,
    pub source_name: Option<String>,
    pub kind: JobKind,
    pub state: JobState,
    // `u32`, not `u64`: specta-typescript forbids exporting BigInt-style
    // integers (see `ipc/spike_channel.rs`). Milliseconds in `u32` still
    // cover about 49 days of audio, far beyond any real file.
    pub processed_ms: u32,
    pub total_ms: u32,
    pub chunk_index: u32,
    pub chunk_count: u32,
    pub key_ordinal: Option<u32>,
    pub attempt: Option<u32>,
    pub waiting_quota: bool,
}

/// Một event của registry — mỗi biến thể mang riêng `seq: u32`, một bộ đếm
/// đơn điệu dùng chung cho toàn bộ registry, không phải theo từng Job (spec
/// Always: "`seq: u32` đơn điệu cho cả registry").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum JobEvent {
    /// Toàn bộ Job hiện có trong registry — gửi riêng cho một Channel mới
    /// đăng ký ngay khi `jobs_subscribe` được gọi, trước mọi event khác của
    /// đúng Channel đó (spec I/O Matrix "Remount UI").
    Snapshot { seq: u32, jobs: Vec<JobSnapshot> },
    /// Một Job (đang chờ hoặc đang chạy) vừa đổi trạng thái hiển thị.
    Updated { seq: u32, job: JobSnapshot },
    /// Job vừa commit xong — `session_id` khớp đúng id đã cấp lúc tạo Job.
    Result {
        seq: u32,
        job_id: JobId,
        session_id: SessionId,
    },
    /// Job kết thúc lỗi hệ thống (Auth/Model/Consent/đọc nguồn/DB) trước khi
    /// commit — không có dòng DB nào được tạo.
    Error {
        seq: u32,
        job_id: JobId,
        error: AppError,
    },
    /// Job bị huỷ (đang chờ trong hàng, hoặc đang chạy) trước khi commit.
    Cancelled { seq: u32, job_id: JobId },
}

impl JobEvent {
    pub fn seq(&self) -> u32 {
        match self {
            Self::Snapshot { seq, .. }
            | Self::Updated { seq, .. }
            | Self::Result { seq, .. }
            | Self::Error { seq, .. }
            | Self::Cancelled { seq, .. } => *seq,
        }
    }
}

/// Trả về từ lệnh `jobs_cancel` — phân biệt "đã bắt đầu huỷ" khỏi "Job không
/// còn để huỷ" (đã commit, hoặc id không tồn tại — spec I/O Matrix "Huỷ":
/// "Cancel đến sau commit -> trả kết quả hoàn tất").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CancelOutcome {
    Cancelling,
    AlreadyFinished,
}
