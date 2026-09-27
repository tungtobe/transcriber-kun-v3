---
title: '3.1 Đổi tên và xoá phiên'
type: 'feature'
created: '2026-09-27'
status: 'done'
baseline_commit: 'a99dde6394adc285bf3e0a921b86b00db29d6b53'
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

**Problem:** Thư viện chưa cho đổi tên Phiên hay xoá Phiên không còn cần — tên mặc định lấy từ file và dữ liệu cũ (DB + `media/<sid>/`) nằm mãi trong Container.

**Approach:** Thêm menu ⋯ (Đổi tên · Xoá) cho dòng phiên ở Home và header Transcript detail; đổi tên inline; xoá qua dialog xác nhận, IPC điều phối busy-check với `JobRegistry` và giữ khoá độc quyền theo session trong lúc xoá DB (cascade) rồi thư mục media.

## Boundaries & Constraints

**Always:** Tên được trim; rỗng bị từ chối tại chỗ; > 200 ký tự (đếm theo Unicode scalar) bị chặn ở cả UI (`maxlength`) lẫn Rust. Enter lưu, Esc huỷ và trả focus về nút/tên đã mở. Menu ⋯ là `button` có `aria-label`, mở/đóng bằng bàn phím, Esc đóng và trả focus. Dialog xoá theo UX-DR16 (480 px, radius 16, nút nguy hiểm bên phải `button-danger-soft`, chỉ một cấp modal). Xoá bị chặn khi `JobRegistry.is_busy(session)`: trả outcome `Busy`, UI hiện giải thích inline gần dòng/header, không mở lỗi chung. Khi xoá đang chạy, session bị đánh dấu "deleting" trong `ipc/`; Chạy lại, chọn lại Proxy và export cho session đó bị từ chối; kết quả Job tới muộn không tái tạo dữ liệu (swap theo `expected_transcript_id` đã tự bỏ). Thứ tự: xoá dòng DB trong một transaction (cascade transcript/segment) → xoá `media/<sid>/`. Xoá thư mục thất bại → lỗi category `storage`, không báo thành công; gọi lại xoá trên session đã mất dòng DB vẫn dọn thư mục (idempotent) và reconcile lúc boot đã dọn thư mục không có dòng DB. Sau khi xoá: Home tải lại danh sách; từ Transcript detail điều hướng về `/home`. Mọi chuỗi mới có đủ vi/en/ja.

**Never:** Không tạo bảng tag/notes/memo/recording ở story này (chưa tồn tại — story sau tự gắn vào luồng xoá). Không dùng tên phiên làm thành phần đường dẫn. Không cho frontend tự xoá file. Không dùng toast cho lỗi cần hành động. Không đổi `source_name`/`source_hash` khi đổi tên.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Đổi tên hợp lệ | `"  Họp sprint 12  "` | Lưu `Họp sprint 12`, dòng + header cập nhật, `updated_at` đổi | — |
| Tên rỗng/khoảng trắng | `"   "` | Không gọi IPC, lỗi inline, ô vẫn mở | Rust cũng trả `Request` nếu bị gọi |
| Tên 201 ký tự | 201 scalar | Bị chặn | Rust trả `Request` |
| Esc | đang sửa | Khôi phục tên cũ, không gọi IPC | — |
| Xoá phiên rảnh | có proxy + transcript | Dòng DB, transcript, segment, `media/<sid>/` biến mất; outcome `Deleted` | — |
| Xoá khi có Job | `is_busy = true` | Outcome `Busy`, không xoá gì, giải thích inline | — |
| Xoá lỗi FS | thư mục không xoá được | Dòng DB đã xoá, trả lỗi `storage`, UI không báo đã xoá | Gọi lại / boot reconcile dọn tiếp |
| Xoá session không tồn tại | id không có dòng DB, không có thư mục | `Deleted` (idempotent) | — |
| Race Chạy lại/relink | session đang deleting | Lệnh kia trả lỗi `Request` "đang xoá" | — |

</frozen-after-approval>

## Code Map

