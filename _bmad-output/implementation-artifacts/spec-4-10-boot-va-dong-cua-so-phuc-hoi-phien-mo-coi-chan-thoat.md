---
title: 'Story 4.10: Boot và đóng cửa sổ — phục hồi Phiên mồ côi, chặn thoát'
type: 'feature'
created: '2026-09-28'
status: 'done'
baseline_commit: '273ad9ebc13bbb41ea9cf29936e9fb8760438f5d'
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

**Problem:** Force-quit để lại Live ở `recording`/`finalizing`, hiện boot không phục hồi; đóng app khi Live chạy có thể thoát mà chưa lưu. **Approach:** Boot phục hồi nền từ WAV/DB bền và tạo lại Proxy; mọi cách đóng app đi qua một coordinator xác nhận, lưu Live và hủy Job trước khi thoát.

## Boundaries & Constraints

**Always:** `ipc/boot` là chủ boot duy nhất: migrate DB → hook phục hồi Ducking marker (Epic 5 thực hiện) → dọn `.staging` → khởi chạy recovery nền, Home không chờ encode (mục tiêu ≤2 s). Recovery chỉ chạm Live `recording|finalizing`, giữ WAV phát được (mất tối đa 5 s cuối nhờ checkpoint), Segment/Ghi chú đã flush, commit `complete,recovered=true`, duration và Proxy nếu tạo được; chạy lại idempotent. Phiên đang recover phải busy với delete/rerun; kiểm tra row/revision trước publish/commit để không hồi sinh phiên đã xóa. Thoát tự nguyện qua nút cửa sổ, menu hoặc CmdQ dùng cùng `ipc/close`, hỏi một lần nếu Live/Job bận; đồng ý thì flush Ghi chú, finalize WAV và metadata tối thiểu, hủy Job, rồi thoát. Không chờ Proxy vô hạn: Proxy chậm/lỗi có thể để thiếu cho boot sau; lỗi lưu metadata phải báo và giữ app mở.

**Never:** Chặn Home để encode; xóa WAV/Segment/notes vì Proxy hỏng; tạo bản Transcript trùng khi recovery lặp; thoát tự nguyện sau lỗi lưu tối thiểu; giả định SIGKILL/OS shutdown chạy được dialog.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|---------------|----------------------------|----------------|
| Crash recovery | Live `recording` hoặc `finalizing`, WAV checkpoint + Segment | Boot tới Home nhanh; nền mark `complete,recovered=true`, WAV nghe được, Proxy tạo nếu được | Proxy lỗi vẫn giữ dữ liệu và proxy NULL; DB lỗi còn row recoverable |
| Repeat/race | Boot lại, hoặc xóa/rerun khi encode | Không nhân đôi Segment; thao tác xung đột báo busy; recheck row trước publish | Không resurrect row/file sau delete |
| Close Live | Live chạy, cửa sổ/menu/CmdQ | Một dialog “Phiên đang ghi — dừng và lưu trước khi thoát?”; xác nhận flush notes, lưu WAV/metadata rồi thoát | Chọn ở lại không dừng; storage lỗi báo và không thoát |
| Close Live + Job | Cả hai bận | Một lần xác nhận; Live lưu, Job hủy sạch | Proxy chậm không giữ app vô hạn |
| OS kill | Process bị SIGKILL lúc Live giả chạy | Mở lại thấy phiên WAV/Segment trong thư viện | Không trông cậy close handler |

</frozen-after-approval>

## Code Map

