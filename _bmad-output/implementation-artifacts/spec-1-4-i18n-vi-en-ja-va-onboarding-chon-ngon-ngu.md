---
title: 'Story 1.4 — i18n vi/en/ja và Onboarding chọn ngôn ngữ'
type: 'feature'
created: '2026-09-22'
status: 'done'
baseline_commit: '8e25c872751312b0cfa2c850c14687eca141f1a6'
route: 'full'
route_source: 'auto'
review: 'thorough'
review_source: 'auto'
lenses_ran:
  - 'blind-hunter'
  - 'edge-case-hunter'
  - 'verification-gap'
  - 'intent-alignment'
review_loop_iteration: 0
context:
  - '{project-root}/_bmad-output/implementation-artifacts/epic-1-context.md'
  - '{project-root}/_bmad-output/implementation-artifacts/spec-1-3-token-thi-giac-theme-sang-toi-va-app-shell.md'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** App hiện hardcode tiếng Việt; người dùng vi/en/ja không được chào bằng ngôn ngữ hệ thống, không thể đổi tức thì, và catalog chưa có cổng parity nên các màn sau dễ thiếu bản dịch.

**Approach:** Dựng module i18n typed dùng ba JSON bundle cục bộ, mở rộng settings typed để lưu preference ngôn ngữ, dịch toàn bộ UI v3 hiện có và thay placeholder Onboarding bằng bước chọn ngôn ngữ accessible; thêm startup guard và CI chặn lệch key.

## Boundaries & Constraints

**Always:**
- Locale UI hỗ trợ đúng `vi | en | ja`; preference persisted là `system | vi | en | ja`. `system` resolve từ locale chính dạng `vi-*`/`en-*`/`ja-*`, ngoài tập này fallback `en`; card của locale đã resolve mang badge “theo hệ thống”.
- Đổi radio áp UI và `<html lang>` đồng bộ ngay, không reload; save qua domain settings store, optimistic rồi rollback + typed error nếu ghi thất bại. UI locale độc lập với ngôn ngữ transcribe/target dịch.
- Dùng module tự viết với static JSON imports, flat key `<screen>.<block>.<label>`, English fallback và interpolation text an toàn; không `innerHTML`, runtime fetch hay backend i18n. ADR ghi quyết định này và lý do không chọn i18next dù API hiện hành hỗ trợ bundled resources.
- Ba bundle có cùng exact key set; CI fail khi thiếu/thừa key hoặc key sai convention. Migrate/rewrite chỉ key v2 mà UI v3 hiện có dùng; loại `setup`, `whisper`, `copilot`, `updates` và không dump key tương lai chưa có consumer.
- Onboarding card 600px, stepper `Ngôn ngữ · Dữ liệu · API key`, ba radio card bàn phím được, nút/badge dùng `min-width`; shell/Home/Settings/Onboarding hiện có đều đi qua `t()`.
- Settings Rust vẫn là durable source; cache frontend chỉ là first-paint hint. Binding được regenerate, không sửa tay. Guard đưa trạng thái chưa hoàn tất về `/onboarding`, đã hoàn tất từ `/onboarding` về `/home`.

**Never:** Không triển khai nội dung/decision Consent, API key, Gemini request, route epic sau, ICU/rich HTML translation, hay thay đổi theme/router geometry. Consent precedence chỉ có seam rõ để Story 1.5 mở rộng; không tạo fake consent state để giả hoàn tất AC tương lai.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|---------------|---------------------------|----------------|
| First launch | preference `system`; OS `ja-JP` | `/onboarding`, Japanese selected, badge system, toàn màn Japanese | OS ngoài vi/en/ja → English |
| Runtime switch | user chọn `vi`/`en`/`ja` | mọi copy/title/aria đổi cùng render; persist full settings | save lỗi rollback locale persisted, shell vẫn render và báo typed error |
| Backend sync | load/event mang preference mới | locale, cache và document language đồng bộ | malformed/missing key backend → `system` riêng field, giữ theme |
| Route guard | onboarding incomplete/complete | route tương ứng `/onboarding`/`/home` | không loop, unknown route vẫn theo router policy |
| Catalog drift | một bundle thiếu/thừa key | CI fail với locale + key cụ thể | không fallback để che lỗi parity |

</frozen-after-approval>

## Code Map

