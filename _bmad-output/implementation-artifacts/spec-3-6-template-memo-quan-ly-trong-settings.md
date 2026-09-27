---
title: '3.6 Template memo — quản lý trong Settings'
type: 'feature'
created: '2026-09-27'
status: 'done'
baseline_commit: 'e7ec409c8a4a66d567e69ee55a845023250974fb'
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

**Problem:** Chưa có Template memo nào để Story 3.7 sinh memo; người dùng cần một bộ mẫu mặc định theo ngôn ngữ UI và một nơi duy nhất để thêm/sửa/xoá mẫu theo cách công ty họ viết 議事録.

**Approach:** Bảng `memo_templates` + module Rust `memo/templates` (seed mặc định theo locale, CRUD, khôi phục mặc định) và nhóm Settings "Memo" dạng master-detail với kiểm tra `{transcript}`/`{notes}` trực tiếp.

## Boundaries & Constraints

**Decision OQ8 (autorun — người dùng uỷ quyền chọn phương án đề xuất, 2026-09-27):** Viết lại dựa trên mẫu v2. Mỗi locale (vi/en/ja) có đúng 2 mẫu mặc định với `default_key` ổn định:
- `meeting-minutes` — tên vi "Biên bản họp" / en "Meeting minutes" / ja "議事録". Prompt (bằng ngôn ngữ đó): vai trò người ghi biên bản chuyên nghiệp; từ transcript (và ghi chú nếu có) viết biên bản bằng ngôn ngữ của mẫu, định dạng Markdown với các mục: Tóm tắt, Nội dung chính đã thảo luận, Quyết định, Việc cần làm (người phụ trách, hạn nếu có), Vấn đề còn mở; không bịa thông tin không có trong nguồn; chỉ trả về nội dung biên bản; sau đó khối `Ghi chú:` + `{notes}` và `Transcript:` + `{transcript}`.
- `bilingual-ja-vi` — tên vi "Memo song ngữ Nhật–Việt" / en "Japanese–Vietnamese memo" / ja "日越バイリンガルメモ". Prompt = mẫu v2 (tạo riêng memo tiếng Nhật và tiếng Việt, chỉ trả về nội dung memo) viết bằng ngôn ngữ của mẫu, cuối có `{notes}` và `{transcript}` như trên.
Văn bản chính xác nằm trong một file hằng số Rust và là nguồn duy nhất.

**Always:** Bảng `memo_templates(id TEXT PK, name TEXT NOT NULL, prompt TEXT NOT NULL, is_default INTEGER NOT NULL, locale TEXT NULL, default_key TEXT NULL, created_at INTEGER, updated_at INTEGER, UNIQUE(locale, default_key))`; id UUIDv7 do app sinh; tên/id không bao giờ là thành phần đường dẫn. `memo_templates_list(locale)` đảm bảo bộ mặc định của locale đó tồn tại (chèn nếu thiếu theo `(locale, default_key)`, không bao giờ nhân bản, không ghi đè bản mặc định người dùng đã sửa) rồi trả mẫu mặc định của locale đó + mọi mẫu người dùng (mặc định trước theo thứ tự cố định, rồi mẫu người dùng theo `created_at`). Mẫu mặc định sửa được (tên/prompt) nhưng không xoá được (Rust từ chối, UI vô hiệu nút). `memo_templates_restore_defaults(locale)` ghi lại tên/prompt gốc cho các mẫu mặc định của locale đó (tạo lại nếu thiếu), không đụng mẫu người dùng hay locale khác. Kiểm tra (cả Rust lẫn UI): tên trim 1–100 ký tự; prompt ≤ 20 000 ký tự và phải chứa `{transcript}`; `{notes}` tuỳ chọn. Lỗi kiểm tra → `Request`. Editor: hàng kiểm tra `{transcript}` (bắt buộc) và `{notes}` (tuỳ chọn) cập nhật theo từng lần gõ; thiếu `{transcript}` → dòng kiểm tra đỏ kèm giải thích và nút Lưu vô hiệu. Danh sách trái có badge "Mặc định · vi|en|ja" hoặc "Của bạn" và nút "+ Thêm mẫu" (tạo bản nháp với tên mặc định và prompt khung có sẵn `{transcript}`, chỉ ghi DB khi Lưu). Xoá mẫu người dùng: xác nhận inline trong editor (không dialog). "Khôi phục mẫu mặc định" cần xác nhận inline. Đổi mẫu đang chọn khi editor có thay đổi chưa lưu → xác nhận inline "Bỏ thay đổi?" trước khi chuyển. Nhóm Memo là nơi duy nhất thêm/sửa/xoá mẫu. Nhóm ẩn khi Consent bị từ chối (như các nhóm khác). Xoá toàn bộ dữ liệu (3.4) giữ nguyên `memo_templates`. Chuỗi UI mới đủ vi/en/ja; prompt textarea dùng font mono.

