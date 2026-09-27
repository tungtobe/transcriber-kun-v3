---
title: '3.2 Tag — gắn, lọc và quản lý'
type: 'feature'
created: '2026-09-27'
status: 'done'
baseline_commit: 'e611655c3db2d8a2f50197c9bc0fc903d5057e41'
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

**Problem:** Thư viện nhiều phiên chưa có cách nhóm theo khách hàng/sprint — không gắn được tag, không lọc được danh sách.

**Approach:** Thêm bảng `tags`/`session_tags` với khoá chuẩn hoá UNIQUE; module `library/tags.rs` + IPC gắn/gỡ/tạo/xoá toàn cục; một component Tag picker dùng chung (mode "filter" ở Home, mode "assign" ở Transcript detail và menu ⋯ của dòng phiên); hàng chip lọc một dòng ở Home, lọc AND phía client.

## Boundaries & Constraints

**Always:** Migration mới (thứ 4, chỉ tiến) tạo `tags(id TEXT PK, name TEXT NOT NULL, name_key TEXT NOT NULL UNIQUE, created_at)` và `session_tags(session_id REFERENCES sessions ON DELETE CASCADE, tag_id REFERENCES tags ON DELETE CASCADE, PRIMARY KEY(session_id, tag_id))`. `name` = trim + gộp khoảng trắng; `name_key` = `name` lowercase theo Unicode (Rust `to_lowercase`), không dựa vào `COLLATE NOCASE`; frontend `normalize('NFC')` trước khi gửi. Tạo tag là idempotent theo `name_key` (`INSERT … ON CONFLICT DO NOTHING` rồi đọc lại) — tạo đồng thời không lỗi, không trùng. Rỗng/chỉ khoảng trắng bị từ chối; > 80 ký tự (Unicode scalar) bị từ chối; Phiên đã có 20 tag thì gắn thêm bị từ chối — UI chặn tại chỗ có giải thích, Rust trả `Request`. Tag không phụ thuộc tên/transcript: đổi tên, Chạy lại, Transcribe lại không đụng `session_tags`; xoá Phiên chỉ xoá liên kết, tag còn lại. Danh sách Home (`library_sessions_list`) trả kèm tag của mỗi Phiên trong cùng truy vấn/không N+1; `tags_list` trả tag kèm số phiên, sắp số phiên giảm dần rồi tên. Tag picker: popover 320 px, radius 12, ô tìm/tạo mới (Enter tạo tag chưa có), danh sách theo số phiên; mode filter có nhóm "Đang lọc", mode assign có nhóm "Đã chọn"; gắn tag không đổi bộ lọc Home; link "Quản lý tag" mở chế độ quản lý (danh sách tag + nút xoá → `ConfirmDialog` → gỡ khỏi mọi Phiên trong một transaction); chưa có tag nào → "Gõ để tạo tag đầu tiên". Esc/click ngoài đóng popover, focus về nút mở; toàn bộ thao tác dùng bàn phím. Hàng chip Home một dòng, không wrap (overflow ẩn): tag đang lọc (có ×) → tối đa 5 tag dùng nhiều nhất chưa lọc → chip "+ N tag khác" mở picker mode filter → vạch dọc → chip "Chưa gắn tag". Lọc: AND các tag; "Chưa gắn tag" loại trừ tag cụ thể (bật nó xoá tag lọc; chọn tag cụ thể tắt nó). Trạng thái lọc sống trong `libraryStore` (không persist) và được đặt tên sao cho story 3.3 thêm query tên vào cùng hàm lọc. Transcript detail: hàng chip tag của Phiên + chip "+ Tag"; dòng phiên Home: menu ⋯ thêm "Gắn tag", hiển thị tối đa vài chip tag trong dòng mà vẫn giữ chiều cao 56 px. Chuỗi mới đủ vi/en/ja.

**Never:** Không dùng tên tag làm đường dẫn; không persist bộ lọc; không thêm dependency npm/Cargo; không làm LiveSetup (Epic 4) — chỉ đảm bảo component nhận mode/props đủ để tái dùng; không làm ô tìm theo tên (story 3.3).

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Trùng hoa thường | có `DỰ ÁN`, tạo `dự án` | Trả tag `DỰ ÁN` sẵn có, không tạo mới | — |
| Khoảng trắng | `"   "` / `"  sprint   12 "` | Từ chối / lưu `sprint 12` | `Request` |
| Tạo đồng thời | 2 luồng tạo `Khách A` | Đúng 1 dòng `tags`, cả hai nhận cùng id | — |
| Quá dài | 81 ký tự | Chặn tại chỗ | `Request` |
| Tag thứ 21 | Phiên có 20 tag | Chặn tại chỗ, giải thích | `Request`; gắn tag đã có trên Phiên là no-op |
| Lọc AND | chọn `A`,`B` | Chỉ Phiên có cả A và B | — |
| Chưa gắn tag | đang lọc `A`, bật "Chưa gắn tag" | Bỏ `A`, chỉ Phiên không tag; chọn `A` lại tắt "Chưa gắn tag" | — |
| Xoá tag toàn cục | tag gắn 3 Phiên | Dialog; đồng ý → tag và 3 liên kết biến mất, gỡ khỏi bộ lọc | Lỗi → báo inline, giữ nguyên |
| Xoá Phiên | Phiên duy nhất có tag `A` | `A` vẫn còn, số phiên 0 | — |
| Chạy lại/đổi tên | Phiên có tag | Tag giữ nguyên | — |

</frozen-after-approval>

## Code Map

