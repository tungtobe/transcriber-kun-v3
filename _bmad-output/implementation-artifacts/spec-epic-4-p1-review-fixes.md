---
title: 'Epic 4 P1 review fixes — Live audio, proxy, transcript, verification gaps'
type: 'bugfix'
created: '2026-09-29'
status: 'done'
baseline_commit: '50ac4e420a837c6904b8a90e6bc2fe5e457e6047'
route: 'full'
route_source: 'auto'
review: 'none'
review_source: 'pinned'
lenses_ran: []
review_loop_iteration: 0
context: ['{project-root}/_bmad-output/implementation-artifacts/epic-4-review-findings.md']
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** After the P0 pass (merged, `spec-epic-4-p0-review-fixes.md`), the epic 4 review still lists P1 defects (audio pipeline drift/headroom/generation, WASAPI timestamps, blocking proxy encode and endless proxy retry, export vs delete, gap and cursor accuracy, sentence splitting, actor start guard and phantom rows, progress reopening a complete session, leaked subscribers and stale frontend lines, duration source, title locale, revision token) plus untested guards listed under "Verification gaps".

**Approach:** Fix each P1 item in the findings file section "P1 — second pass" with the smallest local change and a regression test, and add the tests listed under "Verification gaps" (those not already covered by the P0 pass). One pass; the user asked for it. Where a P1 item was already resolved by P0 work, say so in Implementation Notes and add only the missing test.

## Boundaries & Constraints

**Always:**
- Audio: cap each input queue (drop oldest beyond about 500 ms backlog, counted in a log, content-free); on output-clock deadline overrun emit the missed chunks instead of resetting the deadline; Mixed mode gets headroom (scale or soft limiter) before PCM16 conversion; `note_source_error` matches (generation, side) and `inputs` of dead generations are pruned; `set_source` mutates state only after `prepare()`/`start()` succeed.
- WASAPI: a packet flagged `timestamp_error` or discontinuity uses the expected next frame, not the reported timestamp.
- Live stop: the proxy encode no longer blocks the actor; the session is committed `complete` first, then the proxy is created in a detached task guarded by the `recovering` set. `repair_live_proxy` records a failed attempt so it is not retried at every boot, does not truncate the WAV of a `complete` session, and claims candidates lazily.
- Export and delete: delete/wipe are refused (or wait) while a recording export for that session is active. Staging `.partial` files use the app staging dir when the destination is on the same volume; otherwise keep the sibling file and remove it on every error path.
- Gateway ingest `Lagged(n)` records a `GapRange` for the missing span; after a reconnect, `TurnComplete` uses the replay cursor (first ring chunk start) when nothing was sent on the new socket.
- `split_complete_sentences` does not split between digits, on `...`, or on abbreviations followed by lowercase; never persists a punctuation-only segment.
- `start()` refuses while `pending_close_finalize` or `close_failed` is set; a failure after `create_live_transcript` finalizes or removes the row in-process so no phantom `recording` row remains.
- `update_live_progress` only writes while status is `recording`.
- Live subscribers are unregistered when the frontend unsubscribes or its channel fails; the store reloads persisted lines after a seq gap or remount and clears `lines`, `draft`, `finalizedSessionId` in `start()`.
- `duration_sec` at finalize comes from written samples / 16000 (WAV length), falling back to the clock only if unavailable.
- `default_title` locale comes from the locale passed by `live_start`, then `sys-locale` if already a dependency; no new dependency, else the env fallback stays.
- Revision token: strictly monotonic (`max(now, previous + 1)`) wherever `updated_at` is written by the paths that use it as the conditional token.
- Every verification gap listed in the findings file is covered by a test, or noted in Implementation Notes as already covered.

**Never:**
- No DB migration beyond one column/table only if strictly required for the proxy-attempt marker (prefer reusing existing columns). No new dependency. Do not change P0 behavior. Content-free logging.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Fast device clock | input pushes faster than output clock | queue capped ≈500 ms, oldest dropped | logged count |
| Loud Mixed | both inputs near full scale | no hard clipping | none |
| Sentence `3.14 is out.` | streamed text | one segment `3.14 is out.` | none |
| Proxy fails deterministically | boot repair | attempted once, not retried each boot | marker set |
| Stop on long recording | proxy encode slow | session `complete`, Stop returns promptly | proxy later or at boot |
| Ingest lag | `Lagged(n)` | GapRange for missing span | none |
| Late progress flush | status `complete` | no write | none |
| Start after failed close | `close_failed` | refused | Code::Request |

</frozen-after-approval>

## Code Map

