---
title: 'Epic 2 P1 backend fixes — retry classes, model/fence guards, rerun busy, storage orphans, job unsubscribe, IPC wiring tests'
type: 'bugfix'
created: '2026-09-25'
status: 'done'
baseline_commit: '31e61c0b2bd03e398c1885f3f82928bf5c20665d'
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

**Problem:** The epic 2 review flagged P1 backend defects: flaky network errors fail a chunk on the first attempt, 5xx retries fire with no backoff, a mixed-case `*-transcribe` model and a ```` ```JSON ```` fence slip past the guards, a second rerun with a different scope is silently swallowed, storage leaves orphan folders and one bad session aborts reconcile, a relink can leave disk and DB disagreeing, job event channels are never released, and key IPC wiring has no tests.

**Approach:** Make small local fixes in Rust, plus the one frontend call needed to release a subscription, and add regression tests for each fix and for the untested IPC wiring. Run it as one pass, as the user requested.

## Boundaries & Constraints

**Always:**
- Transport-level `Code::Network` errors (connection/DNS, not HTTP status) map to `RequestOutcome::Server`, so they retry inside the existing 4-attempt budget.
- A `Server` outcome retry waits for a short backoff before it is dispatched again. Put the reporting key on a cooldown of `SERVER_BACKOFF` (e.g. 1 s × attempts so far, max 5 s), reusing the `cooldown_until` mechanism that `Quota` uses. Quota waits keep their existing semantics.
- The model guard is case-insensitive (`to_ascii_lowercase` before `contains`).
- The parser strips a leading code fence case-insensitively, with optional whitespace between ```` ``` ```` and `json`. A fenced `[]` still yields confirmed silence.
- `handle_start_rerun`: return `Existing { job_id }` only when an in-flight rerun has the same session, the same `transcript_id` and identical `ranges`. When the session has an in-flight rerun with a different target or ranges, return the new variant `RerunOutcome::Busy { job_id }` and map it to `TranscribeRerunOutcome::Busy { job_id }` over IPC. The frontend treats `busy` like an error notice that says another rerun for this session is in progress (new i18n key in vi/en/ja). Never enqueue a second rerun for the same session.
- `commit_file_session`: when `publish_proxy` failed and it had created `media/<sid>/`, remove that directory if it is empty (best-effort, logged). Every `let _ = fs::remove_*` in `library/store.rs` becomes an `if let Err(err) … tracing::warn!` with no path content beyond the session ID.
- `reconcile`: an error for one session directory is logged, and the loop continues. It never aborts the other sessions. `reconcile`'s `read_dir(...).flatten()` logs entry errors.
- `ipc/boot.rs`: when `app_data_dir()` fails, log a warning that reconcile was skipped.
- `relink_proxy`: if `set_proxy_ext` fails after the file was published, retry the DB write once. If it still fails, return an error whose detail states that the proxy file was replaced but not recorded. The next `reconcile` must fix the mismatch: when the proxy file exists but `proxy_ext` is `NULL`, set `proxy_ext` from the file (add this to `reconcile_session_dir` if it is missing).
- Add `jobs_unsubscribe(channel_id: u32)`, which removes the subscriber whose `Channel::id()` matches. `jobsStore.unsubscribe()` calls it with its channel's `id`. It is idempotent: an unknown ID is `Ok`.
- Regenerate `bindings.ts` for every IPC change.

