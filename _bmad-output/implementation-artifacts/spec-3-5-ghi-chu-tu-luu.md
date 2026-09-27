---
title: '3.5 Ghi chú tự lưu'
type: 'feature'
created: '2026-09-27'
status: 'done'
baseline_commit: 'e29b5e21731880f20173c7b47400770f86f860ea'
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

**Problem:** Người dùng chưa có chỗ gõ ghi chú cho một Phiên; ghi chú cần lưu bền tự động, biết rõ khi nào đã lưu, và sống sót qua đóng app/force-quit để sau này nhúng vào memo.

**Approach:** Bảng `notes` + lệnh `notes_get`/`notes_save` có revision tăng dần theo Phiên; component `NotesPanel` dùng chung (Transcript detail nay, Live sau) tự lưu sau debounce 800 ms, chỉ báo "Đã lưu hh:mm" sau ACK commit, flush khi rời màn/đóng panel/đóng app.

## Boundaries & Constraints

**Decision OQ10 (autorun — người dùng uỷ quyền chọn phương án đề xuất, 2026-09-27):** Cơ chế bền = commit SQLite (WAL, `synchronous=FULL` đặt tường minh trong `Db::open`); ACK = lệnh `notes_save` trả về sau khi transaction commit. Không có journal riêng. Cửa sổ mất dữ liệu được chấp nhận khi force-quit/crash: chỉ các ký tự gõ sau ACK gần nhất (≤ debounce 800 ms + độ trễ lưu). Không tuyên bố "không mất ký tự nào".

**Always:** Migration mới tạo `notes(session_id TEXT PK REFERENCES sessions(id) ON DELETE CASCADE, body TEXT NOT NULL, revision INTEGER NOT NULL, updated_at INTEGER NOT NULL)` — xoá Phiên/xoá toàn bộ dọn ghi chú qua cascade; Chạy lại/Transcribe lại giữ nguyên (cùng `session_id`). `notes_save(session_id, body, revision)` chỉ ghi nếu `revision` > revision đang lưu (upsert có điều kiện, nguyên tử); trả `Saved { revision, updatedAt }`, `Stale { revision }` (không ghi) hoặc `NotFound`. `notes_get` trả `{ body, revision, updatedAt } | null`. Body giới hạn 100 000 ký tự (vượt → lỗi `Request`). `notes_save` bị từ chối khi đang xoá toàn bộ (`is_wiping`) hoặc Phiên đang bị xoá (`is_session_deleting`). Trong Rust, body đi qua repo/store dưới dạng `Sensitive<String>`; không log/diagnostics nào chứa nội dung (test như `core/log.rs`). Frontend: mỗi lần gõ đặt lại debounce 800 ms; mỗi lần lưu gửi revision = revision cao nhất đã gửi + 1; trạng thái hiển thị: "Đang lưu…", "Đã lưu hh:mm" (giờ địa phương từ `updatedAt` của ACK mới nhất, chỉ khi không còn thay đổi chưa ACK), "Chưa lưu" + nút "Thử lại" khi lỗi (giữ buffer, không mất chữ). ACK về sai thứ tự không được làm lùi trạng thái hay nội dung. Flush (huỷ debounce, lưu ngay, chờ ACK) khi: unmount panel/rời route, đổi tab panel, đóng app. Đóng app: sự kiện đóng cửa sổ luôn đi qua frontend (kể cả khi không có Job) để flush ghi chú; flush thành công → tiếp tục luồng đóng hiện có; flush thất bại → dialog cho chọn "Ở lại" (mặc định, focus) hoặc "Vẫn thoát". Panel là tab "Ghi chú" trong aside 360 px của Transcript detail (tab còn lại giữ thông tin hiện có); `NotesPanel` chỉ nhận `sessionId` (+ tuỳ chọn) để Live tái dùng nguyên vẹn và có hàm `flush()` công khai cho "lưu bền trước khi Phiên finalize". Chuỗi mới đủ vi/en/ja.