- `src-tauri/src/ipc/boot.rs` -- boot mở DB/migrate, hiện `reconcile` đồng bộ; tách cleanup/staging khỏi encode recovery và thêm recovery busy state. Chưa có Ducking implementation, chỉ hook đúng thứ tự.
- `src-tauri/src/library/store.rs`, `src-tauri/src/db/repo/sessions.rs` -- `reconcile`, `finalize_live_session`, proxy staging/publish và conditional DB updates; thêm recovery idempotent, row/revision recheck, Proxy fallback. Không thay transcript/notes.
- `src-tauri/src/live/recording.rs` -- WAV checkpoint mỗi 160 KB hoặc 5 s đã có force-kill test; dùng header bền/validate khi recover, không kéo tail chưa flush vào duration.
- `src-tauri/src/live/mod.rs` -- `LiveSessionHandle::stop` đã flush Segment/finalize WAV; cần trạng thái Live cho close và đường lưu tối thiểu không chờ encode vô hạn.
- `src-tauri/src/ipc/mod.rs`, `src-tauri/src/lib.rs` -- close hiện chỉ nhìn Job và `app_close_confirm` hủy Job rồi exit; đưa coordinator vào `ipc/close`, route window/menu/CmdQ chung, chống re-entry, typed event/bindings.
- `src/components/CloseConfirm.svelte`, `src/lib/stores/app.svelte.ts`, `src/i18n/{vi,en,ja}.json` -- flush notes, dialog Job hiện có; mở rộng Live và cả hai, lỗi lưu giữ app mở.
- `src/routes/home/SessionRow.svelte` -- badge recover đã có; thêm gợi ý Transcribe lại; chi tiết phiên đã có badge/đường rerun.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/db/repo/sessions.rs`, `src-tauri/src/library/store.rs` -- recovery có transaction/guard, WAV/Proxy fallback, idempotency và race tests.
- [x] `src-tauri/src/ipc/boot.rs` -- thứ tự boot, cleanup nhanh, recovery nền và busy state; test boot không chặn và orphan recovery.
- [x] `src-tauri/src/live/mod.rs`, `src-tauri/src/ipc/close.rs`, `src-tauri/src/ipc/mod.rs`, `src-tauri/src/lib.rs` -- query Live, một close coordinator, bounded Proxy, Job cancel, typed event và tests.
- [x] `src/components/CloseConfirm.svelte`, `src/lib/stores/app.svelte.ts`, `src/lib/bindings.ts`, `src/routes/home/SessionRow.svelte`, i18n/tests -- một dialog, notes/error flow, recover hint.

**Acceptance Criteria:**
- Given force-kill sau checkpoint, when boot lại, then Home mở không chờ Proxy và thư viện hiển thị phiên recover với WAV/Segment/Ghi chú đã lưu.
- Given user đóng cửa sổ hoặc CmdQ lúc Live/Job bận, when xác nhận, then dữ liệu Live tối thiểu đã bền và Job được hủy trước exit; lỗi storage giữ app mở.
- Given recovery lặp hoặc tranh với delete/rerun, when publish, then không trùng dữ liệu hay hồi sinh phiên đã xóa.

## Implementation Notes

- Boot đổi tên `.staging` ra khỏi namespace writer trước khi khởi động actor; xóa cây cũ và scan media/recovery chạy nền. Chỉ ứng viên chụp trước lúc actor chạy mới được phục hồi; event typed làm Home tải lại sau commit.
- Recovery đọc độ dài WAV đã checkpoint, giữ đúng số kênh, thử Proxy qua staging, CAS theo row/revision rồi commit `complete,recovered=true`; phiên complete thiếu Proxy được thử lại boot sau. Delete/rerun thấy busy trong lúc recovery.
- Close dùng một coordinator cho window/menu/CmdQ. Khi xác nhận, Live flush/finalize WAV và commit metadata không đợi encode Proxy; sau đó Job được hủy và app mới thoát. Lỗi lưu giữ app mở và cho thử lại.
- Diff audit bổ sung test busy gate, Proxy publish failure và DB commit failure. Không có công việc bị hoãn trong phạm vi story.

## Spec Change Log

## Review Triage Log

## Design Notes

`complete,recovered=true,proxy_ext=NULL` là trạng thái hợp lệ sau proxy lỗi. Với close, mốc đủ để thoát là WAV đã finalize + Segment/metadata đã commit; Proxy là dữ liệu dẫn xuất. `reconcile` phải dọn `.staging` trước khi recovery ghi staging mới. Nếu recovery cần đánh dấu busy sớm, giữ cùng guard qua khâu encode và transaction cuối.

## Verification

**Commands:**
- `cargo test --locked --manifest-path src-tauri/Cargo.toml` -- recovery/close/force-kill tests và suite Rust qua.
- `npm run bindings && npm run check && npm test` -- binding, typecheck và frontend tests qua.
- `npm run check:i18n && npm run check:ui && git diff --check` -- locale, token và whitespace qua.

**Results:** 579 Rust unit tests + 4 media corpus tests, 635 frontend tests, bindings, Svelte, i18n/UI, Rust format/check và diff check đều qua. Force-kill integration test xác minh mở lại DB, hàng thư viện, Segment và 80.000 mẫu WAV. Native Tauri menu/window event delivery chưa được chạy thủ công trong app; nhánh `ExitRequested`/CmdQ và thứ tự close được kiểm bằng unit test tại seam production.