**Never:**
- Do not make `Code::Timeout` retryable or resend a timed-out chunk on another key. The epic contract says "timeouts are never silently resent on another key". `TranscribeFailure.retryable` is not consumed by the registry; leave it as is.
- Do not change the `decide_transcribe_start` gate order (the approved 2.8 spec fixes probe → key). That review item is deferred for a human decision.
- No new dependency, no migration, and no P2/P3 items.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Transport blip | 1st attempt `TransportError::Network`, 2nd 200 | chunk succeeds | — |
| 5xx backoff | 503 then 200 | 2nd attempt dispatched only after backoff (fake clock) | — |
| Mixed-case model | `Gemini-2.5-Transcribe` | rejected | `Code::Model` |
| Upper fence silence | ```` ```JSON\n[]\n``` ```` | confirmed silence | — |
| Same rerun twice | same transcript+ranges | `Existing` | — |
| Different rerun | gap(3) running, gap(7) requested | `Busy`, no new job | frontend notice |
| Publish fails | publish write fails, commit ok | no `media/<sid>/` left | warn |
| Reconcile one bad | one session dir errors | others still reconciled | warn |
| Relink DB fails | publish ok, set_proxy_ext fails twice | error; later reconcile sets `proxy_ext` | `Code::Storage` |
| Unsubscribe | subscribe then unsubscribe(id) | channel no longer receives events | unknown id → Ok |

</frozen-after-approval>

## Code Map

- `src-tauri/src/gemini/mod.rs` -- `outcome_for_error` (L809) → `Code::Network => Server`. `classify_http_status` (L824). Test `http_statuses_and_default_models_are_stable` (≈L1647) → also assert `.0` outcomes. Test helpers `gemini::test_support::{gateway_with,key,FakeTransport}`.
- `src-tauri/src/gemini/keys.rs` -- `handle_report` `RequestOutcome::Server` arm (L624) → set the key cooldown before `retry_or_finish` (L657). The `Quota` arm (L627) shows the cooldown pattern. Test `server_failures_retry_serially_within_the_attempt_budget` (≈L988) uses a fake clock.
- `src-tauri/src/transcribe/adapter.rs` -- `model_id` guard L78.
- `src-tauri/src/transcribe/parser.rs` -- fence strip L71-83. Tests start ≈L260.
- `src-tauri/src/transcribe/registry.rs` -- `RerunOutcome` (L137), `handle_start_rerun` (L578). `JobEntry` has no transcript/ranges today: store them (e.g. in `pending_rerun_params`, or copy them into `JobEntry`) so dedup survives after the params are taken at pipeline start. `handle_subscribe` (L644) / `subscribers` (L448); add `Command::Unsubscribe` + `JobRegistryHandle::unsubscribe`.
- `src-tauri/src/ipc/mod.rs` -- `TranscribeRerunOutcome` mapping (≈L816). `jobs_subscribe` (L933) → add `jobs_unsubscribe`. Register it in `specta_builder` (≈L1320). `transcribe_start_inner` (≈L615) / `transcribe_rerun_inner` (≈L821): add tests proving `Settings.transcribe_language` / `chunk_minutes` reach the params (refactor a pure params-builder fn if that is the smallest testable seam).
- `src-tauri/src/ipc/boot.rs` -- reconcile call ≈L73-88.
- `src-tauri/src/library/store.rs` -- `publish_proxy`, `commit_file_session` cleanup, `reconcile`/`reconcile_session_dir`/`remove_path_any`, `relink_proxy`. Existing tests `commit_publish_write_failure_…`, `reconcile_…`, `relink_proxy_…` show the fault-injection pattern (`PublishWrite`/`PublishRename`).
- `src/lib/stores/jobs.svelte.ts` -- `subscribe`/`unsubscribe` (≈L100) → keep the channel and call `commands.jobsUnsubscribe(channel.id)`. `rerun` (≈L200) → handle `busy`. Tests in `jobs.svelte.test.ts`.
- `src/routes/Session.svelte` -- `handleRerunMissing` / gap rerun handlers → show the busy notice. `src/i18n/{vi,en,ja}.json` → new key.
- `src-tauri/src/transcribe/registry.rs` tests ≈L2355+ -- rerun tests hardcode `discard_old: false` and `chunk_minutes: 5`.

## Tasks & Acceptance

