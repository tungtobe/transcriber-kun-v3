---
title: 'Story 1.9 — Settings: khung, Chung và Gemini'
type: 'feature'
created: '2026-09-23'
status: 'done'
baseline_commit: 'ed370c7fa6831780ead941e4ba8bd2a5ce62d273'
route: 'full'
route_source: 'auto'
review: 'none'
review_source: 'pinned'
lenses_ran: []
review_loop_iteration: 0
context:
  - '{project-root}/_bmad-output/implementation-artifacts/epic-1-context.md'
  - '{project-root}/_bmad-output/implementation-artifacts/spec-1-8-onboarding-nhap-key-va-home-khoi-dau.md'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** `/settings/:group` chỉ là placeholder; người dùng không có nơi đổi ngôn ngữ/theme, quản lý key hay chọn model, nên key sai hoặc model đổi tên thì không tự sửa được.

**Approach:** Dựng khung Settings (nav trái 220px + bảng hai cột `220px | control`, mỗi trường có (?) tooltip bàn phím + helper text bền), nhóm Chung (ngôn ngữ, theme) và nhóm Gemini (key, ba model). Thêm ba trường model vào Settings Rust (per-key JSON sẵn có, mặc định từ `gemini/params.rs`), cùng helper cảnh báo lỗi `model` dùng lại được.

## Boundaries & Constraints

**Always:** Đổi ngôn ngữ/theme áp dụng tức thì và lưu bền qua setter optimistic của `settingsStore`. Model lưu là chuỗi tự do `transcribeModel`/`liveModel`/`memoModel`; rỗng hoặc chỉ khoảng trắng bị chặn inline ngay khi nhập (không lưu). Rust `save` cũng từ chối rỗng (`format`); `load` fallback về mặc định khi thiếu hoặc hỏng. Tên không có trong danh sách đã tải vẫn lưu được, kèm cảnh báo nhẹ. "Tải danh sách" gọi `modelsList(kind)` theo từng select; trong lúc chạy thì nút ở trạng thái busy nhưng ô model vẫn sửa được. Lỗi tải hiện Banner category tại chỗ, không đổi giá trị đang cấu hình. Danh sách live chỉ lấy từ `modelsList('live')`. Ô key: `type=password` + hiện/ẩn; "Kiểm tra key" dùng `keysStore.checkKeys`, kết quả trong `role=status` kèm thời điểm kiểm tra (giờ cục bộ). Danh sách key hiển thị label đã mask, mỗi key có nút xoá gọi `keysDelete` (xoá khỏi kho khoá OS thật). Chỉ gọi Google khi người dùng bấm nút. Mọi chuỗi mới đủ vi/en/ja. Helper `modelErrorBanner(error)` tạo `BannerItem` warning với lối tắt `/settings/gemini`, không bao giờ tự đổi model.

**Never:** Không nối model từ settings vào gateway/transcribe (Epic 2), không thêm nhóm Chunking/Live/Memo/Lưu trữ/Cấu hình đề xuất, không làm nội dung Chẩn đoán/Giới thiệu (Story 1.10; giữ placeholder hiện có), không đổi hành vi router khi từ chối Consent, không gọi mạng nền/poll, không log hay hiển thị secret key, không sửa tay `bindings.ts`.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|---------------|---------------------------|----------------|
| Đổi theme/ngôn ngữ | chọn Tối / `ja` ở nhóm Chung | UI đổi ngay, reload vẫn giữ | Lưu lỗi → rollback + banner storage |
| Tải danh sách | bấm ở select live | Busy; ô vẫn sửa được; gợi ý chỉ gồm model Live | — |
| Tải lỗi | modelsList trả network/auth | Banner category tại chỗ; giá trị cũ giữ nguyên | Không xoá, không đổi model |
| Tên ngoài danh sách | gõ `my-custom-model` | Lưu, kèm cảnh báo nhẹ | — |
| Rỗng | xoá hết ô model | Lỗi inline ngay, không gọi save | Rust save rỗng → `format` |
| Kiểm tra key | key hợp lệ | status "Key hợp lệ, N model khả dụng · HH:MM" | Lỗi → tiêu đề category + hướng dẫn |
| Xoá key | bấm xoá một key | `keysDelete(id)`; danh sách cập nhật; `hasUsableKey` tính lại | Lỗi storage → banner, danh sách giữ nguyên |
| Settings cũ | DB thiếu khoá model | load trả mặc định `params.rs` | Không lỗi |

</frozen-after-approval>

## Code Map

