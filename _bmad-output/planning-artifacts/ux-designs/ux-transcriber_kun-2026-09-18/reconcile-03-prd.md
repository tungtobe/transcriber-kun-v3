# Đối chiếu — PRD trans-kun v3 → hai spine

**Nguồn:** `_bmad-output/planning-artifacts/prds/prd-transcriber_kun-2026-09-17/prd.md` (+ `addendum.md`)
**Ngày:** 2026-09-18 · **Kết luận:** 47/47 FR có mặt; 10/12 NFR có mặt UX; 5/5 hành trình thành Key Flow.

## Độ phủ FR

| FR | Mục trong spine |
|---|---|
| FR-1, FR-47 | `EXPERIENCE.Đa ngôn ngữ` · Flow 1 |
| FR-2 | `EXPERIENCE.Minh bạch chi phí & quyền riêng tư` · State Patterns · Flow 1, 5 |
| FR-3, FR-5 | Component Patterns (ô key) · Minh bạch chi phí · Flow 1 |
| FR-4 | State Patterns (*Chưa có key*) · `DESIGN.button-disabled` |
| FR-6 | `Taxonomy lỗi` (Quota, Key bị từ chối) · State Patterns · Flow 3 |
| FR-7 | State Patterns (*Model bị từ chối*) · Taxonomy lỗi |
| FR-8, FR-11 | Component Patterns (Drop-zone) · Minh bạch chi phí · Flow 3 |
| FR-9 | Taxonomy lỗi (Định dạng) · Flow 3 (thất bại) |
| FR-10 | State Patterns (*Proxy hỏng*) · Component Patterns (Trình phát) · Flow 4 (thất bại) |
| FR-12 | Component Patterns (Card job) · State Patterns · Độ bền phiên |
| FR-13 | Component Patterns (Danh sách segment) · IA (Cài đặt → Chunking) |
| FR-14 | **Ẩn khỏi UI có chủ ý** — xem *Ý cố tình không lên UI* |
| FR-15 | State Patterns (*Transcript partial*) · Component Patterns · Flow 3 (cao trào) |
| FR-16, FR-19 | IA (LiveSetup, Cài đặt) · State Patterns (*Không dịch*) |
| FR-17 | State Patterns (*Thiếu quyền*) · Responsive & Platform · Flow 2 |
| FR-18 | Component Patterns (Tự cuộn) · Độ bền phiên |
| FR-20 – FR-25 | `Độ bền phiên & trạng thái kết nối` (toàn mục) · Flow 2 |
| FR-26, FR-34 | IA · Component Patterns · Flow 2 (thất bại) |
| FR-27 – FR-30 | Component Patterns (Dòng phiên, Ô tìm, Tag picker, Hàng chip) · Flow 4 |
| FR-31 | Responsive & Platform (WAV/FLAC theo OS) |
| FR-32, FR-33, FR-35 | Component Patterns (Trình phát, Ô tìm) · Interaction Primitives · Flow 4 |
| FR-36 – FR-38 | Component Patterns (Panel Ghi chú, Panel Memo, Editor template) · Minh bạch chi phí |
| FR-39, FR-42 | Component Patterns (Trường Settings) · IA |
| FR-40 | IA (Cài đặt → Lưu trữ) · Taxonomy lỗi (Lưu trữ) |
| FR-41, FR-43 | Minh bạch chi phí & quyền riêng tư · IA · Flow 5 |
| FR-44 – FR-46 | `Ad slot & tuân thủ store` |

## Độ phủ NFR

Có mặt UX: NFR-1 (Minh bạch chi phí), NFR-2 (Taxonomy lỗi — "lỗi một luồng không làm hỏng luồng khác"), NFR-3 (Độ bền phiên), NFR-4 + NFR-7 (Foundation), NFR-5 (ngưỡng nằm rải trong Component/State Patterns + state *Cold load*), NFR-6 (Taxonomy lỗi — Mạng/CA), NFR-8 (Minh bạch chi phí), NFR-9 (Taxonomy lỗi), NFR-11 (Accessibility Floor).

**Không có mặt UX, có chủ ý:** **NFR-10** (bundle ≤ 60 MB, RAM ≤ 300 MB) và **NFR-12** (an toàn đường dẫn file) là ràng buộc kiến trúc, không phải quyết định giao diện → thuộc `bmad-architecture`.