**Never:** Không lưu ghi chú vào localStorage/file riêng. Không hiện "Đã lưu" khi debounce kích hoạt mà chưa có ACK. Không để bản cũ ghi đè bản mới. Không đưa nội dung ghi chú vào log, lỗi, tiêu đề, đường dẫn file. Không làm Markdown/rich text (textarea thuần).

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Gõ rồi dừng | gõ "abc", chờ 800 ms | 1 lần `notes_save` rev n+1; sau ACK hiện "Đã lưu hh:mm" | — |
| Gõ liên tục | gõ mỗi 300 ms trong 3 s | Không lưu trong lúc gõ; lưu 1 lần sau khi dừng | — |
| Stale | DB rev 5, gửi rev 4 | `Stale{5}`, DB không đổi | — |
| ACK sai thứ tự | gửi rev 6 rồi 7; ACK 7 về trước 6 | Trạng thái theo rev 7, không lùi | — |
| Lưu lỗi | `notes_save` lỗi `storage` | "Chưa lưu" + Thử lại; buffer còn nguyên; Thử lại lưu được | — |
| Rời màn trước debounce | gõ rồi điều hướng ngay | Flush gửi `notes_save` với nội dung mới nhất | — |
| Đóng app, flush lỗi | lưu lỗi khi đóng | Dialog "Ở lại"/"Vẫn thoát"; Ở lại giữ app mở | — |
| Bền qua đóng đột ngột | ACK rev 3 rồi mở lại DB không đóng sạch | Đọc được body rev 3 | — |
| Chạy lại / xoá phiên | Phiên có ghi chú | Chạy lại: giữ; Xoá/Xoá toàn bộ: dòng `notes` biến mất | — |
| Đang xoá | `wiping`/`deleting` bật | `notes_save` bị từ chối, không ghi | lỗi `Request` |

</frozen-after-approval>

## Code Map

