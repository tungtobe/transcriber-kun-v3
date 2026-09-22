---
title: 'Story 1.3 — Token thị giác, theme sáng/tối và app shell'
type: 'feature'
created: '2026-09-22'
status: 'done'
baseline_commit: '9eccd56b6b8dadeccaeb23a23a244cda789e0bd9'
route: 'full'
route_source: 'auto'
review: 'thorough'
review_source: 'auto'
lenses_ran: ['blind-hunter', 'edge-case-hunter', 'verification-gap', 'intent-alignment']
review_loop_iteration: 0
context:
  - '{project-root}/_bmad-output/implementation-artifacts/epic-1-context.md'
  - '{project-root}/_bmad-output/planning-artifacts/ux-designs/ux-transcriber_kun-2026-09-18/DESIGN.md'
  - '{project-root}/_bmad-output/planning-artifacts/ux-designs/ux-transcriber_kun-2026-09-18/EXPERIENCE.md'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** Frontend hiện chỉ là màn version, chưa có ngôn ngữ thị giác chung, app shell, routing hay theme persisted; các story UI sau sẽ phân kỳ và không đáp ứng accessibility nếu tiếp tục trên nền này.

**Approach:** Dựng design-token CSS light/dark, font và icon bundle cục bộ, store theme bọc typed IPC, router SPA và app shell tối thiểu cho ba route hiện có; thêm lint/test tự động khóa tương phản, shadow và hành vi theme.

## Boundaries & Constraints

**Always:**
- CSS variable phải phản ánh đúng token trong `DESIGN.md`; đo WCAG AA cho từng theme, text thường ≥ 4.5:1. Trạng thái không chỉ dựa vào màu.
- IBM Plex Sans 400/500/600/700 và Plex Mono được bundle; fallback Nhật dùng font hệ thống; mono bật `tabular-nums`; không request font/script mạng.
- Flat UI: chỉ segmented-selected và lớp nổi menu/dialog/toast/popover được có shadow; focus ring `2px solid accent`, offset 2px; `prefers-reduced-motion` tắt pulse/caret và panel animation.
- Sidebar 260px, header 64px, cửa sổ tối thiểu 1024×680; vùng main nhận phần dư và panel phụ đóng trước ở 1024–1279px.
- Theme `system | light | dark` áp dụng ngay khi load, save, event backend hoặc OS scheme đổi; component chỉ dùng domain store, không gọi binding/invoke trực tiếp.
- Chỉ đăng ký `/onboarding`, `/home`, `/settings/:group`; deep link và Back/Forward hoạt động trong WebView.

**Decisions đã duyệt:** Chủ sản phẩm đã duyệt ngày 2026-09-18 toàn bộ token dark bổ sung ghi trong `reconcile-01-master-design-system.md`, gồm giá trị `mark-dark` cuối `#7A6019`; đây là bản giá trị được code, không còn assumption mở.

**Never:** Không UI component library, emoji icon, gradient, màu ngữ nghĩa thứ năm, global OS shortcut, route/màn của epic sau, ad thật, Home/Settings/Onboarding chức năng đầy đủ, hay sửa tay `src/lib/bindings.ts`.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|---------------|----------------------------|----------------|
| Khởi động | setting `light`/`dark` | Theme đúng được đặt trước khi màn ổn định | Không flash theme đối lập |
| Theo hệ thống | setting `system`; OS đổi scheme | UI đổi tức thì theo `matchMedia` | Listener được dọn khi teardown |
| Lưu theme | người dùng chọn theme | UI đổi ngay, gọi `settingsSave`, đồng bộ event | Save lỗi: rollback theme persisted, app shell vẫn render và có trạng thái lỗi typed |
| IPC unavailable | `settingsGet` throw/trả lỗi | Shell vẫn render bằng `system` | Không màn trắng, store giữ lỗi typed/null |
| Deep link | mở/reload route hợp lệ hoặc Back/Forward | Đúng placeholder screen và active nav | Route lạ redirect `/home` |
| Reduced motion | OS bật giảm chuyển động | Pulse/caret/panel animation bị tắt | — |