## Hành trình

| PRD | Spine | Protagonist giữ nguyên tên | Cao trào |
|---|---|---|---|
| UJ-1 | Flow 1 | Linh (BrSE, MacBook M2) | Transcript chạy trong < 3 phút, không cài gì thêm |
| UJ-2 | Flow 2 | Linh | Dừng sau 60 phút — 3 phút mất mạng, 6 lần reconnect — không mất gì |
| UJ-3 | Flow 3 | Minh (sales, Windows 11 + Zscaler) | "Chạy lại phần thiếu" 5 phút thay vì cả 90 |
| UJ-4 | Flow 4 | Linh | Nghe đúng câu trong 30 giây |
| UJ-5 | Flow 5 | Reviewer của Apple | Đi hết vòng sản phẩm không gặp màn trắng/quyền lạ |

## Ý cố tình không lên UI

- **FR-14 (hai họ model transcribe).** PRD nói thẳng: *"Người dùng không cần biết họ model; app tự chọn cách gọi theo tên."* Spine giữ nguyên sự vô hình này. Hệ quả duy nhất lộ ra UI là quy tắc gộp word thành segment ≈ 8 s — và ngay cả nó cũng chỉ nhìn thấy gián tiếp qua độ dài dòng transcript.
- **Speaker label.** FR-14 và FR-34 cho phép hiển thị speaker; quyết định 2026-09-18 **ẩn ở v3**. Trường `speaker` vẫn nằm trong data model.
- **FR-46 (chữ ký `ads.json`).** Việc xác minh chữ ký là backend; UI chỉ thấy kết quả (dùng creative hợp lệ, hoặc rơi về creative nhúng sẵn).
- **`is_premium` / KeyProvider.** PRD §9 chừa sẵn nhưng **không có UI ở v3** — spine không nhắc tới ngoài một dòng trong mục Ad slot.

## Khép kín bề mặt — ĐÃ ĐÓNG TOÀN BỘ 2026-09-18

PRD §8 và doc 06 định nghĩa bề mặt; hành trình UJ-1…5 đi qua phần lớn. Bốn bề mặt còn lại **không có hành trình nào chạm tới** — chủ sản phẩm xác nhận đó là **đúng ý đồ**, vì cả bốn đều là bề mặt cấu hình/tiện ích chứ không phải luồng công việc:

| Bề mặt | Phán quyết (2026-09-18) |
|---|---|
| **Cài đặt → Memo** (FR-37) | Template memo quản lý tại đây (nav "Memo", heading "Memo — mẫu prompt"). Bề mặt cấu hình, không cần hành trình riêng. |
| **Cài đặt → Cấu hình đề xuất** (FR-42) | Một nút → **hiện diff cho người dùng xem trước khi áp dụng** (giữ nguyên theo PRD). Diff render **inline trong chính nhóm setting đó**, không phải route mới — nên vẫn không có màn hình riêng và không cần hành trình. |
| **Tải recording** (FR-31) | Chỉ là một mục trong **menu ⋯** của dòng phiên live ở Trang chủ. Không phải màn riêng. |
| **Cài đặt → Lưu trữ / Chẩn đoán** (FR-40/41) | **Chỉ hiển thị thông tin và tải log.** Không có luồng nhiều bước. |

Hành vi của cả ba mục sau đã được ghi thành dòng trong `EXPERIENCE.md.Component Patterns`.

## Câu hỏi mở của PRD có ảnh hưởng tới UX

| PRD Q | Ảnh hưởng |
|---|---|
| **Q1 — Opus trong webm/mkv** | Quyết định câu chữ lỗi category "Định dạng" và danh sách định dạng hiện trong dialog chọn file (FR-9) |
| **Q3 — FLAC seek trong WebView** | Nếu spike S8 hỏng, `player` phải chuyển sang AAC native hoặc player trong Rust → đổi đặc tả `DESIGN.components.player` |
| **Q4 — Key demo cho reviewer** | Flow 5 phụ thuộc hoàn toàn vào nó; hết quota giữa lúc review là rủi ro trực tiếp cho SM-1 |
| **Q8 — Nội dung template mặc định ba ngôn ngữ** | Liên quan trực tiếp tới lỗ hổng khép kín số 1 ở trên |