**Execution:**
- [x] `gemini/mod.rs`, `gemini/keys.rs` -- network → Server retry, server backoff cooldown, and tests (network retry succeeds, backoff gates the 2nd dispatch, `classify_http_status` outcome asserted for 5xx)
- [x] `transcribe/adapter.rs`, `transcribe/parser.rs` -- case-insensitive model guard and fence strip, plus tests
- [x] `transcribe/registry.rs`, `ipc/mod.rs`, `jobs.svelte.ts`, `Session.svelte`, i18n, `bindings.ts` -- rerun `Busy` outcome and tests (same → Existing, different → Busy, no second job)
- [x] `transcribe/registry.rs`, `ipc/mod.rs`, `jobs.svelte.ts`, `bindings.ts` -- unsubscribe, plus tests (actor drops the channel; the store calls unsubscribe on teardown)
- [x] `library/store.rs`, `ipc/boot.rs` -- orphan dir cleanup, logged cleanup failures, resilient reconcile, relink retry + reconcile `proxy_ext` repair, plus tests per matrix row
- [x] `ipc/mod.rs`, `transcribe/registry.rs` tests -- IPC wiring: settings language/chunk_minutes reach Start/Rerun params; a registry rerun with `discard_old: true` commits only the new segments; a rerun with `chunk_minutes: 1` splits a 150 s range into 3 chunks

**Acceptance Criteria:**
- Given all changes, when the verification commands run, then all pass, clippy is clean, and `bindings.ts` has no drift after `export_bindings`. -- met (see Verification below).

## Implementation Notes

- Server-outcome retry backoff (`SERVER_BACKOFF`/`SERVER_BACKOFF_MAX` in `gemini/params.rs`) reuses `KeyState.cooldown_until`, the same field `Quota` uses; `gemini::test_support::gateway_with` now runs its `FakeClock` on a background auto-advance ticker so every existing Server-outcome test still resolves without individually driving the clock, while a new `gateway_with_clock` helper gives precision tests (`post_job_server_backoff_gates_the_second_dispatch`, and the two new `keys.rs` backoff tests) manual control.
- `transcribe_start_inner`/`transcribe_rerun_inner` were refactored around two small pure functions, `build_start_params`/`build_rerun_params`, which is the seam the new IPC-wiring unit tests exercise directly instead of driving the whole gate chain through a real DB.
- `RerunOutcome::Busy`/`TranscribeRerunOutcome::Busy` dedup on `(transcript_id, ranges)`, stored on `JobEntry` itself (not just `pending_rerun_params`, which empties once the pipeline starts) so the same-target check keeps working for the whole lifetime of an in-flight rerun.
- `library::store::relink_proxy`'s one-retry-then-fail path for `set_proxy_ext` uses a new counted fault-injection point (`fault::set_proxy_ext_failures`) alongside the existing on/off `Point` enum, since the scenario needs "fail the first N calls" rather than a toggle.
- `reconcile_logs_and_continues_past_one_unreadable_session_directory` is `#[cfg(unix)]` (uses `chmod 000` to force a real `read_dir` failure) — no Windows-equivalent fault path exists for this row yet.

## Spec Change Log

## Review Triage Log

## Verification

**Commands:**
- `cd src-tauri && cargo test --locked` -- ran; 351 passed, 0 failed (plus 4 passed in `tests/media_corpus.rs`).
- `cd src-tauri && cargo clippy --locked --all-targets -- -D warnings` -- ran; clean.
- `cd src-tauri && cargo fmt --check` -- ran; clean.
- `npx vitest run` -- ran; 374 passed, 0 errors (includes `jobsUnsubscribe` mocks added to `Session.test.ts`/`Home.integration.test.ts`).
- `npm run check` -- ran; 0 errors, 0 warnings across 307 files.
- `bindings.ts` -- regenerated by the `export_bindings` test inside `cargo test` (adds `jobsUnsubscribe` and `TranscribeRerunOutcome`'s `busy` variant); no drift after regeneration.