**Never:** Không sinh memo hay gọi Gemini ở story này. Không xoá/ghi đè mẫu người dùng khi khôi phục hay đổi locale. Không seed trong migration (migration chỉ tạo bảng). Không dùng dialog modal cho xoá/khôi phục.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Lần đầu mở | DB trống, locale `vi` | 2 mẫu mặc định vi | — |
| Mở lại nhiều lần | gọi list(`vi`) 3 lần | Vẫn đúng 2 mẫu mặc định vi | — |
| Đổi locale | list(`ja`) sau khi đã sửa mẫu vi | Có 2 mẫu ja; mẫu vi đã sửa giữ nguyên trong DB, không hiện ở list ja | — |
| Thiếu `{transcript}` | prompt "Tóm tắt {notes}" | UI: Lưu vô hiệu + dòng đỏ; Rust: từ chối | `Request` |
| Tên rỗng / >100 | `"   "` | Từ chối | `Request` |
| Xoá mặc định | delete(id mặc định) | Từ chối, không xoá | `Request` |
| Xoá mẫu người dùng | xác nhận inline | Mẫu biến mất, chọn mẫu khác | — |
| Khôi phục | mẫu vi mặc định đã sửa + 1 mẫu người dùng | Mặc định vi về gốc; mẫu người dùng còn | — |
| Xoá toàn bộ dữ liệu | có mẫu người dùng | Mẫu còn nguyên | — |

</frozen-after-approval>

## Code Map

