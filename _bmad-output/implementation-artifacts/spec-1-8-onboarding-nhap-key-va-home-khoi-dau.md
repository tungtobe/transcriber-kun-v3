---
title: 'Story 1.8 — Onboarding nhập key và Home khởi đầu khi chưa có key'
type: 'feature'
created: '2026-09-23'
status: 'done'
baseline_commit: '8e383474c131087b521d6435d0186373344dfe32'
route: 'full'
route_source: 'auto'
review: 'none'
review_source: 'pinned'
lenses_ran: []
review_loop_iteration: 0
context:
  - '{project-root}/_bmad-output/implementation-artifacts/epic-1-context.md'
  - '{project-root}/_bmad-output/implementation-artifacts/spec-1-7-cong-gemini-duy-nhat-va-liet-ke-model.md'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** Bước "API key" của Onboarding chỉ là placeholder, không có lối vào Home; Home chưa báo trạng thái thiếu key và chưa có component banner/nút vô hiệu dùng chung, nên người dùng có thể kẹt hoặc không hiểu vì sao tính năng Gemini bị tắt.

**Approach:** Thay placeholder bằng form key (password + hiện/ẩn, "Kiểm tra key", "Bỏ qua, nhập sau", "Tiếp tục") dùng các command IPC đã có; thêm keys store phía frontend, component `Banner` và `DisabledHint` dùng chung, banner warning thiếu key trên Home và bảng thông điệp theo 8 category.

## Boundaries & Constraints

**Always:** Chỉ dùng command đã có (`keysSet`, `keysList`, `keysTest`, `modelsList`) qua store bọc binding; không thêm command Rust. "Kiểm tra key" = `keysSet(input)` → `keysTest` từng id → nếu ≥1 hợp lệ thì `modelsList('transcribe')` để lấy N. Kết quả/lỗi render trong `role=status` ngay dưới ô key, không rời màn. Lỗi hiện tiêu đề category + một câu hướng dẫn từ i18n (không hiện `detailRedacted`, key, URL). Bỏ qua hoặc Tiếp tục → `setOnboardingCompleted(true)` rồi `/home`. Trạng thái "có key hợp lệ" chỉ suy từ dữ liệu trong phiên chạy: không có key → thiếu; mọi key đã test trong phiên đều bị từ chối → thiếu; có key chưa test → coi là có (không gọi mạng nền để kiểm). Mọi chuỗi mới có đủ vi/en/ja theo `check:i18n`. Banner: ba phần, biến thể danger/warning/info, stack tối đa 2 theo danger > warning > info. Nút vô hiệu: `aria-disabled`, focus được bằng Tab, `opacity .45`, tooltip lý do + lối tắt, không ẩn.

**Never:** Không gọi Google trước Consent hiện hành, không poll/timer, không log hay lưu key ngoài kho khoá, không sửa tay `bindings.ts`, không làm UI Settings → Gemini (Story 1.9; link chỉ trỏ `/settings/gemini`), không thêm card kéo file/Live mới (Epic 2/4), không tự đổi model.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|---------------|---------------------------|----------------|
| Key hợp lệ | 1+ key đúng định dạng, mạng ok | status "Key hợp lệ, N model khả dụng"; nút Tiếp tục bật | — |
| Nhiều key, một phần sai | 2 key, 1 bị 401 | Báo hợp lệ + N, thêm "1 key bị từ chối" | Không chặn Tiếp tục |
| Định dạng sai / rỗng | chuỗi không phải key | Chặn tại chỗ, không gọi IPC khi rỗng | Lỗi từ `keysSet` hiện theo category |
| Key sai / quota / mạng / CA | mọi key fail | Tiêu đề category + hướng dẫn trong status, vẫn ở bước key | auth, quota, network (gồm TLS), blocked… qua bảng category |
| Bỏ qua | bấm "Bỏ qua, nhập sau" | Onboarding hoàn tất → `/home` với banner warning thiếu key | — |
| Từ chối Consent | consentDeclined | Không tới bước key; chỉ Settings/About (hành vi 1.5 giữ nguyên) | Không request |
| Home có key | keysList không rỗng, chưa test fail | Không banner thiếu key | — |

</frozen-after-approval>

## Code Map

