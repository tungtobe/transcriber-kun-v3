# Epic 2 Context: Transcribe file & xem kết quả

<!-- Compiled from planning artifacts. Edit freely. Regenerate with compile-epic-context if planning docs change. -->

## Goal

Let a user drag or pick a recording/video file and get a timestamped transcript they can trust: audio is decoded and chunked without external dependencies, sent to Gemini, and reassembled into segments with absolute timestamps; playback and click-to-seek use an in-container proxy so the original source file is never needed again; failures on individual chunks are always surfaced as explicit gaps and can be re-run without reprocessing the whole file; re-opening an already-transcribed file costs no tokens. This epic also introduces Home (drop-zone, session list, running-job card), the natural landing surface once file sessions exist.

## Stories

- Story 2.1: Media pipeline thuần Rust và kiểm chứng phát FLAC (spike S1, S8)
- Story 2.2: Gemini transcribe file — model tổng quát và parser chịu lỗi (spike S2)
- Story 2.3: Lưu Phiên và Transcript bền vững
- Story 2.4: Job transcribe file — hàng đợi, tiến độ và huỷ
- Story 2.5: Chịu lỗi Chunk, Khoảng thiếu và Chạy lại
- Story 2.6: Settings — Chunking, offset và ngôn ngữ transcribe
- Story 2.7: Transcript detail — hiển thị, trình phát và click-to-seek
- Story 2.8: Nhận file — dialog, kéo thả, kiểm tra định dạng, phát hiện trùng
- Story 2.9: Home — danh sách phiên và card job
- Story 2.10: Tìm trong transcript, export và copy

## Requirements & Constraints

- Supported inputs: audio mp3/m4a/wav/flac/ogg(vorbis)/aiff/caf and video mp4/mov/mkv/webm (audio track only); unsupported formats are rejected before any session is created, naming a format to switch to.
- After session creation, playback/seek/re-run/export always use the in-container proxy, never the original file; a broken proxy still lets the transcript open, offering to re-pick the source to rebuild it.
- Duplicate detection is by content hash: an already-transcribed file reopens its existing session with no Gemini call and no new job; a reservation stops concurrent duplicates from both hitting Gemini.
- Job progress is audio minutes processed, not chunk count; cancel stops new sends within ≤2s and never saves a half-finished session; the job stays visible on navigation away; closing the app mid-job asks for confirmation first.
- Chunking defaults to 5 minutes (min 1, configurable); Gemini's relative MM:SS segment times convert to absolute time via the chunk offset and must never go backward; oversized chunks auto-split further. A separate display-only global offset applies only at display/export, never mutating stored data. Transcribe language (auto/ja/vi/en, default auto) applies to file and Live; "auto" never forces a language in the prompt.
- Each chunk gets at most 4 total send attempts (initial + key rotations, one shared counter); quota waits don't count and cap at 180s/chunk; 5xx retries within budget, 400/404/blocked fail immediately, timeouts are never silently resent on another key.
- A chunk that fails for good becomes a `chunk_failed` gap and the transcript saves `partial` (never complete), but the session still commits. Re-run sources audio only from the proxy, uses the same sequential queue, keeps untouched segments, and atomically swaps the target transcript — never overwriting a live primary transcript.
- Player seek must be ≤500ms anywhere in a 90-minute file; clicking a segment seeks to its raw (pre-offset) start; the seek bar is keyboard-operable; audio never auto-plays.
- Search responds ≤100ms against ~700 segments, case-insensitive, cyclic prev/next. Export (`.txt/.srt/.json`) applies the global offset; `.json` keeps the hidden speaker field; partial transcripts note gaps in `.txt`; `.srt` drops cues invalidated by offset normalization and renumbers the rest.
- Home's list renders 500 sessions in ≤1s (app reaches Home in ≤2s) and omits Model/Segment-count/Status columns; a running job shows as a card with chunk/key/retry detail and a "waiting for quota" state.
- Cross-cutting: decode+chunk ≥20× realtime on 4 cores; only the user-granted file permission is needed for intake; every user-visible error maps to one of eight stable categories, never leaking stack traces/keys/URLs; no user-provided string may become a filesystem path component.

## Technical Decisions

