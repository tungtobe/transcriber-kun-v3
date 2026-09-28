---
title: 'Story 4.7: Màn Live — bắt đầu và đang ghi'
type: 'feature'
created: '2026-09-28'
status: 'done'
baseline_commit: 'eec3cbbd23eef41aec50e5b3daed82ec3fa54049'
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

**Problem:** LiveSession chạy được nhưng người dùng chưa có màn để bắt đầu, xem transcript và trạng thái ghi âm. **Approach:** Thêm route Live với setup nguồn, Tag, ngôn ngữ, trạng thái phiên đang chạy và lối vào từ Home/sidebar; dùng IPC typed và snapshot/event của 4.6.

## Boundaries & Constraints

**Always:** Ba lựa chọn nguồn mixed/system/mic; mixed có mic dropdown và refresh. Chỉ chặn nguồn có quyền đã bị OS từ chối; trạng thái chưa hỏi được phép bắt đầu để OS prompt. Thiếu key thì nút bắt đầu disabled với tooltip và link Settings. Tag gắn nguyên tử khi tạo session; lỗi mở capture không để lại session/tag rác. Subscribe lại khi remount, seq hụt thì lấy snapshot mới; không tạo phiên thứ hai. Hai pill Recording và connection riêng. Transcript tự cuộn, dừng khi người dùng cuộn lên; Notes mở mặc định và tự lưu. Phím tắt dùng keymap nội bộ, chống key repeat/gate dialog. Live ẩn Ad slot và hỗ trợ reduced motion.

**Never:** Hiện đang transcribe chỉ vì đang ghi âm; ghi nội dung/key vào log; coi permission chưa hỏi là bị từ chối; gắn Tag bằng lệnh hậu tạo phiên; để điều hướng khỏi Live làm mất phiên đang chạy.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Setup | Mixed/system/mic, Tag, language | Start một session, Tag đúng session; chuyển màn ghi | Thiếu key/quyền đã từ chối chỉ chặn lựa chọn liên quan |
| Recording | ready/delta/segment/gap/connection | Hai pill riêng, transcript/gap đúng thứ tự, notes tự lưu | Event thiếu seq subscribe lại; lỗi có category |
| Remount | Rời và trở lại `/live` | Snapshot khôi phục cùng session | Không gọi start lần nữa |
| Shortcut | Meta/Control+Shift+L, giữ phím | Một hành động start/stop cho một lần nhấn | Không vượt dialog/gate |

</frozen-after-approval>

## Code Map

- `src/lib/router.ts`, `src/routes/Live.svelte` -- route mới, setup và màn đang ghi; `src/components/AppShell.svelte`, `src/routes/Home.svelte` đang có placeholder Live disabled.
- `src/lib/stores/live.svelte.ts` -- store singleton dùng `commands.liveSubscribe` và `Channel<LiveEvent>`; `src/lib/bindings.ts` có LiveSnapshot/ConnectionState/RecordingState.
- `src/components/NotesPanel.svelte`, `TagPicker.svelte`, `src/lib/stores/{notes,library,keys,settings}.svelte.ts` -- reuse notes, Tag picker, key và language.
- `src-tauri/src/{ipc,live}/mod.rs`, `src-tauri/src/library/store.rs`, `src-tauri/src/live/recording.rs` -- mở rộng start nhận TagIds và tạo liên kết cùng transaction session; giữ thứ tự mở capture trước DB.
- `src-tauri/src/audio/mod.rs`, `src-tauri/src/ipc/mod.rs` -- bổ sung trạng thái quyền và mở OS Settings/recheck nếu API hiện tại chưa phân biệt denied/unasked.
- `src/lib/keymap.ts`, `src/i18n/{vi,en,ja}.json` -- shortcut và copy; `src/routes/session/SegmentList.svelte` chỉ tham khảo timestamp/style vì gắn playback.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/{audio,ipc,live}/mod.rs`, `src-tauri/src/library/store.rs` -- permission state, typed start TagIds và insert tag atomically; regenerate bindings.
- [x] `src/lib/stores/live.svelte.ts`, `src/routes/Live.svelte` -- subscription, setup, transcript, notes, connection/recording display và source switch.
- [x] `src/components/AppShell.svelte`, `src/routes/Home.svelte`, `src/lib/router.ts`, `src/lib/keymap.ts` -- lối vào Live, chỉ báo active, shortcut.
- [x] `src/i18n/{vi,en,ja}.json`, tests liên quan -- copy đủ ba locale và edge cases matrix.

**Acceptance Criteria:**
- Given Home/sidebar, when chọn Live, then tới `/live` với setup hoặc phiên đang chạy tương ứng.
- Given nguồn chưa xin quyền, when bắt đầu, then prompt OS có thể xuất hiện; given một nguồn đã denied, when chọn nguồn khác, then vẫn bắt đầu được.
- Given session đang ghi, when kết nối đổi hoặc transcript stream, then pill và Segment cập nhật riêng, đúng thứ tự; notes lưu bền.
- Given rời rồi quay lại, when subscribe, then hiển thị phiên hiện hành; given giữ phím tắt, then chỉ một lệnh chạy.

## Implementation Notes

- Route `/live` dùng store singleton và Channel snapshot để tiếp tục phiên khi remount; setup có radio nguồn, Tag picker, language và permission recheck.
- Tag được kiểm tra và gắn trong cùng transaction tạo session; lỗi Tag rollback row. macOS nhận biết trạng thái mic qua AVFoundation; system audio và Windows để `unknown` nhằm cho phép prompt đầu tiên.
- Test WAV checkpoint được chỉnh nhịp gửi mẫu để ngưỡng kích thước kiểm được trước mốc checkpoint theo thời gian.
- Review bổ sung Live CTA ở header Home, thông báo quyền audio trước khi bắt đầu, `button-lg` và dấu ẩn Ad slot cho route Live.

## Spec Change Log

## Review Triage Log

## Design Notes

4.7 cần nút Dừng và phím tắt; đường finalize và điều hướng hoàn chỉnh thuộc 4.9. 4.8 bổ sung banner reconnect/setup. Hiển thị `Đang nghe…` lúc chưa có lời và caret tĩnh khi reduced motion.

## Verification

**Commands:**
- `npm run check && npm test` -- UI/types/tests qua.
- `npm run check:i18n && npm run check:ui` -- locale và token qua.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml` -- backend qua.

**Results:** `npm run check`, `npm test` (624 tests), `npm run check:i18n`, `npm run check:ui`, `cargo fmt -- --check`, `cargo test --locked --manifest-path src-tauri/Cargo.toml` (556 unit + 4 media tests) đều qua.
Sau review: `npm run check`, `npm run check:i18n`, `npm run check:ui`, và 40 test Live/Home đều qua.
