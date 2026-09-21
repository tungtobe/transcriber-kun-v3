# Đối chiếu "02 — Bài học & điểm cần chú ý" với PRD + Addendum (trans-kun v3)

Nguồn input: `docs/rebuild-v3/02-bai-hoc-va-luu-y.md`
Đối chiếu với: `prd.md` và `addendum.md` (bộ `prd-transcriber_kun-2026-09-17`).
Phạm vi: nhóm A–F, H–K (bỏ qua G — Copilot, đã loại khỏi v3 theo Q1/non-goals).

Quy ước: bài học kỹ thuật thuần (cách làm, không ai dùng app thấy trực tiếp) → chỉ cần có ở addendum. Bài học tạo ra hành vi/lỗi người dùng thấy được → phải có ở PRD dưới dạng FR/NFR.

## Khoảng trống

- A2 → Không có runtime tải về là đúng (NFR-4), nhưng câu cụ thể "version pin ở Cargo.lock" (khoá version dependency, không để venv/toolchain trôi như v2) chưa được ghi lại ở đâu trong addendum → nên vào addendum, mức thấp.
- D8 → Model Live là preview, có thể đổi tên/hành vi không báo trước; bài học yêu cầu "kiểm thử smoke tự động mỗi tuần với key test" để phát hiện sớm. PRD/addendum chỉ có spike một lần (S3, Phase 0) chứ không có cơ chế giám sát định kỳ liên tục sau khi ra mắt → nên vào addendum (mục K lộ trình hoặc mục vận hành CI), mức trung — đây là rủi ro tái diễn D2/D8 (server đóng kết nối vì shape đổi) mà không ai phát hiện cho tới khi người dùng báo lỗi.
- H4 → PRD FR-38 đã yêu cầu sanitize Markdown khi render (đúng phần "người dùng thấy"), nhưng addendum (mục B — stack) không liệt kê thư viện markdown dùng qua npm (thay cho `marked.min.js` vendored cũ) → nên bổ sung vào addendum mục B, mức thấp (tránh lặp lại thói quen vendor file cũ).
- K4 (deferred bug "Profile/skill ID chưa sanitize trước khi ghép vào đường dẫn file") → schema v3 dùng ID tự sinh (SQLite) nên rủi ro giảm, nhưng addendum/PRD chưa phát biểu tường minh nguyên tắc chung "không bao giờ dùng chuỗi người dùng nhập (tên Phiên, Tag, Template) trực tiếp làm thành phần đường dẫn file trong Container" → nên vào addendum như một invariant kế thừa, mức trung (đây là bài học bảo mật cụ thể chưa được tổng quát hoá, dễ bị quên khi implement export/rename).
- K6 (deferred bug "restart + worker cũ chết cùng lúc có thể để UI báo recording khi không còn worker") → generation guard (FR-20, addendum mục L) giảm rủi ro sự kiện cũ lọt vào, nhưng chưa có yêu cầu tường minh dạng "UI không bao giờ hiển thị trạng thái đang ghi/kết nối khi không còn kết nối sống" → nên vào PRD (đây là hành vi người dùng thấy được — trạng thái sai gây hiểu lầm là vẫn đang ghi) như hệ quả bổ sung của FR-22/FR-23, mức trung.

## Mâu thuẫn

- C6 vs FR-6: bài học C6 mô tả chính sách v2 là "xoay vòng với cooldown 60 s [khi quota], **không xoay** với 401/403/400/404/timeout", và cột "Áp dụng v3" ghi "giữ nguyên chính sách". Nhưng PRD FR-6 lại quy định cho 401/403 là "loại key khỏi vòng và báo" — tức có hành vi chủ động lên vòng xoay (loại bỏ + có thể chuyển key khác), khác với "không xoay" trong mô tả gốc. Hai cách diễn đạt có thể tương thích (loại khỏi vòng cho *tương lai*, còn request hiện tại vẫn fail như bài học mô tả) nhưng câu chữ hiện tại đủ mơ hồ để gây hiểu sai khi viết architecture/epics — nên làm rõ trong addendum: request hiện tại có retry ngay bằng key khác khi gặp 401/403 hay không.
- E1: bài học đề xuất "fallback SCK (ScreenCaptureKit) cho macOS 13.x nếu vẫn hỗ trợ", nhưng PRD (Q3, và explicit ở §11.2 Ngoài phạm vi MVP) chốt **không làm fallback SCK**, giới hạn tối thiểu macOS 14.4. Đây là quyết định phạm vi có chủ đích (đã chốt ngày 2026-09-13/14), không phải khoảng trống — liệt kê ở đây chỉ để lưu ý đây là một điểm bài học 02 và PRD "nói ngược nhau" một cách có chủ ý, không cần xử lý thêm.

## Đã phủ tốt

A1, A3–A8, B1–B4, C1–C5, C7, D1–D7, E2–E5, E7–E8, F1, F3–F7, H1–H3, H5, I1, I4–I6, J (keychain FR-5, redaction FR-41, no-telemetry §6.1/FR-45) đều có đối chứng rõ ràng ở PRD (khi tạo hành vi người dùng thấy) hoặc addendum (khi thuần kỹ thuật), phần lớn khớp gần như nguyên văn (vd. C3↔FR-15/FR-26, D4/D5↔FR-19, F3↔FR-23, I5↔FR-41). A4, B2, E6, I2, I3 và K1/K2/K3/K7 không cần biện pháp vì nguyên nhân gốc (Windows console cũ, cài ffmpeg tự động, Linux, updater, Copilot/Radar/Sniper) đã bị loại khỏi v3 theo các quyết định chốt (Q1, Q6, non-goals).