- Strict dependency direction `ipc → feature → infra → core`; features never call each other directly — cross-feature flows are orchestrated in `ipc/`.
- `JobRegistry` is a tokio-task actor (mpsc/oneshot, `is_busy` query) owning **one sequential queue** shared by file transcribe, re-run, and re-transcribe; no `jobs` table, state is in-memory and doesn't survive restart. `transcribe_start` hashes the source first and returns an existing session on a match.
- UI state sync is snapshot + delta with a per-stream monotonic `seq`; the UI resubscribes on any gap and never treats local state as source of truth.
- `db/` is the sole SQLite (WAL) writer, accessed sequentially, forward-only migrations. A file job writes DB only at completion: work happens in `media/.staging/<job-id>/`, then session + transcript + proxy commit together in one transaction; a cancelled/failed job leaves no DB row.
- IDs are UUIDv7; `sessions.source_hash` (streaming SHA-256) is a unique dedup column; media lives at `media/<session-id>/<role>.<ext>`; no user/server string is ever used as a path segment.
- Data model: `sessions` 1–n `transcripts` (`variant: primary|retranscribe`, `status: complete|partial`); a gap is `segments.kind = gap` with `gap_reason ∈ {chunk_failed, disconnected}`, shared by file and live; a transcript is `partial` iff it has a `chunk_failed` gap.
- All Gemini calls go through one gateway with a shared key pool and error classifier; no feature builds requests or retries on its own; every request is refused before Consent is recorded; key priority is `Live > Job > Memo`. File-transcribe uses `generateContent` with inline FLAC and a JSON `responseSchema` of `{start, end, text}` segments, 120s/chunk timeout, no fan-out retry on timeout; the parser rescues valid segments from broken JSON and turns the rest into a gap.
- Errors crossing IPC are one shape, `AppError { category, code, detail_redacted }`, mapped only in `core/error`.
- `Segment.start/end` are absolute `f64` seconds (chunk offset added before saving); stored timestamps are epoch-ms UTC; display offset/`HH:MM:SS` formatting applies only at UI/export.
- Media pipeline: streaming decode to f32 mono → resample 16kHz mono → chunk + FLAC-encode under a ~14MB threshold; video demuxes only the first audio track. WebView reads media only via an asset protocol scoped to `$APPDATA/media/**`.
- Failures in proxy/memo/ads/remote never change a transcript's status or block a job/recording; a proxy that fails to publish still lets the transcript commit, flagged proxy-missing.

## UX & Interaction Patterns

- Badges (memo/audio/partial/recover/live/file/token) use fixed color pairs and `min-width`; memo/audio show only in Transcript detail, while "Thiếu N khoảng" and "Phục hồi" sit right after the name in Home rows.
- Transcript rows use a `56px | 1fr` grid with mono tabular timestamps; speaker is hidden. Gaps/disconnects render as inline rows in the segment stream at the right time position, not a banner, with "Chạy lại khoảng này" for `chunk_failed` gaps; a partial transcript also gets a banner listing every gap with "Chạy lại phần thiếu/toàn bộ", each badged "Tốn token Gemini".
- Player is 64px, uses the proxy; the playing segment gets an accent-soft background; user scroll stops auto-scroll-to-playing and offers a "back to playing line" control.
- Home has two states — empty (drop card + formats) vs. populated (thin persistent drop-zone above a virtualized, newest-first list, no infinite scroll); the running-job card is warning-styled at the top and collapses into the sidebar elsewhere.
- Search uses `⌘F`/`Ctrl+F`, `n/N` counter, cyclic prev/next, mark-color highlight. At 1024–1279px window widths, the 360px side panel closes before the transcript column shrinks.

## Cross-Story Dependencies

- 2.1 (media/proxy) and 2.2 (Gemini transcribe) are foundational; 2.3 (persistence, staging+publish) depends on both and underlies everything after it.
- 2.4 (job queue) depends on 2.2+2.3 and sets the `JobRegistry`/boot-routine pattern Epic 4 later extends. 2.5 (gaps/re-run) depends on 2.4 and shares its gap/partial model with live's `disconnected` gaps.
- 2.6 (settings) depends on Epic 1's Settings shell (1.9). 2.7 (transcript detail/player) depends on 2.4+2.5; 2.8 (file intake) depends on 2.7; 2.9 (Home) depends on 2.8; 2.10 (search/export) depends on 2.7.
- Also builds on Epic 1: repo scaffold (1.1), DB/migrations (1.2), key pool + Gemini gate (1.7). Downstream, Epic 3 builds its tag picker/session actions on the Home list from 2.9, and Epic 4 reuses the queue/actor and boot-routine conventions from 2.3/2.4.