- `src-tauri/src/settings/mod.rs` -- thêm 3 trường `String` + `KEY_*` + nhánh load (fallback per-key) / save (validate non-empty trim); mặc định lấy từ `gemini::params::DEFAULT_{TRANSCRIBE,LIVE,MEMO}_MODEL` (hoặc chuyển const sang nơi settings import được, giữ hướng phụ thuộc feature không import lẫn nhau — nếu cần, định nghĩa default trong `core`/settings và để params tham chiếu). Cập nhật test hiện có.
- `src/lib/bindings.ts` -- regenerate bằng `npm run bindings`.
- `src/lib/stores/settings.svelte.ts` -- `DEFAULT_SETTINGS` + setter `setModel(kind, name)` theo mẫu optimistic/persist/rollback (L270-359).
- `src/lib/stores/keys.svelte.ts` -- thêm `deleteKey(id)` và `lastCheckedAt`; giữ nguyên `checkKeys`.
- `src/lib/errors.ts` -- thêm `modelErrorBanner(error)`.
- `src/routes/Settings.svelte` -- khung hiện có (group param L17, `visibleGroups` L18-20, grid L84-134); render `SettingsGeneral`/`SettingsGemini` theo group, giữ placeholder cho diagnostics/about.
- `src/routes/settings/{SettingsGeneral,SettingsGemini}.svelte` (mới) -- nội dung nhóm.
- `src/components/{SettingsRow,HelpTip}.svelte` (mới) -- hàng `220px | control` với label, (?) tooltip focus được (`role=tooltip`, `aria-describedby`), helper text bền.
- Tái dùng `Banner`/`BannerStack`, `DisabledHint`, icons, mẫu theme select ở `AppShell.svelte:119-136` (giữ nguyên select ở header).
- `src/i18n/{vi,en,ja}.json` -- `settings.general.*`, `settings.gemini.*`, `settings.help.*`.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/settings/mod.rs` (+ tests) -- trường model, default, load fallback, save từ chối rỗng; regenerate bindings.
- [x] `src/lib/stores/{settings,keys}.svelte.ts` (+ tests) -- `setModel`, `deleteKey`, `lastCheckedAt`.
- [x] `src/components/{SettingsRow,HelpTip}.svelte` (+ test) -- tooltip mở bằng focus/hover, đóng bằng Escape.
- [x] `src/routes/settings/SettingsGeneral.svelte` (+ test) -- select ngôn ngữ (Theo hệ thống/vi/en/ja) và theme (Theo hệ thống/Sáng/Tối).
- [x] `src/routes/settings/SettingsGemini.svelte` (+ test) -- key, danh sách key + xoá, ba ô model + datalist + Tải danh sách, cảnh báo nhẹ, lỗi inline, banner tại chỗ.
- [x] `src/routes/Settings.svelte`, `src/lib/errors.ts`, i18n -- nối nhóm, helper model banner; test phủ từng dòng Matrix.

**Acceptance Criteria:**
- Given `/settings/general` hoặc `/settings/gemini`, when mở, then nav trái 220px và mỗi trường có (?) tooltip tới được bằng Tab cùng helper text luôn hiện.
- Given lỗi category `model` ở bất kỳ luồng nào, when dùng `modelErrorBanner`, then banner warning có lối tắt tới Settings → Gemini và model không đổi.
- Given Consent bị từ chối, when vào Settings, then vẫn chỉ nhóm About như hiện tại (không regression).

## Implementation Notes

- Model defaults moved to `src-tauri/src/core/model_defaults.rs`; `gemini::params` re-exports them so settings and gemini both depend only on `core`.
- `Settings` gained `transcribeModel`/`liveModel`/`memoModel`; load falls back per key (missing, corrupt or blank), save rejects blank with `format`. Bindings regenerated.
- `keysStore` gained `deleteKey`, `lastCheckedAt` and per-kind session-only `loadModelList`; `settingsStore.setModel` follows the optimistic/rollback pattern.
- New `HelpTip`/`SettingsRow`, `SettingsGeneral`/`SettingsGemini`; diagnostics/about keep their placeholders. `modelErrorBanner` lives in `src/lib/errors.ts`.
- Main-session fix: model input committed on both `change` and `blur` (double save); now commits on `change` only.

## Design Notes

Ô model dùng `<input list>` + `<datalist>` để vừa chọn từ danh sách vừa cho gõ tự do; giá trị hiện tại luôn nằm trong ô nên "select vẫn dùng được" khi đang tải. Lưu khi `change`/blur, sau khi validate inline. Danh sách đã tải chỉ giữ trong phiên chạy.

## Verification

**Commands:**
- `cargo test --locked --manifest-path src-tauri/Cargo.toml` -- pass.
- `npm run bindings && git diff --exit-code src/lib/bindings.ts` -- sau khi commit bản sinh, không drift.
- `npm run check && npm test && npm run check:i18n && npm run check:ui && npm run check:deps && npm run build && npm run check:build-assets` -- pass.
