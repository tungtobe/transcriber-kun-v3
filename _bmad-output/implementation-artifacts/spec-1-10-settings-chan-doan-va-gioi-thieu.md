---
title: 'Story 1.10 — Settings: Chẩn đoán và Giới thiệu & Quyền riêng tư'
type: 'feature'
created: '2026-09-23'
status: 'done'
baseline_commit: 'adbf2a3c34f87fc1eb9aa1bc07da84941224fbe7'
route: 'full'
route_source: 'auto'
review: 'none'
review_source: 'pinned'
lenses_ran: []
review_loop_iteration: 0
context:
  - '{project-root}/_bmad-output/implementation-artifacts/epic-1-context.md'
  - '{project-root}/_bmad-output/implementation-artifacts/spec-1-9-settings-khung-chung-va-gemini.md'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** Nhóm Chẩn đoán và Giới thiệu & Quyền riêng tư vẫn là placeholder. Người dùng và người hỗ trợ không xuất được log an toàn, không xoá được log, không xem được bộ đếm cục bộ, và không xem lại được thông tin app hay văn bản đã đồng ý.

**Approach:** Thêm module Rust `diagnostics` gồm: xuất một file gói văn bản (chỉ gồm file log trong allow-list, redact lại lần nữa, kèm phần tóm tắt bộ đếm) qua dialog lưu hệ thống mở từ Rust (`tauri-plugin-dialog`); xoá log; bộ đếm cục bộ (phiên, lỗi theo category, crash theo marker "tắt sạch") lưu trong bảng mới. UI có hai nhóm Settings tương ứng.

## Boundaries & Constraints

**Always:**
- **Allow-list:** chỉ các file thường trực tiếp trong thư mục log, tên khớp `trans-kun` + hậu tố rotation của `core::log`. Bỏ qua symlink, thư mục con và mọi file khác. Nội dung đi qua `redact()` trước khi ghi.
- **Định dạng gói:** một file `.txt` UTF-8 (tên gợi ý `trans-kun-diagnostics-YYYYMMDD.txt`), gồm header version/OS, bộ đếm, rồi từng file log có tiêu đề ngăn cách. Không thêm crate nén.
- **Dialog lưu:** mở phía Rust bằng `tauri-plugin-dialog` pin exact `2.7.3`. Frontend không nhận hay gửi đường dẫn. Huỷ dialog → trả `false`, không lỗi.
- **Xoá nhật ký:** xoá các file allow-list cũ. File của ngày hiện tại thì truncate thay vì xoá, để chạy được cả trên Windows khi file đang mở.
- **Bộ đếm:** migration tiến mới tạo bảng `local_counters(key TEXT PRIMARY KEY, value INTEGER NOT NULL)`, SQL chỉ ở `db/repo/counters.rs`.
  - Lỗi: tăng theo category khi một IPC command trả `Err`, qua một helper duy nhất ở `ipc/`.
  - Phiên: có API tăng đếm nhưng chưa ai gọi (Epic 2), nên hiển thị 0.
  - Crash: lúc boot, nếu marker `clean_shutdown` của lần trước là false thì tăng crash, rồi đặt marker = false. Khi `RunEvent::Exit` thì đặt marker = true.
- **Riêng tư của bộ đếm:** chỉ là số, không nội dung, không gửi đi đâu.
- **Giới thiệu:** hiện version qua `appVersion`, tác giả "Relipa", liên hệ hỗ trợ `SUPPORT_URL = https://transkun.app/support` (hằng số cạnh `PRIVACY_URL`, trong scope opener sẵn có), link Privacy Policy mở bằng `openUrl`. "Xem lại văn bản đồng ý" hiển thị chỉ-đọc cùng nội dung i18n của bước Consent, kèm "Văn bản đồng ý phiên bản N". Hoạt động cả khi Consent bị từ chối (chế độ chỉ About).
- **i18n:** chuỗi mới đủ vi/en/ja.

**Never:**
- Không có checkbox thống kê/opt-in, không request mạng nào.
- Không gọi `acceptConsent`/`declineConsent` hay đổi `consentAcceptedVersion` từ màn xem lại.
- Không dùng plugin bị cấm (shell/fs/updater/process/http/store), không cấp capability dialog cho frontend.
- Không đưa đường dẫn tuyệt đối hay tên người dùng của máy vào gói.
- Không sửa tay `bindings.ts`.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|---------------|---------------------------|----------------|
| Xuất gói | thư mục log có `trans-kun.2026-09-22` + `secret.txt` + thư mục con | Gói chỉ chứa log allow-list + tóm tắt | — |
| Phiên mẫu giả | log ghi transcript/dịch/ghi chú/memo/key dạng `Sensitive` + key thô | Grep gói không thấy nội dung hay key | — |
| Huỷ dialog | người dùng bấm Cancel | Trả `false`, UI không báo lỗi | — |
| Ghi gói lỗi | đường dẫn không ghi được | Banner category `storage` | Không panic |
| Xoá nhật ký | có file cũ + file hôm nay | File cũ bị xoá, file hôm nay rỗng; file ngoài allow-list còn nguyên | Lỗi IO → `storage` |
| Crash | lần chạy trước không đặt marker sạch | Bộ đếm crash +1 ở lần boot sau | DB lỗi → boot vẫn tiếp tục |
| Lỗi IPC | command trả `Err(auth)` | Bộ đếm `auth` +1 | Ghi đếm lỗi không làm đổi lỗi gốc |
| Xem lại Consent | đã đồng ý v1 | Hiện văn bản + "phiên bản 1"; `consentAcceptedVersion` không đổi | — |

</frozen-after-approval>