- `src-tauri/src/db/migrations/mod.rs:35` -- `MIGRATIONS: [M; 4]`; thêm migration 5 (bảng `notes`), cập nhật assert version ở test (dòng ~134) và `db/mod.rs:120`. Chỉ file này được chứa `CREATE TABLE`.
- `src-tauri/src/db/mod.rs:36-65` -- `Db::open` (WAL); thêm `PRAGMA synchronous = FULL` + test đọc lại pragma.
- `src-tauri/src/db/repo/notes.rs` (mới, khai báo trong `repo/mod.rs`) -- `get`, `save_if_newer` (upsert `ON CONFLICT(session_id) DO UPDATE ... WHERE excluded.revision > notes.revision`).
- `src-tauri/src/library/notes.rs` (mới, cạnh `library/tags.rs`) -- validate độ dài, `Sensitive<String>`, `NotesSaveOutcome`, test bền (mở `Db` thứ hai trên cùng file sau khi ACK, không đóng kết nối đầu).
- `src-tauri/src/core/sensitive.rs:10` -- `Sensitive<T>` (`new`, `expose`, `into_inner`; Debug/Display = "[redacted]"); không có serde — chỉ dùng trong Rust, payload IPC là `String`.
- `src-tauri/src/ipc/mod.rs` -- mẫu `library_session_rename` (~1402): `is_wiping`/`wiping_error`, `is_session_deleting`/`session_deleting_error` (45-68), `blocking`, `track_ipc_error`; thêm `notes_get`, `notes_save`; đăng ký trong `specta_builder` (~1750); `npm run bindings` tái sinh `src/lib/bindings.ts`.
- `src-tauri/src/lib.rs:39-66` -- `CloseRequested`: hiện nhánh idle `exit` thẳng, nhánh bận emit `ipc::CloseRequested`; đổi để luôn emit (thêm cờ `busy` vào payload event) và frontend quyết định. `app_close_confirm` (`ipc/mod.rs:1718-1744`) giữ nguyên hành vi hủy Job rồi thoát.
- `src/lib/stores/app.svelte.ts:34-43` + `src/components/CloseConfirm.svelte` -- nghe `closeRequested`: flush ghi chú trước; idle + flush ok → `appCloseConfirm`; bận → dialog Job hiện có (flush trước khi xác nhận); flush lỗi → dialog Ở lại/Vẫn thoát (dùng `ConfirmDialog`).
- `src/lib/stores/notes.svelte.ts` (mới) -- store theo mẫu `createXStore()` (`library.svelte.ts`): per-session buffer, revision, debounce (tự viết, không có util sẵn), trạng thái, `flush(sessionId)`, `flushAll()` cho luồng đóng app.
- `src/components/NotesPanel.svelte` (mới) -- textarea + dòng trạng thái; `flush()` qua `bind:this`/export; flush trong `onDestroy`.
- `src/routes/Session.svelte:510-521,616` -- `<aside class="session-aside">` (hiện là `<dl>` thông tin) trong grid `var(--panel-detail-width)`; thêm tab strip (Thông tin | Ghi chú), đổi tab thì flush.
- Giờ "hh:mm": `src/lib/time.ts` chỉ format thời lượng — thêm helper giờ trong ngày dùng `Intl.DateTimeFormat` theo locale i18n.
- Test mẫu: `src/lib/stores/library.svelte.test.ts` (mock `commands` qua `vi.hoisted`), `src/routes/Session.test.ts`, `src/components/CloseConfirm*.test.ts` nếu có; Rust `core/log.rs:205` (test Sensitive không lộ).

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/db/migrations/mod.rs`, `src-tauri/src/db/mod.rs`, `src-tauri/src/db/repo/{notes,mod}.rs` (+test) -- bảng, pragma, repo có điều kiện revision, cascade.
- [x] `src-tauri/src/library/notes.rs` (+test) -- save/get, giới hạn, Stale/NotFound, bền qua mở lại, không lộ log.
- [x] `src-tauri/src/ipc/mod.rs`, `src-tauri/src/lib.rs` (+test) -- 2 lệnh, guard wiping/deleting, close luôn qua frontend; tái sinh bindings.
- [x] `src/lib/stores/notes.svelte.ts` (+test) -- debounce, revision, ACK sai thứ tự, lỗi/Thử lại, flush.
- [x] `src/components/NotesPanel.svelte` (+test), `src/routes/Session.svelte` (+test) -- tab Ghi chú, flush khi đổi tab/unmount.
- [x] `src/lib/stores/app.svelte.ts`, `src/components/CloseConfirm.svelte` (+test) -- flush trước khi đóng, dialog khi flush lỗi.
- [x] `src/i18n/{vi,en,ja}.json` -- chuỗi mới.

**Acceptance Criteria:**
- Given Transcript detail, when mở tab Ghi chú và gõ, then sau ~800 ms dừng gõ thấy "Đang lưu…" rồi "Đã lưu hh:mm".
- Given Phiên có ghi chú, when mở lại Transcript detail (hoặc khởi động lại app), then ghi chú hiển thị đúng bản đã ACK.
- Given không có Job, when đóng cửa sổ và ghi chú lưu được, then app thoát như trước (không có dialog thừa).

## Implementation Notes

- `db::repo::notes::save_if_newer` is a single `INSERT ... ON CONFLICT(session_id) DO UPDATE ... WHERE excluded.revision > notes.revision` statement: 0 rows changed means "existing row, stale revision" (read it back for `Stale{revision}`), a `ConstraintViolation` on the `FK notes.session_id → sessions(id)` means the session doesn't exist (`NotFound`) — no separate existence check, so no race window between "does it exist" and "write it".
- `synchronous = FULL` is set with `pragma_update` (not `query_row` like `journal_mode`) — unlike `journal_mode`, `PRAGMA synchronous = X` does not return a result row.
- Public IPC types keep the established BigInt-avoidance convention: `NoteSnapshot.revision`/`NotesSaveOutcome::{Saved,Stale}.revision` are `i32` (small bounded counter, same as `TagWithCount::session_count`), `updated_at` is `f64` (same as `SessionDetail::created_at`).
- `CloseRequested` now always carries `{ busy: bool }` and Rust never calls `exit(0)` itself from `on_window_event` any more — `app_close_confirm` (unchanged: cancel Jobs, poll, exit) is the only exit path, called from the frontend after flush decides it's safe.
- `notes.svelte.ts`'s `Control` per session allows genuinely concurrent `notesSave` calls (not serialized/queued): `ensureSent` dispatches immediately whenever the current buffer differs from the body of the most recently dispatched request, even while an older request for the Phiên is still in flight. This is what makes the spec's literal "gửi rev 6 rồi 7; ACK 7 về trước 6" scenario reproducible (and safe: `applyOutcome` only ever raises `ackedRevision`, never lowers it) instead of accidentally serializing every `flush()` behind whatever request happened to be in flight.
- `setEntry` (the store's `entries` Map copy-on-write helper) wraps its read of the previous `entries` value in Svelte's `untrack`. Without it, `NotesPanel`'s mount `$effect` calling `notesStore.load()` synchronously (on a `commands.notesGet` rejection/throw reached before any real `await` suspension) reads-then-writes the same `$state` from within that effect's own execution and Svelte throws `effect_update_depth_exceeded`. `untrack` only suppresses dependency tracking for that internal read; `view()`'s own read (used by the `$derived` in `NotesPanel`) is intentionally left tracked.
- `NotesPanel` stays mounted (never destroyed) when the aside tab switches away from "Ghi chú" — `Session.svelte` toggles visibility with a CSS class and calls the panel's exported `flush()` explicitly on tab-away, since the panel's own `$effect` cleanup only fires on unmount or a `sessionId` change, not a hidden/visible toggle.
- Test-suite side effect: `Session.test.ts`'s `commands` mock needed `notesGet`/`notesSave` added (defaulting `notesGet` to `{ status: 'ok', data: null }`) because `NotesPanel` now mounts for every `saved` view and otherwise its load-error state (`role="alert"`) collided with that suite's unrelated `queryByRole('alert')` assertions.

## Spec Change Log

## Review Triage Log

## Design Notes

Revision do frontend cấp (tăng dần từ revision đọc được lúc mở) và Rust chỉ nhận revision lớn hơn — nên ACK/lệnh về sai thứ tự không thể ghi đè bản mới, kể cả khi hai lời gọi chạy song song. Nếu nhận `Stale` (vd hai cửa sổ), store nạp lại bản trong DB chỉ khi buffer không có thay đổi chưa lưu; ngược lại gửi lại với revision = stale + 1. Luồng đóng app luôn qua frontend để không có đường thoát nào bỏ qua flush; Job-busy dialog giữ nguyên.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml --locked` -- pass (422 tests, incl. 17 new across `db::migrations`, `db::repo::notes`, `library::notes`)
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check && cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` -- sạch
- `npm run bindings` -- regenerated `src/lib/bindings.ts` (adds `notesGet`/`notesSave`, `NoteSnapshot`, `NotesSaveOutcome`; `CloseRequested` now `{ busy: boolean }`)
- `npm test && npm run check && npm run check:i18n && npm run check:ui && npm run check:deps` -- pass (549 tests, incl. new `notes.svelte.test.ts`, `NotesPanel.test.ts`, and additions to `Session.test.ts`/`CloseConfirm.test.ts`/`app.svelte.test.ts`)

- Orchestrator fix sau khi implement: `notesStore.load()` không nạp đè buffer chưa ACK (lưu lỗi rồi quay lại màn, hoặc gõ trong lúc `notesGet` chưa về); thêm 2 test.