- `src-tauri/src/db/migrations/mod.rs:35` -- `MIGRATIONS: [M; 5]` (5 = notes, story 3.5); thêm migration 6 `memo_templates`, cập nhật mọi assert `user_version` (test trong file này và `db/mod.rs`). Chỉ file này chứa `CREATE TABLE`.
- `src-tauri/src/db/repo/memo_templates.rs` (mới, khai báo trong `repo/mod.rs`) -- SQL duy nhất cho bảng: list, insert-if-missing theo `(locale, default_key)`, insert, update, delete, reset default. Mẫu tham khảo `db/repo/tags.rs`, `db/repo/notes.rs`.
- `src-tauri/src/core/id.rs:11` -- macro `uuid_v7_id!`; thêm `MemoTemplateId` (mẫu `TagId`).
- `src-tauri/src/memo/mod.rs` -- đang là stub; thêm `memo/templates.rs` (validate, seed, CRUD, restore, kiểu IPC `MemoTemplate { id, name, prompt, isDefault, locale, defaultKey }`) và `memo/defaults.rs` (hằng số prompt 3 locale × 2 mẫu theo Decision OQ8). Locale nhận chuỗi `vi|en|ja`, giá trị khác → lỗi `Request`.
- `src-tauri/src/ipc/mod.rs` -- mẫu lệnh `tags_create`/`library_session_rename` (`blocking`, `track_ipc_error`); thêm `memo_templates_list`, `memo_template_create`, `memo_template_update`, `memo_template_delete`, `memo_templates_restore_defaults`; đăng ký trong `specta_builder`; `npm run bindings` tái sinh `src/lib/bindings.ts`.
- `src-tauri/src/library/store.rs` `wipe_all` -- không đụng `memo_templates`; thêm assert vào test wipe hiện có rằng bảng này còn nguyên.
- `src/routes/Settings.svelte` -- mảng `groups` + nhánh `{#if}`; thêm nhóm `memo` (`settings.group.memo`) sau `chunking`; cập nhật `Settings.groups.test.ts`.
- `src/routes/settings/SettingsMemo.svelte` (mới, +test) -- master-detail; có thể tách `src/routes/settings/memo/TemplateEditor.svelte`. Mẫu UI/test: `SettingsStorage.svelte` (3.4), `SettingsChunking.svelte`. Locale hiện tại: `i18n.locale` (`src/i18n/index.svelte.ts:44`); khi locale đổi thì nạp lại danh sách.
- `src/lib/stores/memoTemplates.svelte.ts` (mới, +test) -- store theo mẫu `createXStore()`; mock `commands` như `library.svelte.test.ts`.
- `src/i18n/{vi,en,ja}.json` -- nhãn nhóm, badge, editor, kiểm tra placeholder, xác nhận inline, lỗi.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/db/migrations/mod.rs`, `src-tauri/src/db/repo/{memo_templates,mod}.rs`, `src-tauri/src/core/id.rs` (+test) -- bảng, ràng buộc unique, repo.
- [x] `src-tauri/src/memo/{mod,templates,defaults}.rs` (+test) -- seed idempotent, validate, CRUD, không xoá mặc định, restore chỉ locale đó, mọi mặc định chứa `{transcript}` và `{notes}`.
- [x] `src-tauri/src/ipc/mod.rs`, `src-tauri/src/library/store.rs` (test) -- 5 lệnh, bindings; wipe giữ template.
- [x] `src/lib/stores/memoTemplates.svelte.ts` (+test) -- list theo locale, create/update/delete/restore.
- [x] `src/routes/settings/SettingsMemo.svelte` (+ editor con, +test), `src/routes/Settings.svelte`, `src/routes/Settings.groups.test.ts` -- master-detail, kiểm tra trực tiếp, xác nhận inline.
- [x] `src/i18n/{vi,en,ja}.json` -- chuỗi mới.

**Acceptance Criteria:**
- Given nhóm Memo, when mở, then thấy danh sách trái (badge đúng) + "+ Thêm mẫu" và editor tên + textarea prompt mono bên phải.
- Given editor, when chỉ dùng bàn phím, then chọn mẫu, sửa, lưu, xoá (xác nhận inline) và khôi phục được mà không cần chuột.
- Given đổi ngôn ngữ UI khi đang ở nhóm Memo, when danh sách nạp lại, then hiện mặc định của ngôn ngữ mới và mẫu người dùng, không nhân bản.

## Implementation Notes

- `memo_templates` migration (6th, `M; 6]`) không seed gì (spec Never) -- chỉ `CREATE TABLE`; hai cột `locale`/`default_key` để `NULL` cho mẫu người dùng, `UNIQUE(locale, default_key)` cho phép nhiều dòng `(NULL, NULL)` (SQLite coi NULL không trùng nhau trong UNIQUE).
- `db::repo::memo_templates::list` lọc `(is_default = 1 AND locale = ?1) OR is_default = 0`: mẫu người dùng không gắn locale nên luôn hiện ở mọi locale, chỉ hai mẫu *mặc định* mới lọc theo locale UI hiện tại -- đây là điểm khác với ban đầu tôi (agent) ngờ vực khi đọc spec, nhưng khớp đúng I/O Matrix "Đổi locale" (mẫu người dùng không "biến mất" khi đổi ngôn ngữ UI).
- `ensure_default` (`ON CONFLICT DO NOTHING`, dùng trong `list`, lười) và `restore_default` (`ON CONFLICT DO UPDATE`, dùng trong `restore_defaults`) là hai câu SQL riêng biệt tuy cùng shape -- gộp chung sẽ mất khả năng phân biệt "đừng đụng nếu đã có" với "ghi đè về gốc".
- `memo::templates::validate_prompt` là `pub` vì cùng logic ({transcript} bắt buộc, ≤ 20 000 ký tự) được lặp lại phía UI (`TemplateEditor.svelte`) cho kiểm tra tức thời -- Rust vẫn là nguồn thật, UI chỉ chặn *gửi đi* sớm.
- `db::repo::memo_templates` không tự chặn xoá mẫu mặc định bằng SQL (`AND is_default = 0` chỉ là lớp bảo vệ thứ hai) -- quyết định "mặc định hay không" đọc dòng trước ở `memo::templates::delete`, để phân biệt được "không tồn tại" (`Ok`, idempotent) với "là mặc định" (`Err`, `Code::Request`).
- `library::store::wipe_all` không cần sửa gì để giữ `memo_templates` -- bảng này chưa từng nằm trong `DELETE FROM sessions`/`DELETE FROM tags`, chỉ thêm test xác nhận thay vì đổi hành vi.
- Frontend: `SettingsMemo.svelte` ban đầu tôi (agent) viết chọn dòng đầu tiên bằng cách đọc `memoTemplatesStore.templates` ngay sau `await memoTemplatesStore.load(locale)` bên trong effect nạp locale -- việc này tạo ra một nhịp reactive riêng, trễ hơn nhịp cập nhật danh sách một chu kỳ flush, khiến test `findByText(tên mẫu)` thấy danh sách nhưng editor bên phải vẫn trống một khoảnh khắc. Sửa bằng cách tách thành một `$effect` thứ hai đọc trực tiếp `templates` (derived) và tự chọn dòng đầu khi `selectedId` không còn hợp lệ -- gộp cùng chu kỳ flush với chính `templates`, đồng thời khiến các chỗ set `selectedId` sau xoá/khôi phục ở `handleDeleted`/`confirmRestore` trở thành thừa (đã bỏ, effect này lo hết).
- `TemplateEditor.svelte` dùng `<label for=id>` tường minh (không bọc input trong `<label>`) cho trường Prompt vì nhãn đó có thêm một dòng help-text nằm cạnh -- nếu bọc kiểu `InlineRename`, `getByLabelText`/trình đọc màn hình sẽ tính luôn cả help-text vào tên nhãn, làm sai accessible name.
- Nút "Xoá mẫu" cho mẫu mặc định dùng `DisabledHint` (component có sẵn, `aria-disabled` + tooltip lý do, focus được bằng Tab) thay vì ẩn hẳn nút -- khớp "UI vô hiệu nút" trong spec mà vẫn giữ nút trong luồng Tab.
- Banner lỗi lưu/xoá/khôi phục dùng chung `errorTitle`/`errorHint` theo `category` (như `SettingsStorage`) thay vì tự đặt chuỗi lỗi riêng cho từng thao tác -- vì lớp bảo vệ chính (không cho gửi request sai) đã nằm ở kiểm tra trực tiếp phía client; lỗi Rust chỉ còn là trường hợp hiếm (đua, bypass validate).

## Spec Change Log

## Review Triage Log

## Design Notes

Seed lười trong `memo_templates_list(locale)` (thay vì lúc boot) vì chỉ frontend biết locale UI thực tế (có thể là "system"); `UNIQUE(locale, default_key)` + `INSERT ... ON CONFLICT DO NOTHING` bảo đảm không nhân bản kể cả khi gọi đồng thời. Mẫu mặc định vẫn là dòng thường có thể sửa — "khôi phục" chỉ ghi đè theo `default_key` của locale được chọn. Story 3.7 sẽ snapshot tên/prompt vào memo nên xoá mẫu không ảnh hưởng memo đã sinh.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml --locked` -- pass (447 tests, gồm 6 test mới `db::migrations`, 6 `db::repo::memo_templates`, 3 `memo::defaults`, 11 `memo::templates`, 1 `library::store::wipe_all_keeps_memo_templates`)
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check && cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` -- sạch
- `npm run bindings` -- tái sinh `src/lib/bindings.ts` (thêm `memoTemplatesList`/`memoTemplateCreate`/`memoTemplateUpdate`/`memoTemplateDelete`/`memoTemplatesRestoreDefaults`, kiểu `MemoTemplate`)
- `npm test && npm run check && npm run check:i18n && npm run check:ui && npm run check:deps` -- pass (585 tests, gồm 11 `memoTemplates.svelte.test.ts`, 11 `SettingsMemo.test.ts`, 11 `TemplateEditor.test.ts`, +1 `Settings.groups.test.ts`; `npm run check` 344 file 0 lỗi; `check:i18n`/`check:ui`/`check:deps` sạch). 3 lỗi "Unhandled Errors" không liên quan (`effect_update_depth_exceeded` trong `Session.test.ts`) đã xác nhận tồn tại từ trước trên baseline `e7ec409` (chạy lại trên baseline cho kết quả giống hệt), không phải do thay đổi của story này.
