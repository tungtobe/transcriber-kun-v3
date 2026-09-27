---
title: '3.4 Settings — Lưu trữ trong Container'
type: 'feature'
created: '2026-09-27'
status: 'done'
baseline_commit: '02ee378bda713311f0f98d28d61e48d77f59ed9a'
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

**Problem:** Người dùng không biết app đang chiếm bao nhiêu dung lượng trên máy và không có cách xoá sạch dữ liệu họp một lần.

**Approach:** Thêm nhóm Settings "Lưu trữ" hiển thị dung lượng Media, DB, số Phiên, nút "Mở thư mục", và "Xoá toàn bộ dữ liệu…" với xác nhận hai bước trong cùng một dialog; phía Rust có lệnh đo dung lượng, mở thư mục, và xoá toàn bộ có khoá toàn cục chặn writer mới.

## Boundaries & Constraints

**Decision OQ9 (autorun — người dùng uỷ quyền chọn phương án đề xuất, 2026-09-27):** "Xoá toàn bộ dữ liệu" xoá **dữ liệu họp**: mọi Phiên, Transcript, segment, Proxy, Recording, Ghi chú, Memo, Tag (cả tag không còn Phiên), thư mục `media/` (kể cả `.staging`). **Giữ**: settings (bảng `settings`), Consent, API key (Keychain), Template memo. Dialog liệt kê chính xác hai danh sách này.

**Always:** Số liệu: Media = tổng byte đệ quy dưới `<data_dir>/media/`; DB = tổng `app.db` + `app.db-wal` + `app.db-shm` (file thiếu = 0); số Phiên = số dòng `sessions`. Hiển thị bằng đơn vị dễ đọc (B/KB/MB/GB) và một thanh tỉ lệ Media/DB. Xoá toàn bộ bị chặn (kèm lời giải thích inline) nếu có Job chưa kết thúc (hoặc Live — chưa tồn tại, để hook). Khi xoá đang chạy, một cờ toàn cục trong `AppState` (chỉ `ipc/` đọc/ghi) làm mọi writer mới bị từ chối tại Rust: transcribe start/rerun, relink proxy, export, rename/delete phiên, gắn/tạo/xoá tag. Đặt cờ **trước** khi kiểm Job bận; gỡ cờ trên mọi nhánh thoát. Thứ tự: DB trong một transaction (`DELETE FROM sessions`, `DELETE FROM tags`, và các bảng dữ liệu họp khác nếu có) rồi mới xoá nội dung `media/`; sau đó `wal_checkpoint(TRUNCATE)` và đo lại thực tế — không giả định DB = 0. Lỗi FS/DB trả category `storage`; gọi lại là idempotent (reconcile lúc boot cũng dọn thư mục mồ côi). Sau khi xoá thành công, frontend reload danh sách Phiên và tag, và cập nhật số liệu. Hai bước xác nhận nằm trong cùng một dialog (không chồng modal); nút xác nhận cuối `button-danger-soft`; Esc/huỷ đóng và trả focus về nút mở. "Mở thư mục" mở `<data_dir>` bằng trình quản lý file của OS; nếu OS/plugin báo lỗi thì hiện lỗi inline. Chuỗi mới đủ vi/en/ja.

**Never:** Không có tuỳ chọn thư mục cache/đổi vị trí lưu. Không tác vụ nền nào tự xoá dữ liệu người dùng. Không xoá settings/Consent/key/template. Không dùng tên phiên/tag làm thành phần đường dẫn. Không mở quyền `opener` rộng cho frontend (mở thư mục qua lệnh Rust).

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Xem số liệu | 3 Phiên, media 120 MB | Media ≈120 MB, DB > 0, "3 phiên" | Lỗi đo → thông báo lỗi `storage` inline |
| Xoá thành công | 3 Phiên, 2 tag, không Job | Outcome `Wiped`; 0 phiên, media 0 B, không còn thư mục trong `media/`, bảng `tags` rỗng, `settings` còn nguyên | — |
| Đang có Job | Job `running` | Outcome `Busy`, không xoá gì; UI giải thích | — |
| Writer khi đang xoá | cờ wiping bật, gọi `transcribe_start`/tag attach | Bị từ chối, không ghi dữ liệu | lỗi `Request` "đang xoá toàn bộ dữ liệu" |
| Lỗi FS giữa chừng | xoá media thất bại | Báo lỗi `storage`; gọi lại xoá tiếp được | cờ wiping đã gỡ |
| Kho rỗng | 0 Phiên | Xoá vẫn `Wiped` (idempotent) | — |
| Huỷ ở bước 2 | bấm Huỷ/Esc | Không gọi IPC, dialog đóng, focus về nút | — |

