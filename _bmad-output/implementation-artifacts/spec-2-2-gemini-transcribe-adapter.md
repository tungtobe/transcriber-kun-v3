---
title: '2.2 Gemini transcribe adapter theo họ model và parser chịu lỗi'
type: 'feature'
created: '2026-09-23'
status: 'in-review'
baseline_commit: 'a0c6c82588af61dc3fd0ee05c79e29fcab0f12aa'
route: 'full'
route_source: 'auto'
review: 'none'
review_source: 'pinned'
lenses_ran: []
review_loop_iteration: 0
context: []
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** Chunk FLAC của story 2.1 chưa gửi được tới Gemini; hai họ model dùng API và response khác nhau, còn response hỏng có thể làm mất phần lời nói đã nhận.

**Approach:** Thêm adapter transcribe qua Gemini gateway, chọn protocol bằng capability đã kiểm chứng, chuẩn hoá cả hai response thành Segment thời gian tuyệt đối và báo phần không cứu được cho Job 2.5.

## Boundaries & Constraints

**Always:** Gửi inline, kiểm kích thước toàn JSON dưới 20 MB; mỗi Chunk deadline 120 s gồm chờ key, không fan-out. Model tổng quát dùng generateContent + JSON schema; model `*-transcribe` dùng Interactions verbatim, word timestamps, language codes. Giữ text hợp lệ, speaker, offset tuyệt đối và metadata thiếu; chỉ xác nhận silence từ response thành công rõ nghĩa. 5xx retryable, 400/404/blocked không retryable.

**Never:** Không tự thêm Files API/upload, không tự đổi model, không đoán capability của alias/tên nhập tay từ chuỗi, không log audio/transcript/key hay body response. Nếu inline của model chuyên transcribe không được xác minh hoặc API từ chối, báo gate S2/OQ2 rõ ràng thay vì upload ngầm hay âm thầm dùng model khác.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| General | FLAC và model có profile generateContent | JSON shape/schema đúng, Segment 5–15 s | HTTP/shape lỗi được phân loại |
| Specialized | Model transcribe có profile inline được xác minh | Interactions verbatim, word annotations gộp ≈8 s, cắt ở speaker/15 s | Gate S2 nếu thiếu bằng chứng inline |
| Damaged result | JSON cắt/fence, timestamp sai hoặc trùng | Giữ Segment hợp lệ, sắp và cộng offset, ghi phần mất | Không coi response rỗng/lỗi là silence |

</frozen-after-approval>

## Code Map

- `src-tauri/src/gemini/mod.rs` — gateway duy nhất; `TransportRequest` hiện chỉ GET và không có body, `send`/key pool/consent/error classification cần tái dùng.
- `src-tauri/src/gemini/params.rs` — deadline và giới hạn của Gemini.
- `src-tauri/src/media/chunk.rs` — `Chunk` chứa FLAC base64, `start_ms`, `duration_ms`; budget 20 MB hiện đo JSON của Chunk, cần đo request thật.
- `src-tauri/src/transcribe/mod.rs` — stub; chứa adapter và parser độc lập Job/DB/UI.
- `src-tauri/src/core/error.rs` — `AppError`/`Code` ổn định; kết quả Chunk cần retryability riêng.
- `_bmad-output/planning-artifacts/epics.md` — Story 2.2 AC gốc; `epic-2-context.md` — gate S2/OQ2 và giới hạn NFR-8.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/gemini/mod.rs`, `params.rs` — thêm POST body nhạy cảm, cổng request Job dùng consent/key pool/cancel/deadline 120 s, phân loại HTTP và retryable; test mock khóa method/path/header/body.
- [x] `src-tauri/src/transcribe/*` — profile model đã kiểm chứng, dựng hai request, parse response chịu lỗi, word grouping, normalize timestamp/gap metadata; test cạnh biên. Nhánh chuyên transcribe bị gate S2 trước khi gửi.
- [x] `src-tauri/src/media/chunk.rs` — kiểm budget serialized request thực khi dùng Chunk.
- [x] `_bmad-output/implementation-artifacts/adr-2-2-inline-spike.md` — ghi tài liệu/bằng chứng S2, trạng thái xác minh inline và gate OQ2.

**Acceptance Criteria:**
- Given model tổng quát có profile xác minh, when gửi Chunk, then POST generateContent chứa inline FLAC, schema mảng start/end/text và cấu hình thinking đúng profile.
- Given model chuyên transcribe có bằng chứng inline, when gửi Chunk, then POST interactions verbatim/word/language_codes, word gộp tối đa 15 s và giữ speaker.
- Given response lỗi một phần, when parse, then giữ mọi Segment hợp lệ không trùng, thời gian tuyệt đối không lùi và trả metadata phần thiếu đủ để retry.
- Given request lỗi/timeout/blocked, when hoàn tất, then kết quả không được coi là silence, retryability đúng loại và không có fan-out.

## Implementation Notes

- Gateway hỗ trợ POST JSON nhạy cảm và retry 5xx tuần tự trong cùng deadline 120 s; timeout và 400/404/blocked dừng.
- Parser cứu Segment hợp lệ từ JSON nội dung bị hỏng, gộp word annotation, lưu speaker và `unresolved` cho story 2.5. Full request JSON được đo dưới 20 MiB.
- Adapter nhận cả tên model trần lẫn dạng `models/<id>` từ model list; dispatch Interactions và parser word đã nối sau profile chuyên biệt, nhưng profile đó bị gate cho đến khi S2 có bằng chứng.
- S2 chưa xác minh bằng API thật vì không có Gemini API key. Model `gemini-3.5-transcribe` trả gate rõ ràng; alias mặc định `gemini-flash-lite-latest` cũng chưa có profile xác minh. Vì vậy trạng thái review được giữ, không coi story đã được chấp nhận hoàn toàn.

## Spec Change Log

## Review Triage Log

## Design Notes

Tài liệu Google hiện mô tả Interactions `input.audio.data` ở mức API chung nhưng ví dụ `gemini-3.5-transcribe` chỉ dùng URI từ Files API. Chưa có key thử S2 trong môi trường, nên profile inline cho model này phải là gate xác minh độc lập; không đánh dấu đã chứng minh bằng unit test request shape.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml --locked transcribe`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked gemini`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
- `cargo check --manifest-path src-tauri/Cargo.toml --locked`

**Results (2026-09-23):** `cargo test --locked` qua 158 unit và 4 media corpus tests. `cargo fmt --check`, `cargo check --locked` và `git diff --check` đều qua.