</frozen-after-approval>

## Code Map

- `src/App.svelte`, `src/main.ts` -- thay placeholder bằng router + shell; cấu hình history mode trước `mount`.
- `src/lib/stores/app.svelte.ts` -- pattern `$state` và typed-result cần tái dùng; không gọi IPC từ component.
- `src/lib/stores/settings.svelte.ts` -- mới: load/save/listen `settingsChanged`, theo dõi `matchMedia`, áp `data-theme`.
- `src/lib/bindings.ts` -- generated source của `Settings`, `Theme`, commands/events; chỉ import.
- `src/styles/{fonts,tokens,global}.css` -- mới: font local, light/dark variables, reset/a11y/motion primitives.
- `src/components/AppShell.svelte`, `src/components/icons.ts` -- shell/sidebar/header và Lucide icon registry.
- `src/routes/{Onboarding,Home,Settings}.svelte`, `src/lib/router.ts` -- placeholder route thật trong phạm vi Epic 1 và typed navigation.
- `src-tauri/tauri.conf.json` -- min/default size đã đúng; giữ nguyên.
- `scripts/` -- thêm checks contrast và forbidden shadow; pattern script/test hiện có từ `check-forbidden-deps`.
- `docs/adr/` -- ADR chốt `@keenmate/svelte-spa-router` 5.3.0 history mode cho Svelte 5/Tauri.

## Tasks & Acceptance

**Execution:**
- [x] `package.json`, lockfile, font assets -- pin router, Lucide, Fontsource và DOM test tooling; không runtime network.
- [x] `src/styles/*`, `scripts/check-ui-tokens.mjs` + tests -- triển khai token/font/motion và CI-check contrast + shadow.
- [x] `src/lib/stores/settings.svelte.ts` + test -- typed IPC, optimistic theme, rollback/error, backend event và OS scheme lifecycle.
- [x] `src/lib/router.ts`, `src/routes/*`, `docs/adr/0002-frontend-router.md` -- ba route, param group, redirect lạ, internal deep link/history.
- [x] `src/components/AppShell.svelte`, `src/App.svelte` + render tests -- shell accessible luôn render trong loading/error; active nav, header và job placeholder.
- [x] `src/main.ts`, `src/lib/keymap.ts` -- bootstrap theme/router và giữ shortcut trong-app tập trung.

**Acceptance Criteria:**
- Given CI, when chạy UI checks, then từng cặp foreground/background nội dung ở light và dark đạt AA và shadow ngoài allow-list làm build fail.
- Given bundle production, when quét output/network references, then IBM Plex và Lucide nằm cục bộ, không có Google Fonts hay remote asset.
- Given viewport 1024×680 và 1280×800, when render shell, then sidebar/header đúng 260/64px, nội dung không tràn và keyboard focus rõ ở cả hai theme.
- Given typed binding không đổi, when hoàn tất build/test, then `git diff --exit-code src/lib/bindings.ts` pass.

## Implementation Notes

- Frontend dependencies are exact-pinned: `@keenmate/svelte-spa-router@5.3.0`, `@lucide/svelte@1.47.0`, IBM Plex Fontsource packages, `jsdom`, and `@testing-library/svelte`.
- Theme bootstrap uses a local first-paint cache only as a visual hint; the Rust settings command remains the persistence source of truth. Save failures restore the last persisted theme and expose the typed error to the shell.
- The UI guard measures the approved semantic foreground/background pairs in both token scopes and rejects any shadow outside the segmented/floating allow-list. Reduced-motion selectors explicitly disable dot, caret, and panel motion.
- Unknown history routes replace to `/home`; router integration tests cover a parameterized deep link and Back/Forward transitions. The Live item remains an accessible disabled placeholder because `/live` is outside this story's registered route set.
- Review hardening adds a same-origin pre-style theme bootstrap, generation guards for overlapping settings work/listener teardown, exact per-theme contrast matrices, and a post-build local-asset/network-reference gate.

## Spec Change Log

## Review Triage Log

