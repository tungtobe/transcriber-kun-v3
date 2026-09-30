---
title: 'Story 6.3: House ad creative pipeline'
type: 'feature'
created: '2026-09-30'
status: 'done'
baseline_commit: 'fb48126cbf0e1362fffeb79eccae15cee8803032'
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

**Problem:** There is no safe way to choose and display house ad creatives while respecting locale, time, rotation, frequency, offline behavior, and a server or local disable flag.

**Approach:** Build a Rust `ads/` pipeline consuming only Story 6.1 verified content and images. Expose typed next/impression/click/report operations, persist local caps/counters, and provide an embedded fallback when ads are enabled but no remote creative qualifies.

## Boundaries & Constraints

**Always:** Signed manifest controls `ads_enabled` and creative digest; local `is_premium` takes precedence. Filter locale and inclusive start/end at selection time, validate 300×100 or 320×50 image, finite positive weights, HTTPS click and safe report URL. Weighted selection excludes creatives shown within 10 minutes. Record an impression only after explicit visible-display confirmation, persist cap across restart, and count click/impression locally only. Verified `ads_enabled=false` must remain effective offline via verified cache.

**Never:** Fetch or count ads on `/live`; report metrics to a server; use remote IDs as file paths; let ads errors alter transcript, Job or Recording state. Disabled flags must suppress fallback and slot.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Eligible signed creatives | Matching locale/time, positive weights | Weighted eligible creative; verified image | Skip invalid candidate |
| Capped or invalid candidates | All expired, invalid weight, capped or offline | Embedded creative, stable dimensions | No repeated fallback impression on remount |
| Disabled | Signed `ads_enabled=false` or local premium | No creative at all | Disable beats fallback |
| Visible creative | Display confirmation, then click/report | Persist one impression per remote ID per 10 min; local counters; safe URL | Duplicate display ignored |
| Remote failure | Cached signed manifest or no cache | Use verified cache before fallback | Typed failure, no core state change |

</frozen-after-approval>

## Code Map

- `src-tauri/src/ads/mod.rs` -- existing stub; implement selection service and focused tests.
- `src-tauri/src/remote/mod.rs` -- signed ads fetch, digest-bound image fetch, safe URL helpers; only remote access path.
- `src-tauri/src/db/migrations/mod.rs`, `src-tauri/src/db/repo/` -- local persistent cap/counter storage.
- `src-tauri/src/ipc/mod.rs`, `src-tauri/src/ipc/boot.rs`, `src/lib/bindings.ts` -- typed frontend operations and app state.
- `src-tauri/src/settings/mod.rs` -- local premium flag (without UI in this story).
- `_bmad-output/planning-artifacts/epics.md` -- detailed Story 6.3 acceptance and fallback behavior.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/ads/` -- parse signed manifest, filter candidates, rotate by weight, enforce disable/cap/fallback, and validate links/images -- trusted creative choice.
- [x] `src-tauri/src/db/` -- persist last displayed timestamps and local counters -- cap survives restart without telemetry.
- [x] `src-tauri/src/ipc/` and `src/lib/bindings.ts` -- expose typed next/display/click/report API for Story 6.4 -- UI never receives unsafe links.
- [x] `src-tauri/src/settings/mod.rs` -- add durable local premium flag without exposing a UI -- local ad suppression.
- [x] Rust tests in changed modules -- cover every matrix row with fake time/transport/randomness -- deterministic safety checks.

**Acceptance Criteria:**
- Given signed, valid creatives, when selected, then locale/time/weight/cap rules choose only an eligible creative with verified image.
- Given no eligible creative and ads enabled, when selected, then embedded fallback is available without repeated count from remount.
- Given server disable or premium, when selected, then no creative or fallback is returned.
- Given a visible remote creative, when reported, then one persistent local impression is recorded and duplicate reports within 10 minutes do not count.
- Given remote failure, when selected, then verified cache or embedded fallback is used without affecting core state.

## Implementation Notes

- The signed Creative parser accepts the Epic fields `locale[]`, `start`, `end`, `url`, `sponsor` and retains aliases for earlier fixtures.
- The embedded fallback is localized text in a fixed 300×100 view; Story 6.4 should preserve the same slot dimensions when no image is present.
- Ads tests passed 12/12, remote tests 12/12, migration tests 26/26; Rust and Svelte checks passed.

## Spec Change Log

## Review Triage Log

## Design Notes

Story 6.4 owns route gating and actual display confirmation; this service must not count an impression merely because `ads_next` returned a candidate. Keep fallback content bundled and safe for all three UI locales.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml ads::` -- selection/cap/disable/fallback cases pass.
- `cargo check --manifest-path src-tauri/Cargo.toml` -- crate compiles.
- `npm run check` -- bindings type check passes.
