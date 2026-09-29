---
title: 'Story 4.12: Tải Recording WAV hoặc FLAC'
type: 'feature'
created: '2026-09-29'
status: 'done'
baseline_commit: 'f0f5b11c021d738b2569a1e4b0ba9e60157abf46'
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

**Problem:** Users cannot save a live session's durable Recording outside the app. **Approach:** Add Download Recording to the Home and Session detail menus, use a system save dialog for WAV/FLAC, and stream the export with progress, cancellation and safe publication.

## Boundaries & Constraints

**Always:** Only live sessions with a persisted Recording may export; recording/finalizing sessions show a disabled action with a reason. Dialog offers WAV and FLAC only on macOS and Windows. WAV copies from the canonical Recording; FLAC encodes in bounded blocks without loading the full file. Show progress based on recorded duration and a Cancel action. Write to a temporary sibling and publish only after successful completion, preserving an existing destination on cancel/error. Clean temporary output on every failed exit. Storage failures return a `storage` AppError with actionable guidance. A successful export shows a four-second “Đã lưu” toast.

**Never:** Modify/delete the source Recording, expose M4A/AAC choices, leave partial output after failure, or overwrite an existing destination before encode/copy completes.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|---------------|---------------------------|----------------|
| Menu | Finalized live vs file or active live session | Download in both live menus; absent for file, disabled with reason while active | No export starts while recording/finalizing |
| WAV | User chooses WAV destination | Streaming copy, progress and “Đã lưu” toast | Cancel removes staging; source and old destination intact |
| FLAC | User chooses FLAC destination | Streaming encode, bounded memory, progress and same toast | Encode/write error returns storage category and removes staging |
| Existing target | Target file already exists | Replace only after successful encode/copy | Cancel/error preserves old bytes |

</frozen-after-approval>

## Code Map

- `src-tauri/src/core/paths.rs` -- canonical `recording_path` for live WAV.
- `src-tauri/src/media/{flac,proxy,decode}.rs` -- reusable streaming FLAC writer/decoder; avoid collecting whole audio.
- `src-tauri/src/ipc/mod.rs`, `src-tauri/src/ipc/boot.rs` -- typed commands, native save dialog pattern and app state for export progress/cancel.
- `src-tauri/src/db/repo/sessions.rs`, `src-tauri/src/library/store.rs` -- session kind/status and Home rows; ensure availability/active reason is backend-validated.
- `src/components/SessionMenu.svelte`, `src/routes/home/SessionRow.svelte`, `src/routes/session/SessionHeader.svelte`, `src/routes/{Home,Session}.svelte` -- shared menu action, progress/cancel UI and toast.
- `src/lib/bindings.ts`, `src/i18n/{vi,en,ja}.json` -- generated typed IPC and localized labels.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/media/*`, `src-tauri/src/library/*` -- stream WAV/FLAC to sibling staging with cancellation/progress and safe publish; test large input, errors and existing destination.
- [x] `src-tauri/src/ipc/mod.rs` and state -- validate session, present WAV/FLAC native dialog, coordinate progress/cancel via typed API; test invalid kind/status and cancellation.
- [x] `src/components/SessionMenu.svelte`, Home/Session routes, bindings and locales -- expose live-only action, busy reason, progress/cancel and four-second success toast; test both menus.

**Acceptance Criteria:**
- Given a finalized live session, when Download Recording is selected from Home or detail, then the system dialog offers only WAV/FLAC and the selected format is exported with visible progress and cancellation.
- Given a write/encode failure or cancellation, when export ends, then the source and previous destination remain intact and no temporary file remains.
- Given an active live session or file session, when its menu is opened, then export cannot start and the UI explains the active-session restriction where applicable.

## Implementation Notes

Export writes a temporary sibling in bounded blocks, then atomically replaces the destination on success. The UI asks for WAV or FLAC before showing a format-specific native save dialog so the selected format cannot be inferred incorrectly from a filename. The backend validates kind, status and canonical Recording before and after the dialog. Windows uses `MoveFileExW` for replace; that path and native dialog behavior were not exercised in this environment.

## Spec Change Log

## Review Triage Log

## Design Notes

Use a sibling staging file so publication is on the target volume. Revalidate kind/status/source in Rust before work begins, even when the menu already disables the action.

## Verification

**Commands:**
- `cargo test --locked --manifest-path src-tauri/Cargo.toml` -- streaming/export/error tests and Rust suite pass.
- `npm run bindings && npm run check && npm test` -- generated bindings, Svelte check and UI tests pass.
- `npm run check:i18n && npm run check:ui && git diff --check` -- locales, tokens and whitespace pass.

**Results:** 587 Rust unit tests and 4 integration tests, 648 frontend tests, bindings, Svelte check, i18n, UI token and diff checks passed. The additional storage-failure test passed after the full runs. Review was pinned to none. No work was deferred.