| # | Lens | Finding | Verdict | Route and evidence |
|---:|---|---|---|---|
| 1 | blind-hunter | A pending `load()` can overwrite a newer optimistic `setTheme()`. | medium | patch — `load()` has no generation check after `settingsGet()`, so the demonstrated interleaving leaves the saved theme different from the rendered theme. |
| 2 | blind-hunter | The dark-specific contrast rows are never evaluated. | medium | patch — both scopes iterate `CONTRAST_PAIRS.slice(0, 13)`, making rows 14–26 unreachable. |
| 3 | blind-hunter | Cached dark can flash the default light theme before application bootstrap. | high | patch — production CSS declares light by default and the cache is not read until the body module executes; an early same-origin bootstrap is required. |
| 4 | blind-hunter | A failed settings load leaves the old explicit theme in local storage. | medium | patch — both error paths render `system` without replacing the cache, so the next launch can apply the known-stale explicit hint. |
| 5 | blind-hunter | The shell error banner misdescribes save failures as a system-theme load fallback. | medium | patch — typed save errors reach the same banner after rollback, but its copy always claims the system theme is active. |
| 6 | blind-hunter | An unknown settings group falls back to “Chung” without changing its URL. | low | reject — `/settings/:group` is intentionally a parameterized placeholder and no finite group contract is stated; the state is reachable mainly through a hand-edited URL and normalization would add routing policy not settled by the intent. |
| 7 | blind-hunter | `settingsStore.destroy()` is not connected to application teardown. | low | patch — the store owns two listeners and exposes direct cleanup, while `App` already has one mount cleanup hook that can call it without a new surface. |
| 8 | blind-hunter | The document language is English although the delivered UI is Vietnamese. | medium | patch — `index.html` declares `lang="en"` while all shell/route labels are Vietnamese, which changes screen-reader pronunciation. |
| 9 | blind-hunter | Space normalizes to a literal blank rather than the documented `Space` combo. | medium | patch — `KeyboardEvent.key === ' '` follows the one-character branch, so a future registered `Space` shortcut can never match. |
| 10 | blind-hunter | A semicolon-less `box-shadow` can bypass the source elevation scan. | medium | patch — the regular expression requires `;`, although a declaration immediately before `}` is valid CSS. |
| 11 | blind-hunter | CI does not scan the production bundle for remote assets. | medium | patch — build success alone accepts remote `url()`/`@import` references and therefore does not enforce the explicit offline-assets criterion. |
| 12 | blind-hunter | The active onboarding step lacks progress semantics. | medium | patch — visual `step-current` styling has no `aria-current="step"`, so the non-color state is absent for assistive technology. |
| 13 | blind-hunter | Shell tests omit themes, geometry, focus, reduced motion, and overflow. | medium | patch — the acceptance surfaces are real, while the only shell render assertion covers landmarks and labels; targeted source/render guards can close the demonstrated gaps. |
| 14 | edge-case-hunter | Dark-only contrast pairs are skipped. | medium | patch — independently confirmed by the same unreachable second half of `CONTRAST_PAIRS`. |
| 15 | edge-case-hunter | A stale settings load can revert a newer save. | medium | patch — independently confirmed; `saveGeneration` is checked only inside `setTheme()`, not by `load()`. |
| 16 | edge-case-hunter | Destroy/bootstrap can race an unresolved backend-listener registration. | low | patch — after teardown, the old promise can resolve last and replace the new unlisten handle; a lifecycle generation directly closes the demonstrated race. |
| 17 | verification-gap | Dark-theme contrast rows have no executable regression coverage. | medium | patch — pre-verified: current tests mutate only a light token and both theme checks use the light slice. |
| 18 | verification-gap | Accent text on `accent-soft` is absent from the matrix. | medium | patch — pre-verified: active navigation uses this pair and making both tokens equal remains green; the current values pass AA but regressions are unguarded. |
| 19 | verification-gap | Component-style traversal is not executed by a focused test. | low | patch — pre-verified: in-memory shadow tests do not prove recursive `.svelte`/`.css` discovery or its wiring into `runUiChecks()`. |
| 20 | verification-gap | Reduced-motion coverage is only a permissive substring check. | low | patch — pre-verified: `reduced` contains the asserted `reduce` substring, so an invalid media value can pass. |
| 21 | verification-gap | Local-font/no-remote-asset behavior is not checked in the production bundle. | medium | patch — pre-verified: no test or CI command examines emitted font files or network references. |
| 22 | verification-gap | The mounted `App` composition is never rendered in a test. | medium | patch — pre-verified: isolated shell/router tests remain green if `App.svelte` drops either composition edge. |
| 23 | verification-gap | Typed settings-load errors are not verified through the shell. | medium | patch — pre-verified: the load error branch and visible retry banner have no combined assertion. |
| 24 | verification-gap | The rendered theme selector is presence-tested but never operated. | medium | patch — pre-verified: removing `onchange={changeTheme}` leaves direct store tests and the shell presence test green. |
| 25 | verification-gap | Forward navigation checks only the URL. | low | patch — pre-verified: stale Settings content could remain mounted after the URL returns to `/home`. |
| 26 | verification-gap | Required sidebar/header geometry has no automated guard. | medium | patch — pre-verified: changing the width/height tokens currently leaves all tests green; add deterministic token/style assertions for the supported viewport contract. |
| 27 | intent-alignment | Token/design equality and rendered contrast live at a broader surface than current checks. | medium | patch — the current values match `DESIGN.md`, but no guard detects value drift and the dark matrix bug leaves part of the intended contrast surface unchecked. |
| 28 | intent-alignment | Actual shell accessibility states exceed the current landmark-only test surface. | medium | patch — keyboard focus, active-step semantics, theme interaction, and geometry are user-visible states not covered by the isolated shell assertion. |
| 29 | intent-alignment | Production offline assets exceed the current source/dependency test surface. | medium | patch — emitted output is the contract surface and currently has no network-reference/font-emission assertion. |
| 30 | intent-alignment | Real typed Tauri IPC is not exercised by frontend tests. | false | reject — this is a difference in test layer, not a demonstrated defect: the frontend imports generated typed commands/events, backend command tests pass, and binding immutability plus the manual Tauri check cover the integration seam. |
| 31 | intent-alignment | First paint and responsive geometry exceed the current source-test surface. | medium | patch — the pre-module flash is demonstrated and geometry constants can regress undetected; no auxiliary panel exists in this minimal shell, so panel collapse is presently not applicable. |
| 32 | intent-alignment | Router tests use jsdom rather than a Tauri WebView. | false | reject — no WebView-specific failure is identified; history mode is configured before mount, URL/content transitions are exercised, and the spec retains an explicit WebView manual check. |
| 33 | intent-alignment | Reduced-motion behavior exceeds the current substring-test surface. | low | patch — an invalid media feature can satisfy the substring check, while an exact media-rule assertion is a direct correction. |
| 34 | intent-alignment | Shell and router are tested separately rather than as the root `App`. | medium | patch — removing the production composition edge from `App.svelte` is not detected by the isolated tests. |

## Design Notes

`@keenmate/svelte-spa-router` 5.3.0 được chọn vì hỗ trợ Svelte 5, history/hash mode, params và navigation API; cấu hình history trước mount. App shell dùng semantic `aside/nav/header/main`; placeholder là màn thật tối thiểu của route hiện tại, không giả lập tính năng tương lai.

## Verification

**Commands:**
- `npm run check:deps && npm run check:ui && npm run check && npm run build && npm run check:build-assets && npm test` -- expected: toàn bộ lint/type/build/unit/render/a11y/offline-asset checks pass.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml` -- expected: backend continuity pass.
- `git diff --exit-code src/lib/bindings.ts` -- expected: generated binding không bị sửa.

**Manual checks (if no CLI):**
- Chạy `npm run tauri dev`; kiểm tra direct navigation/Back/Forward cho ba route, theme system/light/dark đổi tức thì, resize 1024×680, tab/focus và reduced motion.