- `src-tauri/src/db/migrations/mod.rs` -- thêm `M::up` thứ 4; cập nhật test `user_version` (hiện kỳ vọng 3); nơi duy nhất được `CREATE TABLE`.
- `src-tauri/src/db/repo/tags.rs` (mới, đăng ký trong `repo/mod.rs`) -- SQL cho `tags`/`session_tags`: upsert theo `name_key`, list kèm count, list theo session, attach (kiểm ≤ 20 trong transaction), detach, delete; mẫu `repo/sessions.rs` (parse id, `params!`).
- `src-tauri/src/db/repo/sessions.rs::list_for_home` -- thêm tag của mỗi Phiên (ví dụ `GROUP_CONCAT` của `tag_id` hoặc query thứ hai gom theo map — không N+1); `SessionListRow` thêm `tag_ids`.
- `src-tauri/src/core/id.rs` -- thêm `TagId` theo mẫu `SessionId` (UUIDv7, specta `Type`).
- `src-tauri/src/library/tags.rs` (mới, `library/mod.rs`) -- chuẩn hoá tên (`normalize_tag_name` → `(name, key)`), validate 80/rỗng, giới hạn 20, gọi repo; test đơn vị cho ma trận (kể cả hai thread tạo đồng thời trên cùng `Db`).
- `src-tauri/src/library/store.rs::SessionDetail`/`get_detail` -- thêm `tags: Vec<TagSummary>` của Phiên.
- `src-tauri/src/ipc/mod.rs` -- lệnh `tags_list`, `tags_create(name)`, `session_tags_attach(session_id, tag_id)`, `session_tags_detach`, `tags_delete(tag_id)`; `SessionListItem` thêm `tag_ids: Vec<TagId>`; guard `is_session_deleting` cho attach; đăng ký `specta_builder`, tái sinh `src/lib/bindings.ts`.
- `src/lib/stores/library.svelte.ts` -- thêm `tags` (list + count), `tagFilter: { tagIds: string[]; untagged: boolean }`, `filteredSessions` (derived, hàm lọc thuần tách ra `src/lib/session-filter.ts` + test để 3.3 mở rộng), hành động tag gọi IPC rồi tải lại tags/sessions.
- `src/components/TagPicker.svelte` (mới) -- popover dùng chung, props `mode: 'filter' | 'assign'`, `selectedIds`, callbacks; chế độ quản lý bên trong; dùng `ConfirmDialog` (đã portal lên body).
- `src/routes/home/TagFilterBar.svelte` (mới) -- hàng chip lọc một dòng; `Home.svelte` render nó trên danh sách và dùng `filteredSessions`.
- `src/routes/home/SessionRow.svelte` -- menu ⋯ thêm "Gắn tag" (mở TagPicker assign, neo vào nút ⋯); chip tag gọn trong dòng; giữ 56 px (virtual list `SessionList.svelte` giả định cố định).
- `src/components/SessionMenu.svelte` -- thêm mục tuỳ chọn `onTag`.
- `src/routes/Session.svelte` / `src/routes/session/SessionHeader.svelte` -- hàng chip tag + "+ Tag" (TagPicker assign), cập nhật chi tiết sau khi gắn/gỡ.
- `src/i18n/{vi,en,ja}.json` -- nhãn picker, lọc, quản lý, giới hạn, lỗi.

## Tasks & Acceptance

**Execution:**
- [ ] `src-tauri/src/db/migrations/mod.rs`, `src-tauri/src/db/repo/tags.rs`, `src-tauri/src/core/id.rs` -- schema + repo + test cascade (xoá Phiên giữ tag, xoá tag gỡ liên kết).
- [ ] `src-tauri/src/library/tags.rs`, `src-tauri/src/library/store.rs`, `src-tauri/src/db/repo/sessions.rs` -- chuẩn hoá, giới hạn, detail/list kèm tag, test ma trận + đồng thời + rerun giữ tag.
- [ ] `src-tauri/src/ipc/mod.rs` -- lệnh mới + bindings.
- [ ] `src/lib/session-filter.ts` (+test), `src/lib/stores/library.svelte.ts` (+test) -- lọc AND/untagged, tags state.
- [ ] `src/components/TagPicker.svelte` (+test), `src/components/SessionMenu.svelte` -- picker hai mode, quản lý, bàn phím/Esc/focus.
- [ ] `src/routes/home/TagFilterBar.svelte` (+test), `src/routes/Home.svelte`, `src/routes/home/SessionRow.svelte` (+test) -- hàng chip, lọc, gắn tag từ dòng.
- [ ] `src/routes/Session.svelte`, `src/routes/session/SessionHeader.svelte` (+test) -- chip tag + "+ Tag".
- [ ] `src/i18n/{vi,en,ja}.json` -- chuỗi mới.

**Acceptance Criteria:**
- Given app khởi động trên DB cũ (3 migration), when chạy migration, then có `tags`/`session_tags`, `user_version` = 4, dữ liệu cũ giữ nguyên.
- Given popover mở từ bất kỳ nút nào, when Esc hoặc click ngoài, then popover đóng và focus về đúng nút đã mở.
- Given gắn tag cho Phiên khi Home đang lọc, when gắn xong, then bộ lọc Home không đổi.

## Design Notes

Lọc chạy phía client trên `libraryStore.sessions` (≤ 500 phiên, đã tải hết cho virtual list) — tránh round-trip IPC mỗi lần bấm chip và để 3.3 ghép query tên vào cùng một hàm thuần `filterSessions(sessions, { tagIds, untagged, query })`. `name_key` dùng `to_lowercase` của Rust (Unicode-aware: `DỰ ÁN` → `dự án`); tiếng Nhật không có hoa/thường nên trim là đủ. NFC làm ở frontend vì không được thêm crate chuẩn hoá.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml --locked` -- pass
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` && `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` -- sạch
- `npm test && npm run check && npm run check:i18n && npm run check:ui && npm run check:deps` -- pass
