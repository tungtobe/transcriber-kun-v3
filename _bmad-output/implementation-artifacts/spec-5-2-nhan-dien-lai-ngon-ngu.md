---
title: 'Story 5.2: Nhận diện lại ngôn ngữ'
type: 'feature'
created: '2026-09-30'
status: 'done'
route: 'full'
route_source: 'auto'
review: 'none'
review_source: 'pinned'
lenses_ran: []
review_loop_iteration: 0
context:
  - '{project-root}/_bmad-output/implementation-artifacts/epic-5-context.md'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** Khi khách đổi ngôn ngữ giữa họp và nguồn là `auto`, model đã "chốt" ngôn ngữ cũ nên dịch/transcript kém, mà người dùng không có cách bắt lại ngoài dừng phiên. **Approach:** Nút "Nhận diện lại" mở generation mới với ngữ cảnh trống (không resumption handle), chờ `setupComplete`, swap sender, đóng kết nối cũ ≤ 1 s, tái dùng máy swap generation của story 5.1.

## Boundaries & Constraints

**Always:** Tái dùng helper swap của 5.1 (một đường code duy nhất, serialize cùng với đổi Target: chỉ lựa chọn mới nhất còn hiệu lực). Nút chỉ hiện/dùng được khi ngôn ngữ của phiên là `auto`; ngôn ngữ khác thì ẩn hoặc vô hiệu. Generation mới không dùng resumption handle cũ. Phiên, Recording, transcript đã có giữ nguyên; timestamp theo sample clock, không nhảy, không Segment lặp. Câu đang stream dở ở generation cũ không được mất và không nhân đôi (flush phần câu dở của generation cũ vào sentence buffer trước khi bỏ event cũ, hoặc tương đương, có test). Event mang generation cũ sau swap bị bỏ. Thất bại: giữ kết nối cũ, banner nhẹ tại chỗ, phiên không gián đoạn. Dừng huỷ candidate. Copy vi/en/ja.

**Never:** Đóng kết nối cũ trước khi mới sẵn sàng; dừng/khởi động lại Recording; thêm code path swap thứ hai; lộ chi tiết lỗi raw.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Nhận diện lại | lang=auto, đang chạy | Generation mới sau setupComplete, cũ đóng ≤1s, transcript liền | — |
| Ngôn ngữ cố định | lang=ja | Nút ẩn/vô hiệu; lệnh backend bị từ chối | Trả lỗi typed |
| Kết nối mới lỗi | setup fail/deadline | Giữ cũ, banner nhẹ | Phiên không gián đoạn |
| Stream dở | Đang có câu dở khi swap | Không mất, không nhân đôi câu | — |
| Event cũ | Sau swap | Bị bỏ | — |
| Đồng thời đổi Target | Redetect + SetTarget | Tuần tự, mới nhất thắng | Candidate cũ huỷ |

</frozen-after-approval>

## Code Map

- `src-tauri/src/live/mod.rs` -- swap generation từ 5.1 (`set_target`/helper), `Command`, `LiveSessionHandle`, sentence buffer trong `RunningSession`; thêm `Command::Redetect` dùng chung helper (handle=None).
- `src-tauri/src/ipc/mod.rs`, `src/lib/bindings.ts` -- `live_redetect`, đăng ký, `npm run bindings`.
- `src/lib/stores/live.svelte.ts`, `src/routes/Live.svelte` -- `redetect()`, nút "Nhận diện lại", banner nhẹ khi lỗi.
- `src/i18n/{vi,en,ja}.json` -- copy mới.

## Tasks & Acceptance

**Execution:**
- [x] `live/mod.rs` -- `Redetect` qua helper swap, từ chối khi ngôn ngữ ≠ auto, xử lý câu dở, tests transport giả (thành công, lỗi giữ cũ, stream dở, event cũ, đồng thời với SetTarget)
- [x] `ipc`, bindings -- `live_redetect`
- [x] UI + store + i18n + tests

**Acceptance Criteria:**
- Given auto và đang Live, when bấm Nhận diện lại, then generation đổi mà Recording/transcript/timestamp liền mạch.
- Given kết nối mới lỗi, when swap, then kết nối cũ giữ và hiện banner nhẹ.

## Implementation Notes

- `set_target` and `redetect` both call one `begin_swap(target, redetect, reply)`; `Candidate.redetect` suppresses the `Target` event on promote. A new gateway run never carries a resumption handle, so the context is empty.
- Backend rejects redetect (typed `Request` error) when no session, transcription not active, or `swap.language != Auto`.
- Open translated sentence is flushed once by the existing `take_translation_tail` at promote; the source sentence buffer lives on `RunningSession`, so it is kept across generations without duplication. Old-generation events are dropped by the generation check.
- UI shows the button when `isRunning && language === 'auto'` (setup-selected language; no snapshot field added). Failure shows a light inline banner with no raw detail.

## Spec Change Log

## Review Triage Log

## Verification

**Commands:**
- `npm run bindings && cargo test --locked --manifest-path src-tauri/Cargo.toml` -- pass
- `npm run check && npm test && npm run check:i18n && npm run check:ui && npm run check:deps` -- pass
