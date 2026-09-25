---
title: '2.10 Tìm trong transcript, export và copy'
type: 'feature'
created: '2026-09-25'
status: 'done'
baseline_commit: '91ccdaea3f5cea7d04a9a642486b2adba185cc98'
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

**Problem:** Người dùng chưa thể tìm câu nói trong Transcript dài, copy toàn bộ nội dung hoặc xuất bản gửi cho người khác.

**Approach:** Thêm tìm kiếm trực tiếp trong Transcript detail và hành động Copy/Export cho Transcript đang chọn; xuất `.txt`, `.srt`, `.json` qua dialog lưu hệ thống, giữ metadata cần thiết và áp dụng offset chỉ lúc trình bày.

## Boundaries & Constraints

**Always:** `⌘F`/`Ctrl+F` focus ô tìm trên Transcript detail; query trim/gộp khoảng trắng, không phân biệt hoa thường; hiện `n/N`, highlight màu `mark`, cuộn tới match; Enter/Shift+Enter và prev/next đi vòng, ≤100 ms với khoảng 700 Segment. Query rỗng/không khớp hiện `0/0`, nút điều hướng vô hiệu, không cuộn. Không bắt Enter trong ghi chú/tên phiên. Tìm kiếm và export/copy dùng Transcript đang chọn (hiện tại là `detail.transcript`, primary); mọi thao tác dùng bàn phím được. Offset là `max(0, t + offset)` chỉ ở UI/export, không sửa DB hoặc thời gian seek. Export qua dialog hệ thống; cancel im lặng. JSON giữ speaker ẩn, metadata offset, tất cả gap; TXT và Copy ghi chú gap; SRT chỉ cue text hợp lệ, bỏ cue có `end ≤ start` sau chuẩn hoá và đánh số lại; thông báo riêng nếu SRT bỏ gap. Copy toàn bộ transcript vào clipboard; toast thành công `aria-live=polite`, tự tắt sau 4 giây. Lỗi ghi/clipboard hiển thị gần nút, không báo thành công giả.

**Never:** Không tìm ở Home, không đổi nội dung gốc hay phát Gemini, không dùng đường dẫn do frontend tự ghép; không thêm capability ghi file cho WebView. Không đưa speaker lên UI transcript.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Search | `  giao  diện `, text `Giao diện` | Một match, highlight + `1/1`, cuộn; next/prev vòng | Rỗng/không match: `0/0`, không cuộn |
| Export TXT | Transcript partial có `chunk_failed` | Timestamp offset, nội dung + ghi chú khoảng thiếu | Huỷ dialog im lặng; ghi lỗi báo inline |
| Export SRT | Offset khiến cue đầu `end ≤ start`; có gap | Bỏ cue hỏng, cue còn lại đánh từ 1; UI báo có gap | Không tạo cue cho gap |
| Export JSON | Segment có speaker; có gap | Speaker, offset metadata và gap còn nguyên | Không lộ path lưu vào UI |
| Copy | Transcript có gap | Copy text và ghi chú gap; toast 4 s | Clipboard lỗi báo inline |

</frozen-after-approval>

## Code Map

