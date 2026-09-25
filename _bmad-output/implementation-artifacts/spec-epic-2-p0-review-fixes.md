---
title: 'Epic 2 P0 review fixes — crash, hang, silent data loss, consent, close, export ownership'
type: 'bugfix'
created: '2026-09-25'
status: 'done'
baseline_commit: 'a25595dd601e18712a30c21994ebe131f2715a04'
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

**Problem:** The epic 2 code review found seven P0 defects: a settings value that aborts the process, an unbounded resampler loop, three paths that silently lose transcript text/audio, a consent snapshot that keeps sending audio after revocation, a close handler that exits mid-Job when the registry is unreachable, and an export command that reads any transcript by bare ID.

**Approach:** Fix all seven in one pass with the smallest local change each, plus a regression test per fix. The user explicitly asked for one pass (multi-goal accepted).

## Boundaries & Constraints

**Always:**
- `chunkMinutes` is an integer in `1..=60`. Rust `settings::save` rejects out-of-range values with `Code::Format`. `settings::load` falls back to the default (5) for out-of-range rows, just as it already does for `< 1`. The frontend blocks the same range inline, and the store guard matches it.
- `Chunker::new` never pre-allocates more than the default chunk (300 s × 16 kHz) of capacity, whatever `max_duration_seconds` is.
- `MonoResampler::finish` flush loop is bounded; if the bound is hit without reaching `expected`, return `format_error`, never loop forever.
- No transcribed text is dropped silently: a text segment fully clamped away by `push_forward` has its text appended to the previous Text segment; if there is none, it is kept as a zero-length segment at the cursor. Log only a count (content-free logging).
- A decode `spawn_blocking` task that panics (JoinError) turns the Job into `JobOutcome::Error`, never a commit — in both `decode_and_transcribe` and `decode_and_transcribe_ranges`, unless an outcome is already set.
- Rerun: the last range that reaches the end of the recording (end_ms ≥ total_duration_ms − 1000) is open-ended — it consumes decoded samples to the true end of the Proxy. Its seconds-range for `splice_rerun` uses `f64::INFINITY` as the end.
- `splice_rerun` drops an old segment only when it lies within a range (1 ms tolerance). An old segment that partially overlaps a range is trimmed to the part outside the range and kept.
- Consent is re-read from settings before every chunk send (file transcribe and rerun). If it is no longer current, the Job stops with a `Code::Blocked` error and does not commit.
- Close handler: when `jobs.snapshot()` returns `Err`, treat it as busy (emit `CloseRequested`). `app_close_confirm` treats `snapshot()` `Err` as "nothing left to wait for" (break), so the user can always exit.
- `library_transcript_export` takes `session_id` + `transcript_id`. `get_export_data` returns `None` unless the transcript's `session_id` matches. The frontend passes the open session's ID. Regenerate `bindings.ts`.

**Never:**
- No change to `ConsentSnapshot`/`StartParams.consent` shape or the IPC start gate; the per-chunk re-check is added on top.
- No new dependency, migration, or i18n key beyond updating the existing chunk-minutes helper/error text to mention the maximum.
- No re-check of consent inside the gateway retry loop (per-chunk bound is accepted; documented).
- Don't touch P1/P2 findings.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Save chunk 61 | `chunk_minutes = 61` | Rejected, nothing written | `Code::Format` |
| Load corrupt chunk | DB row `5000` | `chunk_minutes = 5` | warn log |
| Save chunk 60 | `60` | Saved | — |
| Resampler never converges | resampler yields no frames | Returns error after bound | `format_error` |
| Clamped text | segment `[3,4]` "b" after cursor 5 with previous Text "a" | previous text "a b" | count logged |
| Decode panic | blocking task panics after some chunks | Job `Error`, no commit | JoinError → Error |
| Rerun all, proxy longer | total 2000 ms, proxy 2500 ms | all 2500 ms of samples routed to range 0 | — |
| Rounded neighbor | old Text `[0,10.0004]`, range `(10.0,20.0)` | Text kept, trimmed to `[0,10.0]` | — |
| Consent revoked mid-job | declined after chunk 1 | chunk 2 never sent; Job Error Blocked | fatal |
| Registry unreachable on close | `snapshot()` Err | `CloseRequested` emitted | warn |
| Export wrong session | transcript of session A, session_id B | "no longer exists" error | `Code::Storage` |