</frozen-after-approval>

## Code Map

- `src-tauri/src/ipc/boot.rs:38-45` -- `AppState` có `deleting: Arc<Mutex<HashSet<SessionId>>>`; thêm cờ toàn cục `wiping` (vd `Arc<AtomicBool>`) khởi tạo trong `boot`.
- `src-tauri/src/ipc/mod.rs` -- mẫu: `is_session_deleting` (46-51), `session_deleting_error` (59-61), `decide_session_delete` (1406-1426), `library_session_delete_inner` (1428-1453). Thêm `is_wiping`/`wiping_error`, gắn kiểm tra vào mọi chỗ đang kiểm `is_session_deleting` + `transcribe_start` + rename/delete/tag create/delete. Lệnh mới: `library_storage_stats() -> StorageStats { media_bytes, db_bytes, session_count }`, `library_open_data_dir()`, `library_wipe_all() -> WipeAllOutcome { Wiped, Busy }` với hàm quyết định thuần `decide_wipe_all` (mark → any-busy → wipe → unmark). Đăng ký trong `specta_builder` (1594-1633), tái sinh `src/lib/bindings.ts` bằng `npm run bindings`.
- `src-tauri/src/transcribe/registry.rs:335-341` -- `snapshot()` trả mọi job; thêm `any_active()` (trạng thái chưa terminal) hoặc tính từ snapshot ở ipc.
- `src-tauri/src/library/store.rs` -- `delete_session` (806-820), `remove_path_if_exists`, `storage_error`, `reconcile` (881+), module `fault` cho test lỗi. Thêm `storage_stats(db, root)` và `wipe_all(db, root)`.
- `src-tauri/src/core/paths.rs:22-38` -- `media_dir`, staging; thêm helper thư mục gốc media nếu chưa có. `src-tauri/src/db/mod.rs:21,32-52` -- `DB_FILE_NAME`, WAL.
- `src-tauri/src/db/repo/sessions.rs` -- thêm `count`; tag repo ở `library/tags.rs` (bảng `tags`, `session_tags` cascade).
- `src-tauri/src/lib.rs:30` -- `tauri_plugin_opener` đã đăng ký; dùng `OpenerExt::opener().open_path` từ Rust (không đổi `capabilities/default.json`).
- `src/routes/Settings.svelte:16-21,51-66` -- thêm nhóm `storage` (`settings.group.storage`) trước `diagnostics`; `Settings.groups.test.ts` cập nhật.
- `src/routes/settings/SettingsStorage.svelte` (mới, +test) -- mẫu `SettingsDiagnostics.svelte`; thanh dung lượng dùng token màu có sẵn (`check:ui`).
- `src/components/ConfirmDialog.svelte` -- tái dùng style/hành vi (portal, focus huỷ, Esc); dialog hai bước có thể là component mới `WipeAllDialog.svelte` hoặc thêm bước vào ConfirmDialog mà không phá API hiện tại.
- `src/lib/stores/library.svelte.ts:197-210,411` -- `remove`/`load`; thêm reload tag (xem cách 3.2 nạp danh sách tag) sau wipe.
- `src/i18n/{vi,en,ja}.json` -- nhãn nhóm, số liệu, dialog (xoá/giữ), busy, lỗi.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/library/store.rs` (+test) -- `storage_stats`, `wipe_all`: xoá sạch DB họp + media, giữ `settings`, idempotent, lỗi FS trả `storage` (dùng `fault`).
- [x] `src-tauri/src/ipc/boot.rs`, `src-tauri/src/ipc/mod.rs` (+test) -- cờ wiping, gate writer, 3 lệnh mới, `decide_wipe_all` test (busy, wiped, unmark khi lỗi, writer bị từ chối khi cờ bật); tái sinh bindings. (`transcribe/registry.rs` không đổi -- busy-check tính thẳng từ `snapshot()` ở `ipc/`, đúng nhánh "hoặc" mà Code Map cho phép.)
- [x] `src/components/WipeAllDialog.svelte` (+test) -- hai bước trong một dialog, liệt kê xoá/giữ, `button-danger-soft`.
- [x] `src/routes/settings/SettingsStorage.svelte` (+test), `src/routes/Settings.svelte`, `src/routes/Settings.groups.test.ts` -- nhóm Lưu trữ, busy/lỗi inline, reload store sau wipe.
- [x] `src/lib/stores/library.svelte.ts` (+test) -- hàm reload phiên + tag sau wipe.
- [x] `src/i18n/{vi,en,ja}.json` -- chuỗi mới.

**Acceptance Criteria:**
- Given nhóm Lưu trữ, when mở, then thấy Media, DB, số phiên, nút "Mở thư mục", và không có tuỳ chọn thư mục cache.
- Given dialog xoá toàn bộ, when chỉ dùng bàn phím, then đi qua hai bước, huỷ bằng Esc trả focus về nút mở.
- Given xoá thành công rồi mở lại app, when reconcile + list, then 0 Phiên, không lỗi thiếu file, settings/Consent còn nguyên.

## Implementation Notes

- `decide_wipe_all` (`ipc/mod.rs`) mirrors `decide_session_delete`'s gate order exactly (mark → busy-check → wipe → unmark on every exit), reusing the story 3.1 shape one-to-one instead of introducing a new pattern.
- The "any active Job" check reuses `JobRegistryHandle::snapshot()` (`!snapshot.is_empty()`), the same signal `app_close_confirm` already uses to mean "something is running" — no new registry method was needed (Code Map's "hoặc tính từ snapshot ở ipc" branch).
- `StorageStats.mediaBytes`/`dbBytes` are `f64`, not `i64` — same convention as `SessionListItem.createdAt` (specta-typescript forbids exporting BigInt-style integers). The frontend reads them with `?? 0` since specta renders `f64` as `number | null`.
- `wipe_all` deletes `sessions` then `tags` in one transaction (both FKs `ON DELETE CASCADE`, so `session_tags` disappears either way), removes `media/` recursively (including `.staging`), then runs `PRAGMA wal_checkpoint(TRUNCATE)` so the next `storage_stats` call reflects the DB's real, shrunk size instead of stale WAL bytes.
- `library_open_data_dir` calls `tauri_plugin_opener::OpenerExt::open_path` directly from the Rust command body — no new capability was added to `capabilities/default.json` (spec Never: "không mở quyền `opener` rộng cho frontend").
- `WipeAllDialog.svelte` is a new component (not a `ConfirmDialog` extension) with its own two-step internal state; `Esc`/"Huỷ" close the whole dialog at either step (never "go back a step"), matching the spec's literal wording.

## Spec Change Log

## Review Triage Log

## Design Notes

Cờ `wiping` toàn cục song song với `deleting` per-session của 3.1: đặt trước khi hỏi Job bận nên Job bắt đầu sau đó thấy cờ và bị từ chối — không có callback Job nào còn sống để tái tạo dữ liệu. DB trước, file sau: crash giữa chừng để lại thư mục không có dòng DB, `reconcile` sẵn có dọn lúc boot. Notes/Memo/Live chưa tồn tại: story sau thêm bảng vào `wipe_all` và busy-check Live vào `decide_wipe_all`.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml --locked` -- pass (403 unit/integration tests, incl. 10 new: `storage_stats_*`, `wipe_all_*`, `decide_wipe_all_*`, `is_wiping_*`)
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check && cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` -- sạch
- `npm run bindings` -- regenerated `src/lib/bindings.ts` (adds `StorageStats`, `WipeAllOutcome`, `libraryStorageStats`/`libraryOpenDataDir`/`libraryWipeAll`)
- `npm test && npm run check && npm run check:i18n && npm run check:ui && npm run check:deps` -- pass (523 frontend tests, incl. 20 new across `SettingsStorage.test.ts`, `WipeAllDialog.test.ts`, `library.svelte.test.ts`, `Settings.groups.test.ts`)