Paths under `src-tauri/src` unless noted; line numbers drift after the P0 pass, locate by symbol.
- `audio/mod.rs` -- `AudioPipeline::{ingest,tick,note_source_error,preserve_active_samples}`, `InputState.samples`, `spawn_output_clock`, `CaptureController::set_source`.
- `audio/windows_loopback.rs` -- `read_packets`, `TimedSystemMixer::push` (Windows only; unit-test the pure timestamp logic).
- `library/store.rs` -- `finalize_live_session`, `repair_live_proxy`, `create_live_session_with_tags`, `commit_retranscribe`, `updated_at` writes; `ipc/boot.rs` proxy loop.
- `db/repo/sessions.rs` -- `update_live_progress`; `db/migrations/mod.rs` (migration 8 index test).
- `ipc/mod.rs` -- `library_recording_export`, `library_session_delete_inner`, `library_wipe_all_inner`; `media/recording_export.rs` staging.
- `gemini/live/mod.rs` -- `spawn_audio_ingest`, `run_socket` TurnComplete cursor.
- `live/mod.rs` -- `split_complete_sentences`, `start`, `finish_current` duration, `register_subscriber`; `live/recording.rs` -- `default_title`, `system_locale`.
- `memo/generate.rs` -- `view_for_source`, `capture_inputs` (tests only).
- `src/lib/stores/live.svelte.ts` (+ `.test.ts`), `src/routes/Live.svelte`; new unsubscribe command changes `src/lib/bindings.ts` (`npm run bindings`).
- Test commands: `cargo test --locked --manifest-path src-tauri/Cargo.toml`, `npm test`, `npm run check`, `npm run check:i18n`, `cargo fmt --manifest-path src-tauri/Cargo.toml --check`.

## Tasks & Acceptance

**Execution:**
- [ ] `audio/mod.rs`, `audio/windows_loopback.rs` -- audio items (queue cap, deadline catch-up, headroom, generation matching and pruning, set_source ordering, WASAPI timestamps) -- tests
- [ ] `library/store.rs`, `db/repo/sessions.rs`, `ipc/boot.rs`, `live/mod.rs` -- proxy detach and attempt marker, progress guard, start guard, phantom row, duration source -- tests
- [ ] `ipc/mod.rs`, `media/recording_export.rs` -- export/delete exclusion, staging cleanup -- tests
- [ ] `gemini/live/mod.rs`, `live/mod.rs` -- Lagged GapRange, replay cursor, sentence splitting -- tests
- [ ] `live/mod.rs`, `ipc/mod.rs`, `src/lib/stores/live.svelte.ts` -- subscriber unregister, reload lines after gap, clear on start -- tests
- [ ] `live/recording.rs`, `library/store.rs` -- title locale, monotonic revision token -- tests
- [ ] tests for the remaining verification gaps (TurnComplete arm, finish_current unterminated buffer, 20-tag cap, repair_live_proxy stale candidate, memo view_for_source and capture_inputs ownership, export reservation, migration 8 index, close pipeline, boot claim/release)

**Acceptance Criteria:**
- Given a long Live session, when Stop is invoked, then it returns without waiting for proxy encoding and the session is `complete`.
- Given a proxy that always fails, when the app boots repeatedly, then repair is attempted once.
- Given a seq gap on the Live screen, when the store resubscribes, then persisted lines are shown.

## Implementation Notes

- Proxy-attempt marker reuses the filesystem, not a column: `media/<sid>/.proxy-failed` (kept by `reconcile`, removed with the session dir). `repair_live_proxy` also validates the WAV read-only (no `set_len`) and skips marked sessions.
- Stop now always commits via `finalize_live_session_minimal`, then `spawn_proxy_derivation` claims the session in the shared `recovering` set (`LiveSessionActor::with_recovering`, wired in `ipc/boot.rs`) and encodes in a detached blocking task. No Home notification is emitted for that late Proxy (the actor has no app handle). `ClaimGuard` moved to `library::store`; boot claims Proxy candidates lazily and interrupted sessions eagerly (`run_boot_recovery`, now unit-tested).
- `finalize_live` now writes the passed duration (no `MAX`); Stop passes WAV samples / 16000 (`RecordingStopOutcome.written_samples`), clock only as fallback.
- Revision token: `updated_at = MAX(now, updated_at + 1)` in every `sessions` UPDATE in `db/repo/sessions.rs`. `update_live_progress` requires `status = 'recording'`.
- `default_title`: the locale passed by `live_start` already took priority; a blank value now falls back to the env. `sys-locale` is not a dependency, so the env fallback stays.
- Gateway `Lagged(n)`: `AudioRing::note_lagged` records the gap right after the last observed sample. The TurnComplete/InputTranscription end sample uses the replay cursor when nothing was sent on the new socket (`transcript_sample_end`).
- `split_complete_sentences`: a trailing `<digit>.` or a known abbreviation (`Dr.`, `e.g.`, ...) at the end of the buffer waits for the next delta; such text is flushed by TurnComplete or Stop.
- Frontend: new `live_unsubscribe(channel)` command (bindings regenerated); the store drops its Channel on last unsubscribe and on resubscribe, reloads persisted lines from `librarySessionDetail` on every `ready`, and clears lines/draft/finalizedSessionId in `start()`.
- Export vs delete: `AppState.recording_export` now holds `ActiveRecordingExport { session_id, cancel }`; delete of that session and wipe return Busy while it is held. `export_recording` takes an optional staging dir (used when on the same volume, else the sibling partial file; the partial is removed on every error path).
- Already covered before this pass (test only): `finish_current` queued-event drain, close retry failure, `set_source` rejection when idle, `ClaimGuard` panic release. Windows WASAPI timestamp logic is unit-tested on the pure mixer only (`TimedSystemMixer::push_checked_at`).

## Spec Change Log

## Review Triage Log

## Verification

**Commands:**
- `cargo test --locked --manifest-path src-tauri/Cargo.toml` -- expected: all pass
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` -- expected: clean; `cargo clippy` must not add errors beyond the 17 pre-existing
- `npm test`, `npm run check`, `npm run check:i18n` -- expected: pass
