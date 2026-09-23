---
title: '2.2 Gemini transcribe file bằng model tổng quát và parser chịu lỗi'
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

<frozen-after-approval reason="human-owned intent — updated by user's 2026-09-23 correction">

## Intent

**Problem:** Chunk FLAC cần được transcribe chính xác khi tiếng Việt và tiếng Nhật xen kẽ, với model Gemini thông thường tiết kiệm chi phí hơn theo lựa chọn của người dùng. Thiết kế trước nhầm model `*-transcribe`/Interactions vào đường file và chặn alias mặc định.

**Approach:** Gửi từng Chunk qua `generateContent` với FLAC inline, prompt nhận diện ngôn ngữ tự động và JSON Segment; cứu phần hợp lệ từ response lỗi. Live Translate là luồng WebSocket riêng của Epic 4–5.

## Boundaries & Constraints

**Always:** Kiểm toàn request JSON dưới 20 MB, deadline 120 s kể cả chờ key, retry 5xx tuần tự qua gateway, không fan-out. Giữ Segment hợp lệ, offset tuyệt đối và metadata phần thiếu. Alias/tên tự nhập dùng thinking mặc định, không đoán version.

**Never:** Không dùng Interactions, model `*-transcribe` hay Files API cho transcribe file. Không tự đổi model, không dịch nội dung file, không log audio/transcript/key/body response.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| General | FLAC và model generateContent, có tiếng Việt–Nhật | JSON schema start/end/text, prompt giữ ngôn ngữ gốc | 400/404/blocked dừng |
| Alias/custom | `gemini-flash-lite-latest` hoặc tên tự nhập | Gửi đúng tên model, không gán thinking config theo version đoán | API từ chối thì báo lỗi model/request |
| Damaged result | JSON cắt/fence, timestamp sai/trùng | Giữ Segment hợp lệ và phần chưa cứu được | Response rỗng/lỗi không phải silence |

</frozen-after-approval>

## Code Map

- `src-tauri/src/gemini/mod.rs`, `keys.rs`, `params.rs` — gateway POST nhạy cảm, consent/key pool/deadline/retry.
- `src-tauri/src/media/chunk.rs` — Chunk FLAC base64, budget và serialized request limit.
- `src-tauri/src/transcribe/adapter.rs`, `parser.rs` — request generateContent và chuẩn hoá Segment.
- `_bmad-output/planning-artifacts/epics.md` và PRD FR-14 — nguồn yêu cầu đã cập nhật theo lời người dùng.

## Tasks & Acceptance

**Execution:**
- [x] Gateway POST Job, 120 s và retry 5xx; test mock request và timeout.
- [x] Adapter generateContent cho model version, alias và tên tự nhập; prompt Việt–Nhật, JSON schema và size check.
- [x] Parser cứu Segment hợp lệ, absolute timestamp, metadata gap và silence rõ nghĩa.
- [x] Gỡ đường `*-transcribe`/Interactions khỏi file, cập nhật ADR và nguồn yêu cầu.
- [ ] S2 test API thật với model tổng quát và fixture Việt–Nhật.

**Acceptance Criteria:**
- Given model tổng quát, when gửi Chunk, then POST generateContent có FLAC inline, JSON schema và prompt nhận diện Việt–Nhật không dịch.
- Given alias hoặc tên tự nhập, when gửi Chunk, then giữ nguyên model và không đoán thinking config từ tên.
- Given response lỗi một phần, when parse, then giữ Segment hợp lệ, không trùng/lùi thời gian và báo phần thiếu.
- Given HTTP lỗi hoặc timeout, when hoàn tất, then retryability đúng loại và không coi response lỗi là silence.

## Implementation Notes

- Điều chỉnh theo code v2 và chỉ dẫn trực tiếp của người dùng sau merge `a3ad44d`. Model Live Translate thuộc Live, không thuộc transcribe file.
- S2 API thật chưa chạy vì môi trường không có key; trạng thái vẫn `in-review`.

## Spec Change Log

- 2026-09-23: Người dùng sửa định hướng FR-14: file dùng generateContent model tổng quát, Live dùng Live Translate. Bỏ Interactions/ASR khỏi story và mở alias mặc định bằng thinking config của model.

## Review Triage Log

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml --locked`
- `cargo check --manifest-path src-tauri/Cargo.toml --locked`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`

**Results (2026-09-23):** 156 unit tests và 4 media corpus tests qua; `cargo check`, `cargo fmt --check`, `git diff --check` qua. S2 API thật chưa chạy vì không có key trong môi trường.
