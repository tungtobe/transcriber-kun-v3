---
title: 'Story 6.2: Preview and apply recommended settings'
type: 'feature'
created: '2026-09-30'
status: 'done'
baseline_commit: '1f321c8095e09f89ccdd3b330a886d1e35f1b3b3'
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

**Problem:** Users cannot see or safely adopt current recommended model, chunk, and memo template settings. Applying a remote document without a fresh preview could overwrite local choices.

**Approach:** Add an on-demand, inline Settings group with Rust preview/apply commands. Use Story 6.1 to fetch a fresh signed document, show only allow-listed differences, and atomically apply exactly the displayed valid changes after stale-state checks.

## Boundaries & Constraints

**Always:** Fetch only on button click. Preview binds payload digest and current settings/template revision. Revalidate before apply; if local state or payload is stale, return a new diff without writing. Persist changes in one transaction. Only allow-list transcribe/live/memo models, chunk minutes, and recommended templates. Reuse local validators. Unknown fields are ignored; future schema version is an explicit error.

**Never:** Display or write Gemini keys, consent, unrelated settings, or user-created templates; auto-apply, auto-fetch, or create a new route. Cancel must invalidate the preview token.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Valid signed recommendation | Click fetch with changed allow-listed values | Inline `label · old → proposed` rows; unchanged omitted | No writes yet |
| Invalid remote or invalid values | Bad signature/network, wrong type/range, future version | Inline category banner, no setting changes | Redacted typed error |
| Apply and cancel | Current preview token; cancel or apply | Apply exactly displayed valid rows transactionally, show count; cancel writes nothing | Old token unusable |
| Stale local state | Settings or template edited after preview | Recomputed diff, no automatic apply | Ask user to apply new preview |
| Recommended template | External ID matches untouched recommendation or edited local copy | Add/update recommendation only; user edits show conflict | Never touch user template |

</frozen-after-approval>

## Code Map

- `src-tauri/src/settings/mod.rs` -- typed Settings, validators, save/load; own recommendation preview/apply service.
- `src-tauri/src/ipc/mod.rs` -- Tauri commands and Specta bindings registration.
- `src-tauri/src/db/repo/settings.rs`, `src-tauri/src/db/repo/memo_templates.rs`, `src-tauri/src/db/migrations/mod.rs` -- transaction and template storage patterns.
- `src-tauri/src/memo/templates.rs` -- prompt validator and local template invariants.
- `src-tauri/src/remote/mod.rs` -- signed fresh fetch; no direct HTTP in settings.
- `src/routes/Settings.svelte`, `src/routes/settings/`, `src/i18n/{vi,en,ja}.json`, `src/lib/bindings.ts` -- inline group, commands, labels.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/settings/` and `src-tauri/src/db/` -- implement allow-listed parser, preview token/revision, conflict detection, atomic apply and invalidation -- protect local state.
- [x] `src-tauri/src/ipc/mod.rs` and `src/lib/bindings.ts` -- expose `settings_recommended_preview`, `settings_recommended_apply`, and cancel flow -- typed UI boundary.
- [x] `src/routes/Settings.svelte` and `src/routes/settings/SettingsRecommended.svelte` -- add inline preview/apply/cancel/error UI -- intentional user control.
- [x] `src/i18n/{vi,en,ja}.json` -- translate labels, values, banners and confirmation -- consistent locales.
- [x] Rust and frontend tests adjacent to changed modules -- cover matrix including keys/consent exclusion, stale state, template conflict and no auto-fetch -- prevent unsafe apply.

**Acceptance Criteria:**
- Given a valid signed recommendation, when fetched explicitly, then only changed allow-listed values appear inline and nothing is saved.
- Given a displayed preview, when applied against unchanged state, then exactly those valid changes commit together and a count is shown.
- Given a stale or canceled preview, when apply is attempted, then no writes occur; stale state yields a refreshed diff.
- Given unknown fields, key or consent data, when parsed, then they are ignored and never rendered or persisted.
- Given user-modified recommended template, when previewed, then a conflict appears and apply does not overwrite it.

## Implementation Notes

- The exact signed payload digest and a digest of local settings/template state bind a one-use process-local preview token.
- Accepted recommendation baselines live in migration 10 and distinguish later remote changes from local template edits.
- Focused settings tests passed 53, remote tests 10, Settings UI tests 9; Rust/Svelte checks and i18n checks passed.

## Spec Change Log

## Review Triage Log

## Design Notes

The signed document format must be documented for Story 6.5's publishing tool. An in-process preview token can expire at app restart; persistent settings/template state remains authoritative.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml settings::` -- backend cases pass.
- `npm test -- --run src/routes/Settings.groups.test.ts` -- group/UI cases pass.
- `npm run check` -- Svelte types pass.
- `cargo check --manifest-path src-tauri/Cargo.toml` -- backend compiles.
