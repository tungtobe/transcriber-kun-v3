# Epic 4 review findings (bmad-review, 2026-09-29)

Diff reviewed: `8e79dc5..c3294ec` (stories 4.1–4.12). Lenses: adversarial, edge-case-hunter, verification-gap.
Split follows the epic 2 precedent: P0 = data loss, hang, privacy, cannot-quit. P1 = everything else.

## P0 — fix first (one pass, regression test per fix)

1. **Delete/wipe/start vs running Live session** — `ipc/mod.rs` `library_session_delete_inner`, `library_wipe_all_inner`, `live_start`. Only Jobs are checked. Reject with Busy when the Live session is running; `live_start` must reject while `wiping`.
2. **Export overwrites unconfirmed file** — `ipc/mod.rs` `pick_recording_export_destination`. `set_extension` rewrites the confirmed path (`a.v2` → `a.wav`). Append the extension only when none is present.
3. **Binary WS frames dropped** — `gemini/live/mod.rs` `TungsteniteLiveSocket::receive_text`. Decode `Message::Binary` as UTF-8 text; fail closed if invalid.
4. **App cannot quit** — `ipc/close.rs` `request_close`: `close_requested` stays true if no frontend listener handles it. Add expiry/re-arm. Also `app_close_confirm`: cancel jobs before stopping Live; a job that ignores cancel after the 4 s deadline must not block exit after the user confirmed. Restore an "exit anyway" path in `CloseConfirm.svelte` after a notes-flush failure.
5. **Unrecoverable WAV retried forever** — `library/store.rs` `recover_live_session` / `finalize_checkpointed_wav`. WAV shorter than checkpoint or missing: finalize best-effort (`recovered=1`, `complete`) or mark failed; never leave a permanent `recording` row.
6. **Boot recovery claim leak** — `ipc/boot.rs`. Panic before cleanup leaves sessions in `recovering` forever. Use a Drop guard to release claims.
7. **Noise gate degrades the archival WAV** — `audio/mod.rs` `SoftGate`/`ingest`. Gate only the Gemini-bound copy; recorder and export get ungated audio.
8. **Recording consumer aborts on fsync stall** — `live/recording.rs` `run_consumer` + `OUTPUT_BROADCAST_CAPACITY=32`. Decouple drain from fsync, or raise capacity; on `Lagged` write silence for the missing range instead of failing.
9. **Partial source failure kills recording** — `run_consumer` `source_errors`. Fail only when all active inputs failed.
10. **Stop loses data** — `live/mod.rs` `finish_current`: drain queued `internal_receiver` events before finalizing; on non-close `final_flush` failure keep `running` + `retry_batch` so Stop can be retried; on close retry set `RecordingState::Failed` and do not emit `Done` until success.
11. **Mic stays on / source swap bypasses actor** — `ipc/mod.rs` `live_set_source`. Route through the actor; reject during a running session; stop preview capture when leaving Live.
12. **Half-open socket stalls silently** — `gemini/live/mod.rs`. Add send/receive idle timeout and ping-based liveness; in-band `error` after establish must reconnect; `WireServerError.code` must tolerate string/large values; key-status retries must back off.

## P1 — second pass

- `set_source` mutates state before `prepare()` succeeds (`audio/mod.rs`).
- Input queue unbounded / no clock drift compensation; Mixed mode has no headroom or limiter.
- `note_source_error` ignores generation; `inputs` never pruned.
- WASAPI `timestamp_error` / discontinuity handling (`windows_loopback.rs`).
- Stop blocks actor on synchronous proxy encode; `repair_live_proxy` retries forever each boot and truncates WAV of `complete` sessions.
- Export is not locked against delete/wipe; `.partial` litter after crash.
- `spawn_audio_ingest` `Lagged` records no `GapRange`; `TurnComplete` after reconnect uses wrong `sample_end`.
- `split_complete_sentences` splits decimals, abbreviations, `...`; punctuation-only segments persisted.
- `start()` guard ignores `pending_close_finalize` / `close_failed`; failed `create_live_transcript` leaves phantom `recording` row.
- `update_live_progress` can reopen a `complete` session.
- Subscriber Channels never unregistered; store `lines` lost after seq gap, and not cleared on `start()`.
- `duration_sec` should come from written samples, not sample clock.
- `default_title` locale from env vars; `updated_at` ms revision token.

## Verification gaps (add with the fixes they cover)

`TurnComplete` arm; `finish_current` with unterminated buffer; `split_complete_sentences` edge cases; 20-tag cap; `repair_live_proxy` stale candidate; `memo view_for_source` and `capture_inputs` ownership guard; `library_recording_export` reservation/release; migration 8 unique index; close pipeline closures (`app_close_confirm`, `release`); `boot()` claim/release (current ipc test only exercises a local HashSet).
