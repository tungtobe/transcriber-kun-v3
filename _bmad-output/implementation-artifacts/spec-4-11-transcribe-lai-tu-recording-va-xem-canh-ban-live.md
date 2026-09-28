---
title: 'Story 4.11: Transcribe lại từ Recording và xem cạnh bản live'
type: 'feature'
created: '2026-09-29'
status: 'done'
baseline_commit: 'ccb51e40cb0c44f67b595c7dd39081b8d1b3f973'
route: 'full'
route_source: 'auto'
review: 'none'
review_source: 'pinned'
lenses_ran: []
review_loop_iteration: 0
context: []
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** Live transcript can have disconnected gaps, and the app cannot transcribe the durable Recording into a second transcript or compare them. **Approach:** Run a cancellable full Recording job in the existing serial queue, atomically publish one current `retranscribe` variant, and expose both variants in Session detail.

## Boundaries & Constraints

**Always:** Use the persisted Recording or Proxy for complete/recovered live sessions, current Consent and Gemini gateway, and the existing chunk/progress/cancel/error behavior. Keep `primary`, notes, tags and memo untouched. A repeated run builds a candidate and swaps only `retranscribe` on success; failure/cancel leaves both existing variants intact. In side-by-side view, Export/Copy/Memo source is explicit, persists while navigating the detail view, and defaults to live. Seek does not change it. Offset is visible and applies to both columns. Retry of a retranscribe gap targets its own transcript ID. A `disconnected` live gap stays visible with a retranscribe hint even if technical status is complete.

**Never:** Overwrite live segments with the job result, allow multiple current `retranscribe` rows per session, enqueue an additional job for the same session while one is active, or expose an unfinished candidate as current.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|---------------|---------------------------|----------------|
| First run | Complete/recovered live session with Recording | Queue full transcribe; publish retranscribe beside live | Missing source/Consent/key returns categorized error |
| Repeat run | Existing retranscribe | Atomic replacement after all chunks finish | Cancel/error retains old retranscribe and live |
| Partial run | Failed chunk | Commit partial retranscribe with gap | Live remains intact; retry addresses retranscribe ID |
| Compare | Both variants | Equal columns, 56 px time grids, click either segment seeks player | Export/Copy/Memo uses chosen source, independent of seek |
| Disconnected | Live gap only | Gap and retranscribe hint visible despite complete status | No retry against live gap |

</frozen-after-approval>

## Code Map

- `src-tauri/src/transcribe/{job,registry}.rs` -- serial JobRegistry, progress/cancel and candidate pipeline; add a distinct full Recording job path, reuse chunk engine.
- `src-tauri/src/ipc/mod.rs` -- typed command/gates and registration; `transcribe_rerun` rejects live primary, so add separate retranscribe command.
- `src-tauri/src/db/repo/transcripts.rs`, `src-tauri/src/library/store.rs` -- variant schema and atomic transcript swap; detail currently loads primary only. Preserve live `primary` and session data.
- `src/routes/Session.svelte`, `src/routes/session/{SessionHeader,SegmentList,PartialBanner}.svelte` -- detail menus, segment rendering/seek, gap hint and export/copy controls.
- `src/lib/stores/{jobs,memo}.svelte.ts`, `src/lib/bindings.ts`, `src/i18n/{vi,en,ja}.json` -- command wiring, memo source, generated types and labels.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/transcribe/{job,registry}.rs`, `src-tauri/src/ipc/mod.rs` -- queue a full Recording retranscribe with progress/cancel and session busy gate.
- [x] `src-tauri/src/db/repo/transcripts.rs`, `src-tauri/src/library/store.rs` -- commit/swap exactly one current retranscribe and load both variants; test cancel, repeat, partial and preservation.
- [x] `src/routes/Session.svelte`, `src/routes/session/*`, stores, bindings and i18n -- add menu action, segmented/side-by-side presentation and explicit source for export/copy/memo; test selection, seek and gaps.

**Acceptance Criteria:**
- Given a persisted live Recording, when Transcribe lại is selected, then a cancellable Job uses the serial queue and publishes `retranscribe` without changing `primary`, notes, tags or memo.
- Given two variants, when switching modes or clicking a segment, then both columns and the chosen operation source remain correct and the player seeks to the clicked time.
- Given a disconnected live gap, when viewing detail, then the gap and rerun suggestion remain visible irrespective of `complete` status.

## Implementation Notes

The existing serial registry now runs a full Recording job and commits its candidate through a conditional transaction. A migration enforces one current retranscribe variant per session. Session detail loads both variants; Export, Copy and Memo share an explicit source choice independent of seek. The retranscribe retry path continues to use the existing gap pipeline.

## Spec Change Log

## Review Triage Log

## Design Notes

The candidate should remain private until the existing commit seam swaps by expected transcript ID. A new retranscribe variant needs a conditional insert/swap transaction. Keep the selected operation source as UI state shared by export/copy/memo, independent of the visible pane.

## Verification

**Commands:**
- `cargo test --locked --manifest-path src-tauri/Cargo.toml` -- Rust job/DB/IPC suites pass.
- `npm run bindings && npm run check && npm test` -- generated bindings, Svelte check and UI suites pass.
- `npm run check:i18n && npm run check:ui && git diff --check` -- locales, tokens and whitespace pass.

**Results:** 582 Rust unit tests and 4 media corpus tests; 639 frontend tests; bindings, Svelte check, i18n, UI token, and diff checks passed. Review was pinned to none. No work was deferred.