- `src/i18n/{vi,en,ja}.json`, `src/i18n/index.svelte.ts` -- thay seed một-key bằng catalog dùng thật; module reactive detect/cache/apply/translate, không DOM `innerHTML`.
- `/Users/nguyenthanhtung/code/ai_lab/transcriber-kun/app/src/i18n/*.json`, `app/src/js/i18n.js` -- nguồn v2 chỉ để chọn lại wording/locale rules; không copy nhóm obsolete hay runtime fetch/DOM mutation.
- `src-tauri/src/settings/mod.rs`, `src-tauri/src/ipc/mod.rs` -- thêm `UiLanguage` và `onboardingCompleted` theo per-key fallback/transaction/event hiện có; giữ theme semantics.
- `src/lib/stores/settings.svelte.ts` -- mở rộng snapshot save/load/event, cache và rollback theo generation; component không import binding.
- `src/routes/Onboarding.svelte` -- dựng card/radiogroup thật trên placeholder, giữ `aria-current`; Continue là placeholder accessible cho Story 1.5.
- `src/components/AppShell.svelte`, `src/routes/{Home,Settings}.svelte`, `src/App.svelte`, `src/main.ts` -- thay literal hiện có bằng reactive `t()`, bootstrap locale và áp startup route sau settings load.
- `src/lib/router.ts` -- pure startup-guard helper, giữ đúng ba route hiện tại và seam cho Consent Story 1.5.
- `scripts/check-i18n.mjs`, tests, `.github/workflows/test.yml` -- parity/convention gate theo pattern các check hiện có.
- `docs/adr/0003-frontend-i18n.md`, `src/lib/bindings.ts` -- ghi quyết định custom module; binding chỉ sinh bằng `npm run bindings`.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/settings/mod.rs`, `src-tauri/src/ipc/mod.rs`, generated binding -- persist language/onboarding fields, per-field corrupt fallback và event snapshot.
- [x] `src/i18n/*`, `docs/adr/0003-frontend-i18n.md` -- implement typed reactive runtime/catalog và record library decision.
- [x] `src/lib/stores/settings.svelte.ts` + tests -- integrate language bootstrap/save/event/rollback without regressing theme races/lifecycle.
- [x] `src/routes/Onboarding.svelte` + tests -- render 600px three-language radio-card step with system badge and immediate switch.
- [x] existing shell/routes + render tests -- localize every delivered UI string/title/aria while preserving layout and theme behavior.
- [x] router/main tests -- enforce incomplete/completed startup destinations without adding future route or Consent behavior.
- [x] `scripts/check-i18n.mjs`, package/CI tests -- fail exact parity, invalid keys and forbidden legacy groups.

**Acceptance Criteria:**
- Given any supported/unsupported system locale, when app first renders, then language selection, text and `<html lang>` resolve deterministically before the screen stabilizes.
- Given a locale selection and successful typed save, when app restarts or receives `settingsChanged`, then the same preference is restored and all delivered screens remain translated.
- Given production verification, when catalogs/build/bindings are checked, then key parity, offline bundles, type checks, frontend/backend tests and generated-binding drift all pass.

## Implementation Notes

- Implemented a synchronous runes-based i18n store with bundled flat JSON catalogs, system-locale normalization, first-render cache, safe text interpolation and reactive `<html lang>` updates.
- Extended the Rust-owned settings snapshot with `uiLanguage` and `onboardingCompleted`; saves remain atomic and frontend optimistic changes rollback the complete last persisted snapshot.
- Startup routing is a pure completion guard invoked after typed settings load. Consent priority is deliberately left as the documented Story 1.5 seam.
- The catalog contains only strings consumed by the currently delivered v3 shell/routes; legacy forbidden groups are rejected by the new CI gate rather than copied forward.

## Spec Change Log

## Review Triage Log

| Finding | Verdict | Evidence | Route |
|---|---|---|---|
| BH-1 startup mounts Home before redirect | medium | `main.ts` mounts before awaiting `settingsGet`; an incomplete first launch can paint Home before `replace('/onboarding')`. | patch |
| BH-2 save during pending load can overwrite durable completion | medium | Production currently mounts interactive controls while load is pending, and setters build a full snapshot from bootstrap defaults. | patch |
| BH-3 concurrent full-snapshot saves can commit out of order | medium | `saveGeneration` suppresses stale UI responses but both Tauri writes run concurrently; the older write may commit last. | patch |
| BH-4 backend event can be overwritten by pending load | false | The app has one window, and its only event producer is this store's own save, which increments `saveGeneration` and invalidates the pending load before the event can arrive. | reject |
| BH-5 rejected IPC promise leaves no typed error | medium | The `catch` branch rolls back but sets `error = null`, so the shell cannot explain a transport-level persistence failure. | patch |
| BH-6 `quiet utility` remains hardcoded | low | The visible shell tagline bypasses `t()` despite the frozen requirement to translate the delivered shell. | patch |
| BH-7 key regex accepts more than three segments | low | `{2,}` accepts four-plus segments although the frozen convention is exactly `<screen>.<block>.<label>`. | patch |
| BH-8 placeholder parity is unchecked | low | Placeholder sets currently match and the approved CI contract is exact key parity; a new placeholder gate adds nontrivial policy for an unlikely future typo. | reject |
| BH-9 stepper label lacks a semantic role | low | `aria-label` is attached to a generic `div`; adding list semantics makes the progress label/current step reliably exposed. | patch |
| BH-10 onboarding card is not vertically centered | false | The requirement fixes a 600px card and horizontal centering; it does not require vertical centering, and `place-items: start center` satisfies the stated horizontal layout. | reject |
| BH-11 startup wiring is not exercised | medium | Existing tests cover only the pure guard and `App`, so removing the `main.ts` callback leaves them green. | patch |
| BH-12 rollback test does not preserve completed onboarding | low | The implementation rolls back a full snapshot, but the test fixture uses `false` and would not detect accidental loss of a persisted `true`. | patch |
| BH-13 backend corrupt-completion fallback is untested | low | Rust has per-field code but no corrupt `onboardingCompleted` or legacy-row regression test. | patch |
| BH-14 Home/Settings lack en/ja render coverage | medium | Their changed keys are rendered only under Vietnamese, so wrong supported-locale copy can pass all current tests. | patch |
| EH-1 newer failed save after older pending save can desync UI/DB | medium | This is the concrete two-save ordering failure in BH-3: the UI can roll back past an older successful backend commit. | patch |
| EH-2 overlapping loads can resolve out of order | false | Production has one startup load; retry appears only after it settles and disappears synchronously when a retry begins, so two loads are not reachable. | reject |
| EH-3 settings event during load can be overwritten | false | Same claim as BH-4; the single-window event source is coupled to a generation-incrementing local save. | reject |
| EH-4 shell tagline is untranslated | low | Same literal and user-visible outcome as BH-6. | patch |
| IA-1 no native restart end-to-end test | low | The absence is real, but Rust round-trip plus frontend load tests cover the contract; adding native E2E infrastructure is disproportionate for this residual verification risk. | reject |
| IA-2 only Vietnamese exercises full Shell/Home/Settings | medium | Same supported-locale verification gap as BH-14; isolated Japanese tests do not render those consumers. | patch |
| IA-3 system language uses WebView rather than native locale | false | The frozen design and ADR explicitly resolve the WebView's primary locale and keep OS detection out of Rust. | reject |
| IA-4 bundle parity does not catch the hardcoded tagline | low | The concrete divergence is the same visible literal as BH-6 and is directly correctable. | patch |
| IA-5 `system` is not a fourth radio choice | false | The frozen requirement explicitly calls for three locale cards; `system` is an initial persisted sentinel represented by a badge, not a fourth choice. | reject |
| VG-1 entrypoint route enforcement lacks a wiring test | medium | Pre-verified by the lens: deleting the `main.ts` `.then` leaves every current test green. | patch |
| VG-2 en/ja Shell/Home/Settings render paths are unverified | medium | Pre-verified by the lens: corrupting Japanese Home copy to Vietnamese leaves current tests green. | patch |
| VG-3 invalid key convention has no negative test | low | Pre-verified by the lens: removing `KEY_PATTERN` validation leaves the current script tests green. | patch |
| VG-4 completion fallback lacks legacy/corrupt tests | low | Pre-verified by the lens: missing or corrupt completion could change routing without any current test failing. | patch |

## Design Notes

The custom module reuses v2’s proven locale normalization/cache/English fallback but uses Svelte runes and static imports. `system` is a preference sentinel, not a fourth translation bundle. The startup guard models only completion state owned by this story; Story 1.5 must insert versioned Consent priority before Gemini-capable navigation.

## Verification

**Commands:**
- `npm run bindings && npm run check:deps && npm run check:i18n && npm run check:ui && npm run check && npm run build && npm run check:build-assets && npm test` -- expected: generated types, catalogs, UI, build and frontend tests pass.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml` -- expected: per-key defaults/round trips/events and backend continuity pass.
- `git diff --exit-code src/lib/bindings.ts` -- expected: binding is regenerated and clean after verification.

**Manual checks (if no CLI):**
- In `tauri dev`, launch under vi/en/ja/unsupported locale; switch each radio with keyboard, verify immediate full-screen translation, restart persistence, 600px card at 1024×680 and no clipped Japanese/Vietnamese labels.