- `src/routes/Onboarding.svelte` -- `Stage` 'apiKey' (≈l.147) là placeholder; thay bằng form key. Giữ stepper/consent logic.
- `src/lib/stores/settings.svelte.ts` -- tái dùng `setOnboardingCompleted` (≈l.338), mẫu store bọc binding + xử lý `AppError`.
- `src/lib/stores/keys.svelte.ts` (mới) -- bọc `keysList/keysSet/keysTest/modelsList`; state `keys`, `testState`, `hasUsableKey`; `load()` gọi ở Home.
- `src/lib/router.ts` -- `enforceStartupRoute` giữ nguyên; đảm bảo sau `onboardingCompleted=true` vào `/home` không bị đẩy lại.
- `src/lib/errors.ts` (mới) -- map `Category` → i18n key tiêu đề/hướng dẫn (`error.<category>.title|hint`), dùng chung app.
- `src/components/Banner.svelte`, `BannerStack.svelte` (mới) -- tokens `--color-{danger,warning,info}*` trong `src/styles/tokens.css`; icon qua `src/components/icons.ts`.
- `src/components/DisabledHint.svelte` (mới) -- chuẩn hoá mẫu `aria-disabled`+`opacity .45` đang lặp ở `Home.svelte:147-150`, `AppShell.svelte:263-266`; áp dụng lại cho hai nút Home.
- `src/routes/Home.svelte` -- giữ header/empty note; thêm BannerStack với banner warning thiếu key, action "Nhập key" → `/settings/gemini`.
- `src/i18n/{vi,en,ja}.json` -- key 3 đoạn lowerCamel; thêm `onboarding.apiKey.*`, `home.banner.*`, `error.*.*`.
- Tests: mẫu `src/routes/Onboarding.test.ts`, `Home.test.ts` (vitest + @testing-library/svelte, jsdom, `vi.mock` store).

## Tasks & Acceptance

**Execution:**
- [x] `src/lib/errors.ts`, `src/i18n/*.json` -- bảng category → tiêu đề + hướng dẫn cho đủ 8 category.
- [x] `src/lib/stores/keys.svelte.ts` (+ test) -- luồng check theo Always, tính `hasUsableKey`, xử lý lỗi thành `AppError`.
- [x] `src/components/{Banner,BannerStack,DisabledHint}.svelte` (+ test) -- ba biến thể, sắp xếp + cắt 2, tooltip focus bàn phím.
- [x] `src/routes/Onboarding.svelte` (+ test) -- form key, show/hide, check, skip, continue; status `role=status` `aria-live`.
- [x] `src/routes/Home.svelte` (+ test) -- banner thiếu key, dùng DisabledHint.
- [x] Test luồng: bỏ qua key → Home; consent declined → không vào bước key; key sai → ở lại sửa được; không crash/dialog lặp.

**Acceptance Criteria:**
- Given bước API key, when dán key và "Kiểm tra key", then status đọc được bằng screen reader báo "Key hợp lệ, N model khả dụng" hoặc lỗi category + một câu hướng dẫn.
- Given Home lần đầu không có key hợp lệ, when hiển thị, then có header, dòng "Chưa có phiên nào…" và banner warning có link "Nhập key" tới Settings → Gemini.
- Given 3 banner danger/warning/info cùng lúc, when render, then chỉ hiện danger và warning theo thứ tự đó.
- Given nút vô hiệu, when Tab tới, then nhận focus, hiện tooltip lý do + lối tắt và không bị kích hoạt.

## Design Notes

"Có key hợp lệ" không được lưu bền vì validity phụ thuộc thời điểm; suy theo phiên chạy tránh gọi mạng nền (ràng buộc epic). `keysSet` là replace-all nên lần kiểm tra lại sau khi sửa thay toàn bộ danh sách — đúng mong đợi ở Onboarding.

## Verification

**Commands:**
- `npm run check && npm test` -- pass.
- `npm run check:i18n && npm run check:ui && npm run check:deps && npm run build && npm run check:build-assets` -- pass.
- `npm run bindings && git diff --exit-code src/lib/bindings.ts` -- không drift.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml` -- pass (không đổi Rust).

**Manual checks (if no CLI):**
- `npm run tauri dev`: Onboarding → bỏ qua key → Home có banner, không màn trắng.
