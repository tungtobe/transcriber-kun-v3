---
title: 'Epic 4 P0 review fixes — Live data loss, hang, quit, privacy'
type: 'bugfix'
created: '2026-09-29'
status: 'done'
baseline_commit: 'bc518aefbb3acbaf7886fcf8572608c6c8f79850'
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

**Problem:** The epic 4 review found twelve P0 defects in Live: sessions deletable while recording, an export that overwrites an unconfirmed file, dropped binary WebSocket frames, an app that can become unquittable, unrecoverable recordings retried forever, a noise gate that degrades the archive, an fsync stall that kills recording, partial source failure that kills recording, Stop that loses data, a mic that stays on, and a socket that stalls silently.

**Approach:** Fix all twelve in one pass, smallest local change each, one regression test per fix (rewriting the tests that assert the old behavior). Full findings: `epic-4-review-findings.md` section "P0". The user asked for one pass.

## Boundaries & Constraints

**Always:**
- Delete, wipe-all and rerun return `Busy` while a Live session is running (`state.live.is_running()`); `live_start` returns `wiping_error()` while wiping.
- Export: append the extension only when the picked path has none or a different audio extension is not present; never replace a dotted segment (`a.v2` becomes `a.v2.wav`). Put the logic in a pure helper.
- `Message::Binary` frames are decoded as UTF-8 and handled like text; invalid UTF-8 is a transport error (reconnect).
- `request_close` claim expires (about 10 s) and re-arms. `app_close_confirm` cancels jobs first, then stops Live; a job-cancel timeout is logged and does not block `authorize_exit`; a Live stop failure stays fatal. `CloseConfirm` offers "exit anyway" after a notes-flush error (new i18n keys in vi/en/ja).
- `recover_live_session` never returns Err for a missing/short/corrupt WAV: it finalizes best-effort (`recovered=1`, `complete`, duration from real data bytes, header truncated to real length; duration 0 if unreadable) so the row leaves the recovery list.
- Boot recovery claims are released by a Drop guard on every exit, including panic.
- `PcmChunk.samples` is the raw (ungated) mix used by the recorder and export; the gated mix goes in a new field used only by the Gemini feed.
- `run_consumer`: on `Lagged(n)` write `n` chunks of silence and continue; broadcast capacity raised to about 600. Source errors fail the recording only when every active input has errored.
- `finish_current` drains queued internal events before finalizing; on a non-close flush failure it restores `running` with the pending batch so Stop can be retried; `Done` is emitted only on success; a close-retry failure sets `RecordingState::Failed`.
- Decision (finding 11 wording conflicts with the shipped mid-session source swap in Live.svelte): route `live_set_source` through the actor. Reject with `Code::Request` when NOT running; when running, perform the swap inside the actor (serialized with start/stop). Mid-session swap remains supported. No preview capture is opened while idle.
- Socket: idle receive timeout plus periodic ping (new `LiveSocket` method; update fakes) triggers reconnect; in-band `error` after setup triggers reconnect; `WireServerError.code` tolerates string/large numbers; key-status retry uses `wait_before_retry` backoff.

**Never:**
- No new dependency or DB migration. No change to P1 items. Do not weaken existing guards. Content-free logging (counts, not text).

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Delete while recording | Live running | `Busy` | none |
| Export name `a.v2` | picked `a.v2`, WAV | `a.v2.wav` | none |
| Binary setupComplete | Binary frame w/ JSON | handled as text | invalid UTF-8 → reconnect |
| Recovery, WAV shorter than checkpoint | short file | row `complete`, recovered=1 | no Err |
| fsync stall > 3 s | `Lagged(n)` | silence written, recording continues | none |
| Mixed mode, mic fails | 1 of 2 inputs errored | keeps recording | logged |
| Stop while events queued | queued deltas | persisted before finalize | flush fail → Stop retryable |
| `live_set_source` idle | no session | `Code::Request` | none |
| Idle socket | no frame within timeout | reconnect | none |

</frozen-after-approval>

## Code Map