</frozen-after-approval>

## Code Map

- `src-tauri/src/settings/mod.rs` -- `require_min_chunk_minutes` (≈L315) → add `MAX_CHUNK_MINUTES = 60` range check. `load` chunk match (≈L246) guards `parsed >= 1` → add `<= MAX`. Tests at ≈L920-1010 show the pattern.
- `src-tauri/src/media/chunk.rs` -- `Chunker::new` (L83-103) and `flush` (L146) use `Vec::with_capacity(max_samples)` → cap capacity at `DEFAULT_CHUNK_SECONDS * OUTPUT_SAMPLE_RATE`.
- `src/lib/stores/settings.svelte.ts` -- `isPositiveInteger` (L67) is used by the `chunkMinutes` normalizer (L124) and `setChunkMinutes` (L456) → add an `isChunkMinutes` guard `1..=60`; don't change `isPositiveInteger` semantics elsewhere.
- `src/routes/settings/SettingsChunking.svelte` -- `parseIntegerAtLeast(value, 1)` (L26, L33) → also reject > 60. Add a `max` attribute to the input.
- `src/i18n/{vi,en,ja}.json` -- `settings.chunking.chunkMinutesHelper` / `chunkMinutesError` → mention max 60.
- `src-tauri/src/media/resample.rs` -- `finish` loop L111-114 → bounded loop (e.g. `MAX_FLUSH_BLOCKS = 64`), then `format_error`.
- `src-tauri/src/transcribe/merge.rs` -- `push_forward` L90-101 → keep text as described. `splice_rerun` L132-160 → contain-vs-trim logic. Tests L280-357.
- `src-tauri/src/transcribe/rerun.rs` -- `resolve_ranges` L91-119 → mark the tail range open-ended (`end_sample = u64::MAX`, keep `end_ms`). `decode_ranges_and_chunk` L127-189 needs no change once `end_sample` is `u64::MAX` (check the `(range.end_sample - position) as usize` cast — use saturating/min). `wav_fixture`/`range_from_ms` test helpers at L196+.
- `src-tauri/src/transcribe/registry.rs` -- `decode_and_transcribe` (L1032, join at ≈L1180) and `decode_and_transcribe_ranges` (L1303) → handle JoinError. Add a consent re-check before `transcriber.transcribe(...)` in both loops via a `spawn_blocking(settings::load)` helper (registry already imports `crate::settings`). Thread `db: &Arc<Db>` from `run_job_inner` (L894) / `run_rerun_job_inner`. The `ranges_sec` mapping at ≈L1262 → `f64::INFINITY` for open-ended. Test fakes: `FakeTranscriber`, `StartParams` builders ≈L1600+, rerun tests ≈L2355+.
- `src-tauri/src/lib.rs` -- close handler L47-54 `unwrap_or(false)` → `unwrap_or(true)`.
- `src-tauri/src/ipc/mod.rs` -- `app_close_confirm` L1280-1285 → `Err(_) => break`. `library_transcript_export` L1063 → add `session_id: SessionId`.
- `src-tauri/src/library/store.rs` -- `get_export_data` L446 → add a `session_id` param and check `transcript.session_id`. Test at L830-877.
- `src/routes/Session.svelte` -- `handleExportTranscript` L146 → pass the session ID from `view`. `src/routes/Session.test.ts:287` expectation updates.
- `src/lib/bindings.ts` -- regenerate via `cargo test export_bindings`.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/settings/mod.rs` -- add a `MAX_CHUNK_MINUTES` range check in save and load, plus tests (61 rejected, 60 ok, stored 5000 → default) -- prevents the process abort
- [x] `src-tauri/src/media/chunk.rs` -- cap pre-allocation, plus a test that `Chunker::new` with a huge duration succeeds -- defense in depth
- [x] `src/lib/stores/settings.svelte.ts`, `src/routes/settings/SettingsChunking.svelte`, i18n JSON -- enforce max 60 in the UI, plus tests in the existing `.test.ts` files -- UI parity
- [x] `src-tauri/src/media/resample.rs` -- bounded flush, plus a unit test if feasible (otherwise document) -- no hang
- [x] `src-tauri/src/transcribe/merge.rs` -- `push_forward` text preservation and `splice_rerun` trim, plus tests -- no silent text loss
- [x] `src-tauri/src/transcribe/rerun.rs` -- open-ended tail range, plus tests (resolve marks the tail; decode routes samples beyond `end_ms`) -- no tail audio loss
- [x] `src-tauri/src/transcribe/registry.rs` -- JoinError handling, per-chunk consent re-check, INFINITY seconds-range, plus tests (consent revoked after first chunk stops the job with no commit) -- privacy and data integrity
- [x] `src-tauri/src/lib.rs`, `src-tauri/src/ipc/mod.rs` -- fail-closed close handler; `app_close_confirm` break on Err -- no mid-job exit
- [x] `src-tauri/src/library/store.rs`, `src-tauri/src/ipc/mod.rs`, `src/routes/Session.svelte`, `src/lib/bindings.ts`, tests -- export ownership check -- close the IDOR

**Acceptance Criteria:**
- Given all changes, when running the full Rust and frontend test suites, then everything passes, and `bindings.ts` has no drift after `export_bindings`.
- Given a running Job, when consent is revoked in settings, then no further chunk request is issued and no session is committed.

## Implementation Notes

All seven fixes landed as designed, each with its own regression tests.

- Registry pipeline tests that build `StartParams`/`RerunParams` with a fixed
  `ConsentSnapshot` but never touched Settings needed a small test-harness
  change (`seed_current_consent` helper, called from `registry_for_test` and
  every test that opens its own `channel(...)` directly) so the new per-chunk
  re-check sees a currently-accepted consent, matching what the real `ipc::`
  start gate already guarantees before a Job is ever created.
- `splice_rerun`'s old-segment handling changed from "drop on any overlap" to
  "drop only within 1 ms tolerance, else trim to the part outside the range"
  (`trim_outside_ranges`), covering the rounding-neighbor case without
  splitting a segment into two pieces (not reachable via `resolve_ranges`, so
  the simpler edge-trim is sufficient).
- The `MonoResampler::finish` flush bound (`MAX_FLUSH_BLOCKS = 64`) has no
  dedicated non-convergence unit test -- reliably driving a real
  `rubato::Fft` past that bound isn't feasible without a second injectable
  resampler seam, which is out of scope for a smallest-local-change fix (see
  the doc comment in `media/resample.rs` tests for how the bound itself was
  verified manually).

## Spec Change Log

## Review Triage Log

## Design Notes

Consent re-check lives in the registry loop, not the gateway. The gateway stays snapshot-based (its callers — models list, key test — are one-shot). The registry already reaches `settings`/`library` directly, so this adds no new layering violation. An open-ended rerun tail uses `end_sample = u64::MAX` rather than a new field, so existing `RerunRange` constructors/tests remain valid; `duration_ms()` still uses `end_ms` for progress estimates.

## Verification

**Commands:**
- `cd src-tauri && cargo test --locked` -- expected: all pass
- `cd src-tauri && cargo clippy --locked --all-targets -- -D warnings` -- expected: clean
- `npx vitest run` -- expected: all pass
- `npm run check` (svelte-check) -- expected: no errors
- `git diff --exit-code src/lib/bindings.ts` after the bindings test -- expected: only the intended `sessionId` change, committed
