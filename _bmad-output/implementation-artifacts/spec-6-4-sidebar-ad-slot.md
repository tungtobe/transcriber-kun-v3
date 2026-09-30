---
title: 'Story 6.4: Sidebar house ad slot'
type: 'feature'
created: '2026-09-30'
status: 'done'
baseline_commit: '85fd88ea62581c7b48fbf32a198a8024a3d7c944'
route: 'full'
route_source: 'auto'
review: 'none'
review_source: 'pinned'
lenses_ran: []
review_loop_iteration: 0
context: ['_bmad-output/implementation-artifacts/epic-6-context.md']
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** House ads have a backend pipeline but no compliant, accessible surface. The app must never show an ad during Live, including setup, recording and save states.

**Approach:** Add a compact 236px sidebar slot on Home, Transcript detail and Settings. Render only typed safe content from Story 6.3, include Sponsored/report/why controls, acknowledge an impression only after actual visibility, and structurally omit the slot on every Live state.

## Boundaries & Constraints

**Always:** Keep slot at bottom of 260px sidebar, fixed/stable footprint during load/offline fallback, surface border/radius per design tokens. Support 300×100 and 320×50 images scaled to width, light/dark, vi/en/ja, keyboard and aria labels, AA contrast. Click/report use Rust IPC external opener only. Disable/premium result removes slot without leftover spacer. Route gate must prevent `ads_next` and impression calls on Live, not just hide DOM.

**Never:** Inject remote HTML, expose raw URL to frontend, autoplay sound, cover content, use interstitials, show ads on `/live` or onboarding/consent-restricted states.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Eligible route | Home, session detail or Settings | Compact sponsored slot with image/text, report and explanation | Fallback same dimensions during load/offline |
| Live route | Setup, recording or saving on `/live` | No slot in DOM and no ad selection/impression call | No reserved gap |
| Disabled | `ads_next` returns `null` | Slot removed | No reserved gap |
| Visible creative | Enters viewport; user clicks or reports | One visible impression, external browser via IPC | Duplicate/remount does not recount |
| Accessibility | vi/en/ja, light/dark, keyboard | Readable label and controls, focus and aria text | No clipped buttons |

</frozen-after-approval>

## Code Map

- `src/components/AppShell.svelte` -- 260px sidebar, spacer/footer, reactive router location; mount slot only for eligible routes.
- `src/components/AppShell.test.ts` -- router-aware shell DOM tests for route changes and Live states.
- `src-tauri/src/ads/mod.rs`, `src/lib/bindings.ts` -- typed `adsNext`, `adsImpression`, `adsClick`, `adsReport` operations and safe view.
- `src/i18n/{vi,en,ja}.json`, `src/i18n/index.svelte.ts` -- localized controls and UI locale.
- `src/styles/tokens.css` -- color/spacing tokens; reuse for accessibility.

## Tasks & Acceptance

**Execution:**
- [x] `src/components/AppShell.svelte` -- structurally gate and position slot on eligible routes -- never fetch/display on Live.
- [x] `src/components/AdSlot.svelte` -- render safe card, visible impression, click/report/why controls and fallback/loading behavior -- compliant UX.
- [x] `src/i18n/{vi,en,ja}.json` -- translate report/why/status/aria text -- accessible localization.
- [x] `src/components/AppShell.test.ts` and `src/components/AdSlot.test.ts` -- cover matrix including route transitions and absence of ad commands on Live -- regression protection.

**Acceptance Criteria:**
- Given Home/Transcript detail/Settings, when loaded, then a 236px ad slot appears at the sidebar bottom with Sponsored, report, and why controls.
- Given `/live` in any state, when rendered or navigated to, then no ad slot exists and no ad selection or impression command runs.
- Given a visible creative, when confirmed, then impression IPC runs once; click/report go through typed opener commands.
- Given disabled ads, when backend returns no creative, then slot and gap disappear.
- Given offline/loading/fallback, when slot is eligible, then its footprint does not jump and content stays accessible in all locales/themes.

## Implementation Notes

- Route eligibility is decided by AppShell before mounting AdSlot; Live never calls ads IPC.
- The embedded fallback uses a fixed image placeholder because it has no bundled bitmap; it keeps the same slot footprint.
- Targeted UI tests passed 28, with Svelte and i18n checks passing.

## Spec Change Log

## Review Triage Log

## Verification

**Commands:**
- `npm test -- --run src/components/AppShell.test.ts src/components/AdSlot.test.ts` -- route and slot cases pass.
- `npm run check` -- Svelte types pass.
- `npm run check:i18n` -- locale keys align.
