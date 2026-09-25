---
title: 'Epic 2 P1 frontend fixes — stale session responses, row registry, queued count, search normalization, localized export gaps'
type: 'bugfix'
created: '2026-09-26'
status: 'done'
baseline_commit: 'eb17462412e83333550cf94f9d5e22895aea7aa5'
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

**Problem:** The epic 2 review flagged P1 frontend defects:
- A slow detail, relink or export response can overwrite a session the user already navigated away from.
- `SegmentList` keeps stale row keys, so click-to-seek and auto-scroll can target the wrong row.
- The Home job card counts the displayed queued job as "another" waiting file.
- Search misses Vietnamese text that differs only in Unicode normalization form.
- Exported TXT gap notes are always English, while Copy is localized.

**Approach:** Make local fixes in the Svelte components and stores, thread the localized gap labels through the export command, and add a regression test for each fix. The test gaps from the 2.7 review are included.

## Boundaries & Constraints

**Always:**
- `Session.svelte`: every async result tied to a session is applied only if the session is still the one displayed. Use a monotonically increasing load token (the same idea as `jobsStore`'s generation) that is captured before the `await` and compared after it. This applies to `load`, `loadDetail`, `handleRelinkRequest`, `handleExportTranscript` and `handleCopyTranscript`. `load()` also resets `exporting` and `relinking`, so buttons on the new session are never stuck disabled. A stale result is dropped silently.
- `SegmentList.svelte` `registerRow`: track the current index in a variable that is reassigned in `update`, so `update` and `destroy` delete the key the node currently holds.
- `JobCard.svelte` `queuedTranscribeCount` excludes the job shown on the card (`j.jobId !== job?.jobId`).
- `transcript-search.ts`: normalize both the query and the segment text with `normalize('NFC')` before case-folding, while keeping match offsets valid against the original segment text that the highlight slices.
- Export: `library_transcript_export` takes `gapLabels: { chunkFailed, disconnected, unknown }` (strings). The TXT formatter uses them instead of the hard-coded English. The frontend passes the same `session.export.gap*` i18n strings that Copy uses. SRT and JSON output are unchanged; JSON keeps raw `gapReason`. Regenerate `bindings.ts`.
- Tests to add:
  - `SegmentList`: keyboard scroll keys (`ArrowDown`, `PageDown`) stop auto-scroll while playing.
  - `Player`: seeking past a known `duration` clamps to `duration`.
  - `Session`: relink outcomes `cancelled`, `liveUnsupported` and error.
  - The `JobProgress` log integration test asserts a line unique to the log panel.

**Never:**
- No backend change beyond the export `gapLabels` parameter.
- No change to search performance characteristics beyond one `normalize` per segment per query (keep the existing 700-segment test under budget).
- No P2/P3 items.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Slow detail | load A (slow), navigate to B, A resolves | view shows B | stale A dropped |
| Relink then navigate | relink A in flight, go to B, relink resolves | B unaffected, no relink message | — |
| Export then navigate | export A dialog open, go to B | B's export buttons enabled; A's toast not shown on B | — |
| Row shifts twice | row key 5→6→7 | `rowEls` has only key 7 for that node | — |
| Single queued job | only 1 queued transcribe job shown | no "N file đang chờ" text | — |
| Queued + another | shown queued job + 1 more queued | "1 file đang chờ" | — |
| NFD text | segment "Việt" in NFD, query NFC "việt" | 1 match, highlight covers the word | — |
| vi export TXT | gap chunk_failed, UI vi | note = vi `session.export.gapChunkFailed` | — |

</frozen-after-approval>

## Code Map

- `src/routes/Session.svelte` -- `view` state (L28), `handleExportTranscript` (L137-165), `loadDetail` (L169), `load` (L199), `handleRerun` (L264), `handleRelinkRequest` (L291). Toolbar buttons are at ≈L383-390. The search-reset `$effect` is at ≈L53-66.
- `src/routes/session/SegmentList.svelte` -- `registerRow` (L150-161), `SCROLL_KEYS` / `handleListKeydown` (≈L137).
- `src/routes/session/Player.svelte` -- `seek` (≈L87-92) and `handleLoadedMetadata`.
- `src/components/JobCard.svelte` -- `queuedTranscribeCount` (L55-57). Tests are in `JobCard.test.ts` and `Home.test.ts`.
- `src/lib/transcript-search.ts` -- `normalizeQuery` (≈L17) and `normalizeTextWithOffsets` (L24-61). The mapping must stay correct when NFC changes length: normalize per code point / grapheme, or build the offset map from the NFC-normalized text back to the original indices.
- `src/lib/transcript-export.ts` -- `GapCopyLabels` type (reuse it for the export labels).
- `src-tauri/src/library/export.rs` -- `gap_note` (L94), `format_txt`, `render_transcript`. Golden tests are in the same file.
- `src-tauri/src/ipc/mod.rs` -- `library_transcript_export` (≈L1063). Add a `GapLabels` specta type.
- Tests: `src/routes/Session.test.ts`, `src/routes/session/{SegmentList,Player,JobProgress}.test.ts`, `src/lib/transcript-search.test.ts`.

## Tasks & Acceptance

**Execution:**
- [x] `src/routes/Session.svelte` + test -- load token guard for all async handlers; reset `exporting`/`relinking` on load; relink outcome tests
- [x] `src/routes/session/SegmentList.svelte` + test -- fix `registerRow`; add keyboard scroll-stop tests
- [x] `src/routes/session/Player.svelte` test -- duration clamp test (fix the code if it fails)
- [x] `src/components/JobCard.svelte` + test -- exclude the displayed job from the count
- [x] `src/lib/transcript-search.ts` + test -- NFC normalization with correct offsets
- [x] `src-tauri/src/library/export.rs`, `src-tauri/src/ipc/mod.rs`, `src/routes/Session.svelte`, `src/lib/bindings.ts` + tests -- localized TXT gap notes
- [x] `src/routes/Session.test.ts` -- JobProgress log assertion unique to the log panel

**Acceptance Criteria:**
- Given all changes, when the verification commands run, then all pass, and `bindings.ts` has no drift after `export_bindings`.

## Implementation Notes

## Spec Change Log

## Review Triage Log

## Verification

**Commands:**
- `cd src-tauri && cargo test --locked` -- expected: all pass
- `cd src-tauri && cargo clippy --locked --all-targets -- -D warnings` -- expected: clean
- `cd src-tauri && cargo fmt --check` -- expected: clean
- `npx vitest run` -- expected: all pass
- `npm run check` -- expected: 0 errors
