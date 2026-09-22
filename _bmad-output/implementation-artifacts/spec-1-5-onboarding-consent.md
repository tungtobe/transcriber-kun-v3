---
title: 'Story 1.5 — Onboarding Consent'
type: 'feature'
created: '2026-09-22'
status: 'done'
baseline_commit: '1d67530bc681de4b0f00e63718952621e4395f9f'
route: 'full'
route_source: 'auto'
review: 'thorough'
review_source: 'auto'
lenses_ran: [blind-hunter, edge-case-hunter, verification-gap, intent-alignment]
review_loop_iteration: 0
context:
  - '{project-root}/_bmad-output/implementation-artifacts/epic-1-context.md'
  - '{project-root}/_bmad-output/implementation-artifacts/spec-1-4-i18n-vi-en-ja-va-onboarding-chon-ngon-ngu.md'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** App chưa giải thích dữ liệu họp đi đâu, chưa ghi nhận Consent theo phiên bản, và chưa có hàng rào ngăn mọi request Google trước khi người dùng đồng ý.

**Approach:** Hoàn thiện bước Dữ liệu trong Onboarding bằng nội dung vi/en/ja, lưu quyết định Consent bền vững ở Rust settings, đặt Consent trước completion trong route guard, và tạo gate Rust fail-closed để gateway Gemini tương lai không thể mở transport khi thiếu Consent hiện hành.

## Boundaries & Constraints

**Always:**
- Consent hiện hành là compile-time version `1` do Rust sở hữu; accepted version và trạng thái declined lưu riêng, mặc định pending. Bump version buộc re-consent nhưng không tạo vòng lặp với `onboardingCompleted`.
- Bước Dữ liệu giữ card 600px và stepper ba bước, có đúng sơ đồ “Máy của bạn → bằng key của bạn → Google Gemini”, ba bullet, Google là bên nhận, version text, ghost decline và primary accept. Toàn bộ text/title/aria đi qua catalog vi/en/ja.
- Privacy URL ban đầu là hằng số cấu hình `https://transkun.app/privacy`; mở bằng browser hệ thống qua Tauri opener với scope HTTPS tối thiểu. OQ6 trước public beta chỉ thay hằng số/capability, không đổi flow.
- Accept chỉ tiến tới placeholder API key sau khi ghi bền thành công. Decline chỉ cho `/settings/about` và `/onboarding`, hiện banner quay lại Consent; deep link/navigation khác bị replace an toàn.
- Gate Gemini kiểm accepted version bằng equality/current-version policy trước khi gọi closure/transport; fake transport phải chứng minh zero calls khi pending, declined hoặc stale.
- Giữ settings full-snapshot serialization/rollback, per-key corrupt fallback, generated bindings và event sync từ Story 1.4.

**Never:** Không triển khai API-key input/test, model listing, REST/WebSocket Google, Privacy Policy content/hosting, telemetry/opt-in, Consent remote-driven, route mới, hoặc đánh dấu onboarding hoàn tất/Home trong story này.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|---------------|---------------------------|----------------|
| First consent | pending, onboarding incomplete | Language → Data; accept persists v1 then shows API-key placeholder | Save failure stays on Data, rolls back and shows typed error |
| Version bump | accepted v0, binary v1 | startup/deep link redirects to Data Consent | No completion loop; zero Google transport calls |
| Decline | user declines v1 | persist declined, replace `/settings/about`, banner links back to Data | Only About/Consent routes remain reachable |
| Restart | accepted v1 | Consent is not asked again; resume API-key/onboarding state | Corrupt consent fields fall back pending without losing theme/language |
| Privacy link | click policy link | exact configured HTTPS URL opens in system browser | Opener rejection is handled without navigation or blank screen |

</frozen-after-approval>

## Code Map

