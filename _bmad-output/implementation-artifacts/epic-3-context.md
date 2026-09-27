# Epic 3 Context: Thư viện phiên, Ghi chú & Memo

<!-- Compiled from planning artifacts. Edit freely. Regenerate with compile-epic-context if planning docs change. -->

## Goal

Turn the session list from Epic 2 into a usable library: users find old sessions back by name and tag, rename them inline, and delete them (and everything they own) without leaving orphaned files; they type meeting notes that autosave reliably even through a force-quit; and they generate, regenerate, and copy a memo/議事録 from transcript + notes using their own templates, managed centrally in Settings. This is the payoff moment for the "organize and hand off a meeting" journey (UJ-3/UJ-4).

## Stories

- Story 3.1: Đổi tên và xoá phiên
- Story 3.2: Tag — gắn, lọc và quản lý
- Story 3.3: Tìm phiên theo tên kết hợp lọc tag
- Story 3.4: Settings — Lưu trữ trong Container
- Story 3.5: Ghi chú tự lưu
- Story 3.6: Template memo — quản lý trong Settings
- Story 3.7: Sinh, cache và sinh lại Memo

## Requirements & Constraints

- Rename is inline (Enter saves, Esc cancels), empty rejected, max 200 chars. Delete removes transcript, segments, proxy, recording, memo, notes, and tag links from the Container in one confirmed action, leaving zero orphaned files; a tag with no sessions left still survives until explicitly deleted. Delete is blocked (with inline explanation) while the session has an active job or is being recorded live, and stays blocked until that finishes or is cancelled.
- Tags: up to 20 tags/session, 80 chars/tag, trimmed and case-insensitively deduped (must hold for Vietnamese/Japanese case-folding, not just `COLLATE NOCASE` — e.g. "DỰ ÁN" vs "dự án", whitespace-only input, concurrent create). Filtering by multiple tags is AND; an "untagged" filter is exclusive with specific-tag filters. Tags survive rename, re-run, and retranscribe. Global tag deletion removes it from every session in one confirmed action.
- Home search filters as-you-type, case-insensitive, whitespace-tolerant, ≤200ms at 500 sessions; combines with tag filter as AND; empty result shows a clear-filters affordance; footer always shows "N / M phiên · lọc…".
- Storage settings show Media + DB size and session count, an "open folder" action where the OS allows it, and no cache-folder option. "Delete all data" needs a two-step confirmation; scope (meeting data only vs. also key/settings/Consent/templates) is an open decision (OQ9) that must be resolved before this behavior ships. Deleting all data is blocked while a job/live session is active; no background task ever auto-deletes user data.
- Notes autosave per session via debounce (~800ms), with a "Đã lưu hh:mm" indicator; pending edits are flushed before navigating away, closing the panel, or closing the app. The same notes component is reused, unmodified, in Live (Epic 4). Notes durability across a force-quit before the debounce/ACK fires is an open gap (OQ10) — not to be accepted as solved by a normal-close test alone. Notes content is wrapped for redaction and never appears in logs.
- Memo templates require `{transcript}` (blocked save if missing, with inline error) and support optional `{notes}`; a default set exists per UI language (vi/en/ja), content TBD (OQ8). Deleting a default template is disabled; deleting a user template needs inline confirmation (no extra dialog). "Restore defaults" only rewrites the current UI language's default set and never touches user-created templates.
- Memo generation is cached per (session, template); reopening never re-calls Gemini. Regenerate only replaces the cached memo on a new successful, committed result — errors/cancels keep the old memo, and late/duplicate results never overwrite a newer request. Memo errors (quota/auth/network) surface inline in the memo panel by category, never touching transcript or notes. A retranscribed/re-run session keeps its old memo labeled as generated from a prior version. Memo generation is disabled (with reason) when there's no consent/key, no transcript text, or only gaps; a partial transcript is usable but the prompt/source line must disclose the missing ranges.
- Cross-cutting: session name, tag name, template name, and any ID are never used as filesystem path components — all Container files are named from generated IDs.

