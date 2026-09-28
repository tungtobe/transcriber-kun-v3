---
title: 'Story 4.9: Dừng Live và lưu Phiên'
type: 'feature'
created: '2026-09-28'
status: 'done'
baseline_commit: '2578d175d9e4bd3c93da48ae144520849f99f4c6'
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

**Problem:** Dừng Live hiện chỉ để Phiên ở `finalizing`; chưa tạo Proxy, chuyển `complete` hoặc mở chi tiết phiên. **Approach:** Hoàn tất đường dừng trong actor/IPC: flush Segment cuối, finalize WAV, tạo FLAC Proxy theo khả năng, commit metadata tối thiểu, trả `session_id`; UI flush Ghi chú, hiện overlay lưu rồi điều hướng tới Phiên.

## Boundaries & Constraints

**Always:** Dừng qua `live_stop` do `ipc/` điều phối, thành công trả đúng session ID. Thứ tự: flush Segment cuối → WAV finalize, status `finalizing` → Proxy FLAC nếu được → DB metadata tối thiểu và `complete`. Proxy lỗi vẫn hoàn tất Phiên với Recording/Segment và `proxy_ext=NULL`, UI vào nhánh “Không có audio” cho live. DB finalize lỗi để row ở trạng thái phục hồi được và báo storage; không báo lưu thành công giả. Lỗi finalize/Proxy không làm đổi complete/partial của transcript. Lỗi thiết bị audio dùng cùng đường lưu. Ghi chú đang gõ phải flush thành công trước khi gọi stop; nếu flush thất bại, giữ màn và cho thử lại. Overlay “Đang lưu phiên…” giữ Ad slot ẩn; thành công đi `/session/:id`.

**Never:** Xóa WAV/Segment vì Proxy lỗi; đánh dấu `complete` trước khi metadata tối thiểu commit; điều hướng khi stop/notes/DB thất bại; hiện relink file nguồn cho live thiếu Proxy.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Normal stop | Live có transcript và notes | Flush notes/Segment, WAV + FLAC, complete; trả ID và mở detail | Overlay trong lúc lưu |
| Proxy failure | WAV đã finalize | Phiên complete với WAV/segments, proxy NULL; detail no-audio | Lỗi category phù hợp, không làm transcript partial |
| Finalize/DB failure | WAV/DB lỗi sau khi bắt đầu lưu | Giữ dữ liệu bền và trạng thái phục hồi được | Không điều hướng/báo thành công giả |
| Device failure | Capture/Recording dừng bất ngờ | Cùng pipeline finalize và phiên xuất hiện trong thư viện | Báo lỗi đúng category nếu không thể hoàn tất |

</frozen-after-approval>

## Code Map

- `src-tauri/src/live/mod.rs` -- `LiveSessionHandle::stop`, `Command::Stop`, `finish_current`, `check_running_session`; hiện flush/bắt `finalizing` và phát `Final`, nhưng chưa Proxy/complete/ID result. Nhánh lỗi writer đã gọi `finish_current`.
- `src-tauri/src/live/recording.rs`, `src-tauri/src/core/paths.rs` -- `RecordingHandle::stop` finalize WAV và path theo session ID; giữ bytes đã có khi lỗi.
- `src-tauri/src/media/proxy.rs`, `src-tauri/src/library/store.rs` -- `create_proxy`, `publish_proxy` và staging/publish pattern; thêm finalize live tại store với transaction metadata.
- `src-tauri/src/db/repo/sessions.rs` -- `update_live_progress`, `set_proxy_ext`; bổ sung commit nguyên tử status/duration/proxy khi cần.
- `src-tauri/src/ipc/mod.rs`, `src/lib/bindings.ts` -- `live_stop` trả ID, regenerate binding.
- `src/routes/Live.svelte`, `src/components/NotesPanel.svelte`, `src/lib/stores/live.svelte.ts` -- `NotesPanel.flush()` trước stop, overlay/redirect; `/session/:id` trong `src/routes/Session.svelte` đã có LIVE badge.
- `src/routes/session/Player.svelte`, `src/routes/Session.svelte` -- live thiếu Proxy hiển thị no-audio không relink; file-session relink giữ hành vi cũ.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/library/store.rs`, `db/repo/sessions.rs`, `live/mod.rs` -- pipeline finalize, Proxy fallback, DB failure/recovery và tests.
- [x] `src-tauri/src/ipc/mod.rs`, `src/lib/bindings.ts` -- typed `live_stop` trả session ID; binding sinh tự động.
- [x] `src/routes/Live.svelte`, `src/lib/stores/live.svelte.ts` -- notes flush, overlay và điều hướng; device-stop event cùng flow.
- [x] `src/routes/session/Player.svelte`, `src/routes/Session.svelte`, i18n và tests -- nhánh no-audio cho live, giữ detail/notes/badge.

**Acceptance Criteria:**
- Given Live đang chạy, when Dừng, then Phiên complete với WAV/Proxy, Segments cuối và notes, và màn chi tiết mở đúng ID.
- Given Proxy thất bại, when Dừng, then Phiên vẫn complete, WAV/Segments còn và detail báo không có audio.
- Given DB finalize lỗi, when Dừng, then không báo thành công, row/WAV/Segment sẵn cho boot recovery.
- Given lỗi thiết bị audio, when actor dừng ghi, then đi cùng pipeline lưu và UI biết ID Phiên.

## Implementation Notes

- Actor flush batch cuối vào `finalizing`, finalize WAV, tạo Proxy best effort rồi commit duration/proxy/status `complete` nguyên tử. DB lỗi giữ row `finalizing` và file bền; device error dùng cùng đường nếu WAV finalize được.
- `live_stop` trả session ID. UI flush Notes trước stop, hiện overlay, điều hướng trên `Final` hoặc stop thành công; lỗi Notes giữ Live và cho retry. Player Live thiếu Proxy không hiện relink file nguồn.
- Review bổ sung mô tả bước finalize/Proxy trên overlay và test overlay khi stop còn đang chạy; reduced motion tắt spinner.

## Spec Change Log

## Review Triage Log

## Design Notes

Proxy FLAC là dẫn xuất của Recording WAV. Commit DB thành công là ranh giới để UI xem là đã lưu; Proxy lỗi vẫn có thể commit `complete` với `proxy_ext=NULL`. Boot recovery của 4.10 sẽ xử lý row `finalizing` còn lại sau force-quit.

## Verification

**Commands:**
- `cargo test --locked --manifest-path src-tauri/Cargo.toml live::` -- actor lifecycle tests qua.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml library::store::` -- Proxy/persistence tests qua.
- `npm run bindings && npm run check && npm test` -- typed IPC/UI/tests qua.
- `npm run check:i18n && npm run check:ui` -- locale/token qua.

**Results:** 51 Live tests, 58 store tests, 20 session repo tests, 633 frontend tests, Svelte/i18n/UI checks đều qua; sau review, 42 Live/Session tests và Rust suite đầy đủ (562 unit + 4 media tests) qua.