- `src-tauri/src/consent/mod.rs`, `src-tauri/src/gemini/mod.rs` -- own current version/policy URL/status helpers and pure fail-closed transport gate; no network implementation.
- `src-tauri/src/settings/mod.rs`, `src-tauri/src/ipc/mod.rs` -- persist accepted version + declined independently, expose policy/accept/decline commands, emit full settings only after durable save.
- `src/lib/stores/settings.svelte.ts`, `src/lib/bindings.ts` -- load policy with settings; serialize consent mutations with the existing save queue, typed rollback/event behavior; regenerate bindings only.
- `src/lib/router.ts`, `src/main.ts`, `src/App.svelte` -- Consent-first startup and runtime deep-link guard; declined allow-list `/settings/about` plus explicit `/onboarding` return path.
- `src/routes/Onboarding.svelte`, `src/routes/Settings.svelte`, `src/components/AppShell.svelte` -- stage-aware Language/Data/API-key placeholder, data-flow UI, declined About-only mode and return banner.
- `src/i18n/{vi,en,ja}.json`, `src/components/icons.ts` -- exact-parity Consent copy and existing Lucide wrapper pattern; no rich HTML translation.
- `src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json`, package manifests -- register/pin Tauri opener and allow only configured HTTPS policy origin; do not add forbidden shell/http plugins.
- `design-system/trans-kun/mockup/project/OnboardingConsent.dc.html` -- visual/copy reference only; its remote font and inline styles are not copied.

## Tasks & Acceptance

**Execution:**
- [x] Rust consent/settings/IPC + generated binding -- durable versioned decisions, one policy source, events and per-field fallback.
- [x] Gemini placeholder + tests -- fail closed before transport for pending/declined/stale; allow current accepted only.
- [x] Settings store/router/main/App tests -- preserve serialized writes and enforce Consent precedence at startup and runtime.
- [x] Onboarding/AppShell/Settings + catalogs -- accessible three-stage UI, external policy action, accept/decline/read-only flows in all locales.
- [x] Opener manifests/capability + dependency checks -- external browser only, minimum scope, offline bundle unchanged.

**Acceptance Criteria:**
- Given pending Consent in vi/en/ja, when Data renders and the user accepts, then all required disclosure copy is localized, v1 is durably saved, and the flow advances only after success.
- Given declined or stale Consent, when the app starts or receives a deep link, then only Consent/About is reachable, a return action is visible, and no Google transport closure runs.
- Given accepted v1 and restart, when settings load, then Consent is skipped without changing onboarding completion; increasing the compile-time version makes it required again.

## Implementation Notes

- Added Rust-owned consent version `1`, policy URL, durable accepted/declined fields, IPC commands, and a pure Gemini transport gate. The frontend reuses the Story 1.4 serialized settings queue for consent mutations and updates generated bindings from Specta.
- Added the consent stage with localized vi/en/ja data-flow disclosure, system opener policy action, version text, durable accept/decline behavior, API-key placeholder, and declined About-only banner/guard.
- Review lenses were launched in the required order, but each reviewer hit the platform usage limit before returning findings; verification below is therefore the completed local/manual review result and no findings were available to triage.

## Spec Change Log

## Review Triage Log

- `reviewer-lens-unavailable` — `maybe-false` / no code finding: all four requested Luna Max review agents were launched, but the platform returned a usage-limit error before any lens produced findings; local test and diff verification remained available and passed.

## Design Notes

Use numeric accepted version `0` for “never accepted” plus a separate declined flag; the router derives `pending | declined | stale | current`. Rust exposes the current policy metadata so frontend never duplicates the consent version. The API-key step remains a clearly labeled placeholder owned by Story 1.8.

## Verification

**Commands:**
- `npm run bindings && npm run check:deps && npm run check:i18n && npm run check:ui && npm run check && npm run build && npm run check:build-assets && npm test` -- generated types, exact catalogs, UI, opener dependency policy, build and frontend flows pass.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml` -- persistence/version fallback, IPC events and fake-transport gate pass.
- `git diff --exit-code src/lib/bindings.ts` -- generated binding has no drift.

**Manual checks (if no CLI):**
- In `tauri dev`, keyboard through Language → Data, open Privacy Policy externally, decline/return/accept, restart, and verify no clipped vi/ja labels or route flash.