Paths under `src-tauri/src` unless noted.
- `ipc/mod.rs` -- `library_session_delete_inner` (~1960), `library_wipe_all_inner` (~2606), `live_start` (~227), `live_set_source` (~211), `pick_recording_export_destination` (~1640, `set_extension`), `app_close_confirm` (~2660); tests ~2779, decide_* tests ~4063-4150.
- `ipc/close.rs` -- `request_close` claim; tests ~106-167 use `confirm_then_exit` closures.
- `ipc/boot.rs` -- recovery worker ~157-252; add `ClaimGuard`.
- `gemini/live/mod.rs` -- `receive_text` (~2289), `run_socket` (~1600-1900), `WireServerError` (~1949), `KeyRetry` (~1547); `LiveSocket` trait, `FakeSocket` (~560), tests from line 190.
- `library/store.rs` -- `recover_live_session` (~247), `finalize_checkpointed_wav` (~402); shared with `repair_live_proxy` (P1, keep contract compatible).
- `audio/mod.rs` -- `ingest` (~710), `tick` (~763), `SoftGate` (~824), `PcmChunk`, `OUTPUT_BROADCAST_CAPACITY` (32); gate tests ~1162, ~1215 must be updated.
- `live/recording.rs` -- `run_consumer` (`Lagged` ~481, `source_errors` ~393); tests ~898, ~938, ~979 (lag test must be inverted).
- `live/mod.rs` -- actor, `finish_current` (871-1170), commands enum, `LiveHandle::is_running` (213); test fixture ~1528, `database_flush_failure_restores_pending_then_stops_recording_for_recovery` (~2015) must be inverted.
- `src/components/CloseConfirm.svelte`, `.test.ts`; `src/lib/stores/app.svelte.ts`; `src/lib/stores/live.svelte.ts`; `src/i18n/{vi,en,ja}.json`; `src/lib/bindings.ts` (regenerate with `npm run bindings` only if IPC changes).
- Coupling: do 7+8+9 together (`PcmChunk`, `run_consumer`); 10+11+1+4 together (actor); 3+12 together (`LiveSocket`); 2, 5, 6 independent.

## Tasks & Acceptance

**Execution:**
- [ ] `ipc/mod.rs`, `ipc/close.rs`, `ipc/boot.rs` -- items 1, 2, 4 (Rust side), 6 -- guards, pure export-path helper, claim expiry, job-first close, ClaimGuard; tests for each
- [ ] `gemini/live/mod.rs` -- items 3, 12 -- `decode_frame` helper, idle timeout + ping, in-band error, tolerant code, backoff; tests
- [ ] `library/store.rs` -- item 5 -- best-effort recovery; test with short/missing/corrupt WAV
- [ ] `audio/mod.rs`, `live/recording.rs` -- items 7, 8, 9 -- raw vs gated mix, lag silence fill + capacity, all-inputs-failed rule; update/invert tests
- [ ] `live/mod.rs` -- items 10, 11 -- drain internals, retryable Stop, `Command::SetSource`; `ipc/mod.rs` `live_set_source` routes through it; tests
- [ ] `src/components/CloseConfirm.svelte`, `src/lib/stores/app.svelte.ts`, i18n -- item 4 frontend -- "exit anyway" after flush error; tests

**Acceptance Criteria:**
- Given a running Live session, when delete or wipe-all is invoked, then `Busy` is returned and nothing is removed.
- Given a fsync stall or a lagging consumer, when chunks are missed, then the recording continues with silence in the gap.
- Given a failed Stop flush, when Stop is invoked again, then the pending data is retried and can be saved.
- Given a notes-flush failure at close, when the user chooses exit anyway, then the app exits.

## Implementation Notes

- Rerun `Busy` guard skipped: `TranscribeRerunOutcome::Busy` needs a job_id a Live session lacks. Delete, wipe, start covered.
- Reused existing unused i18n keys `closeConfirm.flushError.*`.
- Key-status retries now back off, so key failover is slower.
- A failed Stop leaves a stopped-but-running session (`RecordingState::Failed`) until Stop is retried.
- `clippy -D warnings` still fails on 17 pre-existing errors (18 at baseline).

## Spec Change Log

## Review Triage Log

## Design Notes

Item 11: finding text said "reject while a session runs", but `Live.svelte` swaps source mid-session by design; the intended bug is capture opened without a session, so the rule is inverted to reject when NOT running.

## Verification

**Commands:**
- `cargo test --locked --manifest-path src-tauri/Cargo.toml` -- expected: all pass
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` and `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` -- expected: clean
- `npm test`, `npm run check`, `npm run check:i18n` -- expected: pass
