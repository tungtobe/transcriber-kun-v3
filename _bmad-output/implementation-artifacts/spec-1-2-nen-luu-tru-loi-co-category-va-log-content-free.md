---
title: 'Story 1.2 — Nền lưu trữ, lỗi có category và log content-free'
type: 'feature'
created: '2026-09-22'
status: 'in-review'
baseline_commit: '0759e30cd2730c45242b56b52d23d53936b3991f'
route: 'full'
route_source: 'auto'
review: 'thorough'
review_source: 'auto'
lenses_ran: ['blind-hunter', 'edge-case-hunter', 'verification-gap', 'intent-alignment']
review_loop_iteration: 0
context:
  - '{project-root}/_bmad-output/implementation-artifacts/epic-1-context.md'
  - '{project-root}/docs/adr/0001-tauri-specta-channel-typed.md'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** App chưa có chỗ lưu bền, chưa có kiểu lỗi chung và chưa có log; mọi story sau (key, settings, phiên) cần ba nền này và cần bảo đảm log không bao giờ lộ nội dung họp hay key.

**Approach:** Thêm vào `src-tauri`: `db/` giữ một connection SQLite WAL trong Container với migration chỉ tiến (chỉ bảng `settings`); `settings/` typed có mặc định, phát event khi đổi; `core/` có `AppError` 8 category, `Sensitive<T>`, ID UUIDv7 + helper đường dẫn an toàn; `tracing` ghi log xoay vòng qua lớp redaction, có test grep log sau phiên giả.

## Boundaries & Constraints

**Always:**
- `category` serialize là một trong `quota | auth | model | network | format | permission | storage | blocked`; `AppError` serialize `{ category, code, detailRedacted }`; ánh xạ `code → category` chỉ ở `core/error`, test phủ mọi biến thể `code`.
- Mọi command trả `Result<T, AppError>` (kể cả `app_version` hiện có).
- Một `rusqlite::Connection` duy nhất, WAL, do `db/` giữ; SQL chỉ trong `db/repo/<entity>.rs` và `db/migrations/mod.rs` (migration là hằng chuỗi SQL trong Rust, chỉ tiến).
- `app.db` nằm ở thư mục dữ liệu app do OS/sandbox cấp (`app_data_dir`), không cấu hình được.
- Settings chỉ đọc/ghi qua `settings/` bằng command `settings_get` / `settings_save`; giá trị lưu thiếu hoặc hỏng → dùng mặc định của khoá đó, không lỗi.
- `Sensitive<T>` in `{:?}`/`{}` ra `[redacted]`; redaction thay `AIza…`, `AQ.…`, URL `http(s)://…` và giá trị header `authorization` trong mọi dòng log trước khi ghi file.
- ID Phiên/Transcript/Job là newtype UUIDv7; helper đường dẫn chỉ nhận newtype ID; parse từ chuỗi chỉ chấp nhận UUIDv7 hợp lệ.
- Crate có trong bảng Stack pin exact theo bảng.

**Decisions (người dùng chốt 2026-09-22):**
- `Code` gồm 12 biến thể: `Quota→quota`, `Auth→auth`, `Model→model`, `Request→model`, `Shape→model`, `Timeout→network`, `Network→network`, `Tls→network`, `Blocked→blocked`, `Format→format`, `Permission→permission`, `Storage→storage`. Lỗi DB/IO dùng `Storage`.
- `Settings` ở story này chỉ có một khoá `theme: 'system' | 'light' | 'dark'`, mặc định `system`; story sau thêm khoá.

**Never:**
- Không `tauri-plugin-store`, không thêm plugin Tauri nào.
- Không bảng nào ngoài `settings`; không bảng `jobs`.
- Không tạo file `*.sql` (hook bảo mật của repo chặn).
- Không làm xuất/xoá Nhật ký chẩn đoán, bộ đếm cục bộ, UI Settings, store settings frontend (story 1.9/1.10).
- Không gửi bất kỳ log/thống kê nào ra ngoài máy.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Lần chạy đầu | Container chưa có `app.db` | Tạo `app.db` WAL, chạy migration, bảng `settings` tồn tại | — |
| Chạy lại | `app.db` đã ở version mới nhất | Migration không làm gì, dữ liệu giữ nguyên | — |
| DB không mở được | Thư mục không ghi được | App vẫn khởi động; `settings_get`/`settings_save` trả `AppError` category `storage` | Log lỗi (đã redact), không panic |
| Settings chưa lưu | Bảng rỗng | `settings_get` trả toàn bộ giá trị mặc định | — |
| Settings hỏng | Một hàng có value không parse được | Khoá đó về mặc định, khoá khác giữ giá trị | Log cảnh báo không chứa value |
| Lưu settings | `settings_save` hợp lệ | Ghi bền, phát event `settingsChanged` một lần với giá trị mới | Lỗi ghi → `storage`, không phát event |
| Log có bí mật | Chuỗi chứa `AIzaSy…`, `AQ.…`, `https://…?key=…`, `authorization: Bearer …` | File log chỉ còn dạng đã thay thế | — |
| ID từ người dùng | Parse `"../etc"` hoặc UUIDv4 thành `SessionId` | Bị từ chối (`Err`) | Không tạo đường dẫn |

</frozen-after-approval>

## Code Map