## Code Map

- `src-tauri/src/core/log.rs` -- `redact()` (L47), `MAX_LOG_FILES`/prefix `trans-kun` (L16-17), test mẫu `fake_session_log_never_leaks_raw_secrets_to_disk` (L193) để mở rộng. Xuất hằng/prefix dùng chung cho allow-list.
- `src-tauri/src/diagnostics/mod.rs` (mới) -- allow-list, build bundle (hàm thuần nhận `log_dir` + counters → String, test được không cần Tauri), clear logs.
- `src-tauri/src/db/migrations/mod.rs` (L11-16) -- thêm migration 2; `src-tauri/src/db/repo/counters.rs` (mới) theo mẫu `repo/settings.rs`.
- `src-tauri/src/ipc/boot.rs` (L35 log dir qua `app_log_dir()`) -- xử lý crash marker, lưu `log_dir` vào `AppState`. `src-tauri/src/lib.rs` -- `.plugin(tauri_plugin_dialog::init())`, `.run(|app, event| …)` đặt marker khi `RunEvent::Exit`.
- `src-tauri/src/ipc/mod.rs` -- command `diagnostics_summary`, `diagnostics_export` (async, dialog `blocking_save_file` trong `spawn_blocking`), `diagnostics_clear_logs`; helper ghi đếm lỗi áp cho mọi command trả `Result`; đăng ký trong `specta_builder()` (L259-271).
- `src-tauri/src/consent/mod.rs` -- thêm `SUPPORT_URL` cạnh `PRIVACY_URL` (L15), đưa vào `ConsentPolicy` hoặc command riêng `app_info`.
- `src-tauri/Cargo.toml`, `Cargo.lock` -- `tauri-plugin-dialog = "=2.7.3"`; không thêm capability frontend (`capabilities/default.json` giữ nguyên).
- `src/routes/Settings.svelte` (L53-58 placeholder) -- render `SettingsDiagnostics`/`SettingsAbout`.
- `src/routes/settings/{SettingsDiagnostics,SettingsAbout}.svelte` (mới) -- dùng `SettingsRow`, `BannerStack`, `errors.ts`; About tái dùng markup/i18n consent ở `Onboarding.svelte:173-190` (tách thành `src/components/ConsentText.svelte` dùng chung, Onboarding giữ hành vi).
- `src/lib/stores/diagnostics.svelte.ts` (mới) -- bọc ba command.
- `src/i18n/{vi,en,ja}.json` -- `settings.diagnostics.*`, `settings.about.*`.

## Tasks & Acceptance

**Execution:**
- [x] `db` migration + `repo/counters.rs` (+ tests) -- get/increment/set marker.
- [x] `diagnostics/mod.rs` (+ tests) -- allow-list, bundle, clear; tests phủ các dòng Matrix xuất/xoá và grep phiên mẫu giả.
- [x] `ipc/{boot,mod}.rs`, `lib.rs`, `Cargo.toml` -- plugin dialog, crash marker, commands, đếm lỗi; regenerate bindings.
- [x] `ConsentText.svelte` + refactor `Onboarding.svelte` (test Onboarding hiện có vẫn pass).
- [x] `diagnostics.svelte.ts`, `SettingsDiagnostics.svelte`, `SettingsAbout.svelte`, `Settings.svelte`, i18n (+ tests).

**Acceptance Criteria:**
- Given nhóm Chẩn đoán, when hiển thị, then thấy số Phiên, lỗi theo từng category (8 dòng, gồm 0), số crash; không có checkbox thống kê.
- Given nhóm Giới thiệu, when mở, then có version, tác giả, link hỗ trợ và Privacy Policy mở trình duyệt ngoài, và nút xem lại văn bản đồng ý.
- Given Consent bị từ chối, when vào Settings, then nhóm About vẫn đủ nội dung trên và không có request mạng.

## Implementation Notes

- Dialog: `tauri-plugin-dialog 2.7.3` kéo theo `tauri-plugin-fs` (dep bắt buộc) làm `check:deps` fail. Người dùng chọn dùng thẳng `rfd` (pin `=0.17.2`); dialog mở qua `app.run_on_main_thread` (yêu cầu macOS), kết quả trả về qua channel; `check-forbidden-deps.mjs` và test của nó không bị sửa.
- `diagnostics/mod.rs`: allow-list theo prefix log (bỏ symlink/thư mục con), `build_bundle` thêm `scrub_paths_and_username` ngoài `redact()`, `clear_logs` xoá file cũ và truncate file hôm nay.
- Bộ đếm ở bảng mới `local_counters` (migration 2); `track_ipc_error` bọc mọi command, lỗi ghi đếm không làm đổi lỗi gốc; crash đếm qua marker đặt ở `RunEvent::Exit`.
- `SUPPORT_URL` thêm vào `ConsentPolicy` thay vì command mới. `ConsentText.svelte` tách từ Onboarding và dùng lại chỉ-đọc ở About.
- Hai test "placeholder không đổi" của story 1.9 được thay bằng test nội dung thật, đúng như 1.9 đã hoãn sang 1.10.

## Verification

**Commands:**
- `cargo test --locked --manifest-path src-tauri/Cargo.toml` -- pass.
- `npm run bindings && git diff --exit-code src/lib/bindings.ts` -- không drift sau khi sinh.
- `npm run check && npm test && npm run check:i18n && npm run check:ui && npm run check:deps && npm run build && npm run check:build-assets` -- pass.

**Manual checks (if no CLI):**
- `npm run tauri dev`: Settings → Chẩn đoán → "Xuất gói nhật ký" mở dialog lưu hệ thống; file lưu ra không chứa key.