## Technical Decisions

- New feature modules: `library/{sessions,tags,notes,export}` and `memo/`; cross-feature orchestration (e.g. `session_delete` checking `JobRegistry`/`LiveSession` busy state) happens only in `ipc/`, never feature-to-feature.
- Schema additions this epic: `tags(id, name UNIQUE COLLATE NOCASE)` + `session_tags(session_id, tag_id)`; `notes(session_id PK, body, updated_at)`; `memo_templates(id, name, prompt, is_default)`; `memos(session_id, template_id, body, created_at)` plus provenance columns (transcript_id/revision, notes revision, template revision/snapshot, model used) needed to show "memo generated from a prior version". All IDs are app-generated UUIDv7; no user/server string is ever a path segment.
- Delete/whole-wipe concurrency: deletion holds exclusive per-session (or global, for wipe-all) intent while in progress, rejects/cancels racing writers (memo, export, proxy build, new job), and never reports success while data still exists; a failed filesystem/DB delete reports a `storage` category error and stays retryable/idempotent on next boot cleanup.
- Notes revisions increase monotonically per session so an out-of-order save ACK can never overwrite a newer one; "saved" is only shown after a durable commit ACK, not on debounce fire.
- Memo generation goes through the shared `gemini/` gateway (Consent-gated, `KeyPool` priority `Live > Job > Memo`) as its own single-request call — not part of the job queue — with a 90s timeout; failures never change transcript status (AD-19).
- Tag picker is one shared 320px-wide component used in three places (Home filter, Transcript detail attach, LiveSetup in Epic 4) with two modes ("Đang lọc" vs "Đã chọn"); the notes panel component is likewise shared verbatim between Transcript detail and Live.

## UX & Interaction Patterns

- Session row / Transcript detail header: rename-in-place via a menu ("⋯") item or click-on-name; the same menu has Delete; both keyboard-reachable, popovers close on Esc.
- Home tag row is a single non-wrapping line: active filter chips (with ×) → 3-5 most-used tags → "+N tag khác" chip → a divider → "Chưa gắn tag" chip.
- Tag management mode lets a tag be deleted globally via confirmation dialog; popovers restore focus to their opening control on close.
- Settings gains two new groups: "Lưu trữ" (storage bar, open-folder, two-step "xoá toàn bộ dữ liệu…" with a `button-danger-soft` final action) and "Memo" (master-detail: template list with "Mặc định · vi" / "Của bạn" badges + "+ Thêm mẫu" on the left, name + mono prompt textarea editor on the right — the only place templates are added/edited/deleted).
- Notes and Memo are both tabs inside the 360px side panel of Transcript detail. Memo panel has a template select, a "Sinh / Sinh lại" button badged "Tốn token Gemini", and a source line ("Sinh từ bản X + ghi chú · giờ"). Memo renders as sanitized Markdown (`marked` + `DOMPurify`, external links only, no scripts) with Copy and download-`.md` actions; a toast confirms completion if the user has navigated away. A session with a memo shows a memo badge in its meta area.

## Cross-Story Dependencies

- 3.1 (rename/delete) depends on Home from 2.9 and is the base for 3.2 (tags) and 3.4 (storage/wipe-all), both of which depend on 3.1's delete/session-action plumbing. 3.3 (search) depends on 3.2's tag filter.
- 3.5 (notes) depends on 2.7 (Transcript detail) and is reused by Live (Epic 4, story 4.7/4.9). 3.6 (templates) depends on Epic 1's Settings shell (1.9). 3.7 (memo generation) depends on 3.5 (notes), 3.6 (templates), and 2.10 (export/copy patterns).
- Shared components/state cross into Epic 4: the tag picker (also used by LiveSetup) and the notes panel are built once here and reused, unmodified, by Live sessions.
- Open product decisions block acceptance, not just polish: OQ9 (exact scope of "xoá toàn bộ dữ liệu") gates 3.4; OQ10 (notes durability mechanism/acceptable loss window on kill) gates 3.5; OQ8 (default template content per language) gates 3.6.