- `src-tauri/src/db/repo/sessions.rs` -- thêm `set_title(conn, id, title, updated_at) -> usize` và `delete(conn, id) -> usize`; FK `ON DELETE CASCADE` đã có cho transcripts/segments (cần `foreign_keys` bật — kiểm `Db::open`).
- `src-tauri/src/library/store.rs` -- thêm `rename_session` (validate + gọi repo, trả tên đã chuẩn hoá, `Ok(None)` nếu không tồn tại) và `delete_session(db, root, sid)` (transaction DB rồi `remove_path_if_exists(paths::media_dir)`); tái dùng `storage_error`, `now_ms`, `remove_path_if_exists`; có module `fault` cho test lỗi.
- `src-tauri/src/ipc/boot.rs` -- `AppState`: thêm `deleting: Arc<std::sync::Mutex<HashSet<SessionId>>>` (hoặc struct guard nhỏ RAII) khởi tạo trong `boot`.
- `src-tauri/src/ipc/mod.rs` -- lệnh `library_session_rename(session_id, title)` và `library_session_delete(session_id) -> SessionDeleteOutcome { Deleted, Busy }`; hàm quyết định thuần `decide_session_delete` (mẫu `decide_proxy_relink`) để test thứ tự: mark deleting → `jobs.is_busy` → xoá → unmark (mọi nhánh). `transcribe_rerun_inner`, `library_proxy_relink_inner`, `library_transcript_export` kiểm guard trước khi làm việc. Đăng ký trong `specta_builder`, tái sinh `src/lib/bindings.ts` (theo cách dự án đang làm — xem test/export bindings trong `lib.rs`/`ipc`).
- `src/lib/stores/library.svelte.ts` -- thêm `rename(id, title)` (cập nhật `sessions` tại chỗ) và `remove(id)` (trả outcome, reload khi `Deleted`).
- `src/components/SessionMenu.svelte` (mới) -- nút ⋯ + menu (Đổi tên, Xoá), bàn phím + Esc + trả focus.
- `src/components/ConfirmDialog.svelte` (mới) -- dialog xác nhận dùng chung (UX-DR16), lấy style từ `CloseConfirm.svelte`; focus nút huỷ khi mở, Esc = huỷ.
- `src/components/InlineRename.svelte` (mới) -- ô nhập inline, validate trim/rỗng/200, Enter/Esc, lỗi inline.
- `src/routes/home/SessionRow.svelte` -- hiện là một `<a>` bao cả dòng; đổi thành container có link tiêu đề/khu vực chính + `SessionMenu` (không lồng button trong `<a>`), giữ layout 56 px và cột hiện tại; khi đang đổi tên hiển thị `InlineRename`; lỗi Busy hiển thị inline.
- `src/routes/session/SessionHeader.svelte` + `src/routes/Session.svelte` -- click tên (button) hoặc menu → đổi tên; Xoá → dialog → `push('/home')` khi `Deleted`; header nhận `sessionId`/callback.
- `src/i18n/{vi,en,ja}.json` -- nhãn menu, dialog, lỗi rename, busy, storage.
- Test mẫu: `src/routes/home/SessionRow.test.ts`, `src/routes/session/SessionHeader.test.ts`, `src/lib/stores/library.svelte.test.ts`, `store.rs` tests (`count_session_dirs`, `open_db`).

## Tasks & Acceptance

**Execution:**
- [ ] `src-tauri/src/db/repo/sessions.rs` -- `set_title`, `delete` + unit test cascade.
- [ ] `src-tauri/src/library/store.rs` -- `rename_session`, `delete_session` + test: xoá sạch DB+media, idempotent, lỗi FS giữ trả `storage`, không còn thư mục mồ côi.
- [ ] `src-tauri/src/ipc/boot.rs`, `src-tauri/src/ipc/mod.rs` -- guard deleting, hai lệnh mới, `decide_session_delete` + test (busy, deleted, unmark khi lỗi), guard ở rerun/relink/export; tái sinh `bindings.ts`.
- [ ] `src/lib/stores/library.svelte.ts` (+test) -- `rename`, `remove`.
- [ ] `src/components/{SessionMenu,ConfirmDialog,InlineRename}.svelte` (+test) -- component dùng chung.
- [ ] `src/routes/home/SessionRow.svelte`, `src/routes/session/SessionHeader.svelte`, `src/routes/Session.svelte` (+test) -- nối menu/rename/delete.
- [ ] `src/i18n/{vi,en,ja}.json` -- chuỗi mới, `check:i18n` pass.

**Acceptance Criteria:**
- Given dòng phiên ở Home, when chỉ dùng bàn phím (Tab tới ⋯, Enter, mũi tên/Tab, Esc), then mở menu, chọn Đổi tên/Xoá và đóng menu với focus trả về nút ⋯.
- Given Phiên đã xoá, when mở lại app (reconcile + list), then Phiên không xuất hiện và không có lỗi thiếu file.
- Given đang xoá một Phiên, when một Chạy lại/relink cho cùng Phiên được gọi, then bị từ chối và không ghi lại dữ liệu nào.

## Design Notes

Khoá "deleting" là một `HashSet<SessionId>` trong `AppState` (chỉ `ipc/` đọc/ghi — AD-1 điều phối ở IPC). Đánh dấu **trước** khi hỏi `is_busy`, nên một Chạy lại bắt đầu sau lần hỏi đó sẽ thấy guard và bị từ chối; Job transcribe file mới luôn có `SessionId` mới nên không đụng. DB trước, file sau: nếu crash giữa chừng, thư mục không còn dòng DB và `reconcile` hiện có sẽ xoá nó lúc boot — không cần bảng tombstone. Live/Recording/Memo/Notes/Tag chưa tồn tại; các story sau tự thêm bảng có `ON DELETE CASCADE` và busy-check `LiveSession` vào `decide_session_delete`.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml --locked` -- pass
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` && `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` -- sạch
- `npm test && npm run check && npm run check:i18n && npm run check:ui && npm run check:deps` -- pass