- `src/routes/Session.svelte` -- saved view sở hữu Transcript hiện chọn, Player và layout; gắn toolbar, search state và hành động, giữ seek raw.
- `src/routes/session/SegmentList.svelte` -- render text/gap, `registerRow` và `scrollIntoView`; nhận match index, highlight `mark` và cuộn match; giữ auto-scroll playback.
- `src/lib/time.ts` -- `formatTimestamp`/`displayTimestamp` clamp offset; thêm helper mili-giây cho SRT nếu cần; `settingsStore.timestampOffsetSec` là nguồn offset.
- `src/lib/keymap.ts` -- registry phím tắt ở WebView; dùng shortcut trong route, tránh hijack ô nhập khác.
- `src-tauri/src/db/repo/segments.rs` -- `list_for_transcript` trả `speaker` và gap; UI detail cố ý giấu speaker.
- `src-tauri/src/library/store.rs` -- đọc Transcript theo id/phiên, xác thực ownership; thêm dữ liệu export qua DB, không dùng `SegmentDetail` thiếu speaker.
- `src-tauri/src/ipc/mod.rs` -- `diagnostics_export`/`save_diagnostics_bundle` là mẫu `rfd` main thread + `spawn_blocking`; command export mới nhận transcript id + định dạng + offset, đăng ký `specta_builder` và tái sinh `src/lib/bindings.ts`.
- `src-tauri/src/library/mod.rs` -- module export mới cho formatter thuần và test golden TXT/SRT/JSON; không thêm plugin/capability.
- `src/i18n/{vi,en,ja}.json` -- nhãn tìm, export, copy, thông báo gap, toast và lỗi; `src/styles/tokens.css` có màu mark hai theme.
- `src/routes/Session.test.ts`, `src/routes/session/SegmentList.test.ts`, `src/lib/time.test.ts` -- mẫu test Vitest/jsdom hiện tại.

## Tasks & Acceptance

**Execution:**
- [ ] `src-tauri/src/library/export.rs` và `src-tauri/src/library/mod.rs` -- đọc đúng Transcript được chọn, formatter TXT/SRT/JSON với offset/gap/speaker, test golden và chuẩn hoá cue.
- [ ] `src-tauri/src/ipc/mod.rs` -- command export an toàn qua save dialog Rust, cancel/success/error, sinh `src/lib/bindings.ts`.
- [ ] `src/lib/transcript-search.ts` (+test) -- chuẩn hoá query, tính match text nhanh và index vòng.
- [ ] `src/routes/session/SegmentList.svelte` (+test) -- highlight mark, cuộn match hiện hành, giữ click-to-seek/autoscroll.
- [ ] `src/routes/Session.svelte` (+test) -- toolbar tìm, prev/next, shortcut, export và copy, toast 4 s, lỗi inline, transcript hiện chọn.
- [ ] `src/i18n/{vi,en,ja}.json` -- thêm mọi nhãn và thông báo mới, kiểm tra đồng bộ.

**Acceptance Criteria:**
- Given khoảng 700 Segment, when gõ query có chữ hoa và khoảng trắng thừa, then kết quả cập nhật trong ≤100 ms và prev/next/Enter/Shift+Enter vòng đúng.
- Given Transcript có gap, speaker và offset, when export ba định dạng hoặc Copy, then nội dung khớp ma trận và thời gian DB/seek không đổi.
- Given chỉ dùng bàn phím, when mở tìm, chuyển match, chọn export hoặc copy, then không cần chuột và focus thấy rõ.

## Implementation Notes

- Export đọc transcript theo ID trực tiếp từ DB để giữ `speaker`; dialog và ghi file chỉ chạy phía Rust. Copy lấy các Segment đang hiển thị và dùng chung quy tắc timestamp của UI.
- Đã sửa vòng render do search effect tự phụ thuộc state của nó. Offset âm được chuẩn hoá theo từng timestamp và test SRT/JSON; toast được test tại mốc 3.999/4.000 ms.
- Kiểm chứng: `npm test` 367/367; `cargo test --locked` 320 unit + 4 integration; `cargo fmt --check`, `npm run check`, `check:i18n`, `check:ui`, `check:deps` đều pass. Dialog native chưa được thao tác thủ công trong app đóng gói.

## Spec Change Log

## Review Triage Log

## Design Notes

Giữ formatting export phía Rust để đọc `speaker` trực tiếp từ DB và không cấp quyền file cho frontend. Frontend chỉ gửi ID Transcript đang hiển thị, format và offset. Copy có thể tạo TXT từ các Segment đang hiển thị vì không cần speaker. SRT dùng mili-giây và chuẩn `HH:MM:SS,mmm`, không làm tròn giây như `formatTimestamp` của UI.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml --locked` -- formatter, DB và bindings pass
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` -- sạch
- `npm test && npm run check && npm run check:i18n && npm run check:ui && npm run check:deps` -- pass
