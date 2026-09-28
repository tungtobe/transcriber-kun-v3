---
title: 'Story 4.8: Live — mất kết nối, lỗi setup và trạng thái lỗi trên giao diện'
type: 'feature'
created: '2026-09-28'
status: 'done'
baseline_commit: '0fd8805206d8ca12e232f119902265191c17ec37'
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

**Problem:** Khi mạng hoặc setup Gemini lỗi, người dùng chưa thấy rõ WAV vẫn ghi và chưa thể chọn chỉ ghi âm sau năm lần setup bị từ chối. **Approach:** Nối tín hiệu lỗi/reconnect của gateway và LiveSession vào màn Live với banner tại chỗ, đồng hồ chờ, gap trong transcript và lựa chọn chỉ ghi âm hoặc dừng.

## Boundaries & Constraints

**Always:** Recording và connection tiếp tục là hai state độc lập; mất mạng giữ WAV, pill ghi/đồng hồ chạy tiếp và pill reconnect có wall-clock m:ss chỉ dùng hiển thị. Gateway tự reconnect theo backoff hiện có, không cần người dùng bấm. Gap vượt ring 60 s đặt inline đúng thứ tự thời gian dạng “Mất kết nối mm:ss–mm:ss”. Sau năm SetupRejected liên tiếp hiện lựa chọn “Tiếp tục chỉ ghi âm”/“Dừng”; chọn chỉ ghi âm là quyết định của phiên, dừng mọi reconnect, ghi gap `disconnected` từ lúc transcript ngừng tới lúc Dừng, giữ WAV và hướng dẫn Transcribe lại. Snapshot/remount phải khôi phục trạng thái lựa chọn. Model/quota/auth có banner category tại chỗ và hành động phù hợp; không đổi model. Mọi lỗi chỉ render copy tĩnh, không lộ key/URL/transcript. Icon và chữ phân biệt trạng thái, không phụ thuộc màu.

**Never:** Dừng WAV vì mạng/Gemini lỗi; tự tiếp tục gọi Gemini sau khi chọn chỉ ghi âm; lấy wall-clock làm timestamp gap; dùng toast cho quota/auth; đưa `detailRedacted` hoặc log raw lên UI.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Outage/recovery | Socket rớt rồi kết nối lại | Recording/timer tiếp tục, reconnect m:ss và banner; transcript tự tiếp tục | Gap ring overflow inline theo sample clock |
| Setup rejected | Năm lần từ chối | Banner danger có hai lựa chọn; chỉ ghi âm giữ WAV và dừng Gemini | Gap cuối kéo tới lúc stop; remount giữ quyết định |
| Category errors | model/quota/auth | Banner tại chỗ có lối tắt Settings/hành động phù hợp | Không hiện raw detail/key/URL/transcript |

</frozen-after-approval>

## Code Map

- `src-tauri/src/gemini/live/mod.rs` -- `LiveGateway::run` đã reconnect vô hạn với backoff tối đa 30s, ring 600 chunks, `LiveFailure::SetupRejected` sau 5 lần; giữ các contract này.
- `src-tauri/src/live/mod.rs` -- actor có `ConnectionState`, `LiveSnapshot`, events, `handle_internal`, `finish_current`; hiện flatten SetupRejected thành model error và không ghi gap tail. Thêm state/command phiên chỉ ghi âm và tín hiệu setup riêng.
- `src-tauri/src/ipc/mod.rs`, `src/lib/bindings.ts` -- thêm command typed, regenerate binding qua `export_bindings`.
- `src/lib/stores/live.svelte.ts`, `src/routes/Live.svelte` -- UI hiện có từ 4.7; nâng snapshot/event và banner/gap/timer; `src/lib/errors.ts` có copy category/model helper.
- `src/i18n/{vi,en,ja}.json` -- copy mới; không đưa lỗi raw vào UI.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/live/mod.rs`, `src-tauri/src/ipc/mod.rs`, `src/lib/bindings.ts` -- trạng thái SetupRejected/recording-only bền trong actor snapshot, command continue, tail gap sample-clock và tests.
- [x] `src/lib/stores/live.svelte.ts`, `src/routes/Live.svelte` -- reconnect banner/timer, inline gap, setup choice, category banners.
- [x] `src/i18n/{vi,en,ja}.json`, UI tests -- copy ba ngôn ngữ và matrix tests.

**Acceptance Criteria:**
- Given Live đang ghi và socket rớt, when reconnect, then pill ghi chạy liên tục, pill kết nối đếm m:ss, transcript tự trở lại.
- Given gap do buffer vượt 60 s, when event/DB phục hồi, then dòng gap nằm đúng thời điểm và hiển thị mốc đầu/cuối.
- Given SetupRejected thứ năm, when chọn chỉ ghi âm, then không còn lần gọi Gemini nào, Recording giữ đến Dừng và gap cuối bền; when remount, then vẫn hiển thị transcript đã dừng.
- Given lỗi model/quota/auth, when hiển thị, then có hành động phù hợp và không lộ dữ liệu nhạy cảm.

## Implementation Notes

- Actor lưu `transcription` và category trong snapshot; sau SetupRejected, lệnh chỉ ghi âm hủy gateway cho phiên đó nhưng giữ capture/Recording. Khi dừng, gap tail được đóng theo sample clock và ghi cùng batch cuối.
- Store chỉ giữ category lỗi, không giữ detail; UI dùng copy tĩnh, đồng hồ reconnect chỉ để hiển thị. Review bổ sung pill “Transcript đã dừng” và dòng gap nền chìm đúng vị trí.

## Spec Change Log

## Review Triage Log

## Design Notes

Gateway đã kết thúc sau SetupRejected; quyết định “chỉ ghi âm” cần actor state rõ ràng để UI không coi đây là lỗi thông thường. Dùng sample clock capture cho gap, còn bộ đếm chờ reconnect chỉ là trình bày.

## Verification

**Commands:**
- `cargo test --locked --manifest-path src-tauri/Cargo.toml live::` -- actor/gap tests qua.
- `npm run bindings && npm run check && npm test` -- binding/UI/tests qua.
- `npm run check:i18n && npm run check:ui` -- locale/token qua.

**Results:** 50 Rust `live::` tests, 629 frontend tests, binding generation, Svelte check, i18n/UI checks đều qua. Sau review, 13 Live UI/store tests và các check liên quan đều qua.