- Continuity 1.1: Rust ≥ 1.93 (specta rc.25); `serde_json` hiện chỉ ở `[dev-dependencies]` — chuyển về `[dependencies]` nếu `settings/` cần; field đếm qua IPC dùng `u32` (specta-typescript cấm `u64`); rc.25 chỉ có `Builder::export(lang, path)`, event dùng `collect_events!` + `mount_events`.
- `src-tauri/src/ipc/mod.rs` -- `app_version` (đổi sang `Result<String, AppError>`), `specta_builder()` là nơi duy nhất liệt kê command/event; test `export_bindings` sinh `src/lib/bindings.ts`.
- `src-tauri/src/lib.rs` -- `run()` hiện chỉ mount builder; thêm boot vào `setup`.
- `src-tauri/src/{core,db,settings}/mod.rs` -- đang là stub một dòng.
- `src/lib/stores/app.svelte.ts` + test -- store đang coi `error: string`; cập nhật theo `AppError` sau khi sinh lại binding.
- Command Tauri đồng bộ chạy trên main thread → command chạm DB phải `async` và đẩy việc DB sang `tauri::async_runtime::spawn_blocking`.
- Crate mới (đã kiểm tương thích): `rusqlite =0.40.2` (bundled), `rusqlite_migration =2.6.0`, `tracing =0.1.44`, `tracing-appender =0.2.5`, `uuid =1.26.1` (v7), `tracing-subscriber 0.3.23`, `regex 1.13.1`, dev `tempfile 3.27.0`.

## Tasks & Acceptance

**Execution:**
- [ ] `src-tauri/Cargo.toml`, `Cargo.lock` -- thêm crate ở Code Map -- nền.
- [ ] `src-tauri/src/core/{mod,error,sensitive,id,paths}.rs` -- `AppError` + enum `Category`/`Code` + `Code::category()` (một `match`, không nhánh `_`), `From` lỗi rusqlite/io → code lưu trữ; `Sensitive<T>`; `SessionId`/`TranscriptId`/`JobId` (UUIDv7, `new()`, `TryFrom<&str>` chỉ nhận v7); helper `media_dir(root, SessionId)`, `staging_dir(root, JobId)` -- AD-5, AD-7, AD-15.
- [ ] `src-tauri/src/core/log.rs` -- khởi tạo `tracing` ghi file xoay vòng hằng ngày trong `app_log_dir`, giữ tối đa 7 file, qua writer redaction áp cho mọi dòng; hàm redact thuần để test -- FR-41, AR-29.
- [ ] `src-tauri/src/db/{mod,migrations/mod,repo/mod,repo/settings}.rs` -- `Db::open(dir)` bật WAL, chạy migration (hằng chuỗi SQL); repo `settings` đọc tất cả / upsert nhiều khoá trong một transaction -- AD-4.
- [ ] `src-tauri/src/settings/mod.rs` -- struct `Settings` (camelCase, specta `Type`) với mặc định từng khoá, đọc từ repo (value JSON), ghi toàn bộ; event `SettingsChanged(Settings)` -- AD-8.
- [ ] `src-tauri/src/ipc/{mod,boot}.rs`, `src-tauri/src/lib.rs` -- `boot` resolve thư mục, khởi tạo log, mở DB, `manage` state (DB lỗi vẫn boot, giữ lỗi `storage`); command `settings_get`, `settings_save` (async); đăng ký event; sinh lại binding -- AD-18 (chủ boot duy nhất).
- [ ] `src/lib/stores/app.svelte.ts`, `src/lib/stores/app.svelte.test.ts`, `src/lib/bindings.ts` -- store dùng kiểu `AppError` sinh ra -- giữ binding typed.
- [ ] Test Rust -- phủ từng dòng I/O Matrix: DB tạm (`tempfile`) cho lần đầu/chạy lại/không ghi được/hỏng/lưu + event; bảng `code → category` phủ mọi biến thể; `Sensitive` Debug/Display; redact từng mẫu; parse ID; test "phiên mẫu" ghi log thật vào thư mục tạm với transcript + key giả (qua `Sensitive` và qua chuỗi lỗi thô chứa key/URL) rồi grep mọi file log không thấy chuỗi gốc.

**Acceptance Criteria:**
- Given app mới cài, when khởi động, then `app.db` và `app.db-wal` xuất hiện trong `app_data_dir`, `PRAGMA journal_mode` = `wal`, `PRAGMA user_version` = 1.
- Given binding sinh lại, when đọc `src/lib/bindings.ts`, then có `settingsGet`, `settingsSave`, event `settingsChanged` và kiểu `AppError` với union 8 category.
- Given `grep -rn "tauri-plugin-store\|CREATE TABLE" src-tauri/src`, when chạy, then chỉ khớp `CREATE TABLE settings` trong `db/migrations/mod.rs`.

## Implementation Notes

## Spec Change Log

## Review Triage Log

## Verification

**Commands:**
- `cargo test --locked --manifest-path src-tauri/Cargo.toml` -- expected: pass, gồm test I/O Matrix và test grep log
- `npm run check:deps && npm run check && npm run build && npm test` -- expected: exit 0
- `git diff --exit-code src/lib/bindings.ts` -- expected: không lệch sau khi chạy test

**Manual checks (if no CLI):**
- `npm run tauri dev` trên macOS: app mở như cũ; `app.db` và file log mới trong `~/Library/Application Support/com.transkun.app` và `~/Library/Logs/com.transkun.app`.
