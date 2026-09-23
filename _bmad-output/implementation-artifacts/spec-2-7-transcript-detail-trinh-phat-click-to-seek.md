---
title: '2.7 Transcript detail — hiển thị, trình phát và click-to-seek'
type: 'feature'
created: '2026-09-23'
status: 'done'
baseline_commit: 'cf5ca1d989624ace4e9c60ab62b86047000d2c38'
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

**Problem:** `/session/:id` hiện chỉ có tiến độ Job và tóm tắt Phiên; người dùng không đọc được Segment, không nghe lại đúng đoạn, không thấy vị trí từng khoảng thiếu và không có cách phục hồi khi Proxy mất (FR-10, FR-15, FR-32, FR-34).

**Approach:** Thêm IPC đọc chi tiết Phiên (meta + Transcript `primary` + Segment + đường dẫn Proxy nếu dùng được) và IPC "Chọn lại file nguồn" (dialog `rfd` phía Rust, kiểm `source_hash`, dựng lại Proxy). Viết lại màn Transcript detail: header + meta + badge, banner partial với các nút Chạy lại, grid Segment `56px | 1fr` có dòng khoảng thiếu nằm đúng vị trí, trình phát 64 px dùng `<audio>` trên asset protocol với click-to-seek/highlight/tự cuộn, panel phụ 360 px, và panel log cho Job đang chạy.

## Boundaries & Constraints

**Always:** Timestamp hiển thị qua `src/lib/time.ts` (`displayTimestamp`, mono tabular, đã cộng offset); click Segment seek tới `start_sec` gốc; highlight so `currentTime` gốc với `[start, end)`; Segment đang phát có nền `accent-soft` + `aria-current="true"`. Speaker không hiển thị. Không tự phát audio (`preload="metadata"`). Thanh seek `role="slider"` có `aria-valuemin/max/now` và `aria-valuetext`, ←/→ ±5 s; Space play/pause khi focus nằm trong danh sách Segment (không nuốt Space trong input/nút). Có chỉnh tốc độ (0.75/1/1.25/1.5/2) và âm lượng. Cuộn tay (wheel/touch/phím cuộn) khi đang phát dừng tự cuộn ngay và hiện nút "Xuống dòng đang phát"; bấm nút → cuộn tới dòng đang phát và bật lại tự cuộn. Proxy lỗi/thiếu (DB `proxy_ext` NULL, file mất, hoặc `<audio>` phát `error`) → Transcript vẫn hiển thị đầy đủ, thanh phát thay bằng "Không có audio · Chọn lại file nguồn". Chọn lại: Phiên file hash file chọn, chỉ chấp nhận khi khớp `source_hash`, dựng Proxy trong staging rồi publish thay Proxy cũ và cập nhật `proxy_ext`; sai hash/huỷ dialog/lỗi → báo tại chỗ, Transcript giữ nguyên. Phiên live (không `source_hash`): không nhận file ngoài, báo rõ chưa có Recording để tái tạo. Badge dùng một component 7 biến thể (memo, audio, partial, recover, live, file, token) đúng cặp token DESIGN.md, cao 20 px, radius 6 px, 11px/600, `min-width` (không `width` cố định). Banner partial liệt kê mọi khoảng `chunk_failed` dạng `mm:ss–mm:ss` (qua `displayTimestamp`) + "Chạy lại phần thiếu" (`missing`) và "Chạy lại toàn bộ" (`all`), mỗi nút kèm badge token "Tốn token Gemini"; mỗi dòng gap `chunk_failed` nền warning trong luồng có "Chạy lại khoảng này" (`gap(idx)`). Gap `disconnected` là dòng trung tính không có nút. Chạy lại/Job hiện có → chuyển sang view Job; Job commit → tự tải lại view Phiên. Mọi chuỗi qua i18n vi/en/ja; log không chứa path đầy đủ hay transcript.

**Never:** Không thêm plugin Tauri, không cấp capability dialog/fs cho frontend, không mở rộng asset scope ngoài `$APPDATA/media/**`. Không tìm/export/copy (2.10), không Home/danh sách (2.9), không nhận file mới (2.8), không Memo/Ghi chú/segmented chọn bản (Epic 3/5, live side-by-side). Không đổi định dạng Proxy (S8 còn mở). Không ghép file bất kỳ vào Phiên live.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Mở Phiên | Phiên file complete, Proxy có | Header (quay lại, tên, badge FILE, ngày địa phương, thời lượng, số Segment text), grid Segment, trình phát | — |
| Offset | Offset 3600, Segment 65 s | Hiển thị `01:01:05`; click seek `currentTime = 65` | — |
| Phát | `currentTime` trong `[start,end)` Segment 3 | Segment 3 nền accent-soft + `aria-current` | — |
| Phím | Focus slider, → / ← | `currentTime` ±5 s (kẹp 0..duration) | — |
| Space | Focus trong danh sách Segment | Play/pause, không cuộn trang | Focus trong input/nút → hành vi mặc định |
| Cuộn tay | Đang phát, người dùng wheel | Ngừng tự cuộn, hiện "Xuống dòng đang phát" | Bấm nút → cuộn lại, tự cuộn bật |
| Partial | 2 gap `chunk_failed` | Banner liệt kê 2 khoảng + 2 nút có badge token; 2 dòng gap trong luồng có nút riêng | Lỗi IPC Chạy lại → thông điệp i18n |
| Proxy thiếu | `proxy_ext` NULL / file mất | Transcript đầy đủ; "Không có audio · Chọn lại file nguồn" | — |
| Proxy hỏng | `<audio>` phát `error` | Như Proxy thiếu | — |
| Chọn lại khớp | File hash = `source_hash` | Proxy mới publish, `proxy_ext` cập nhật, trình phát dùng được | — |
| Chọn lại sai | Hash khác / huỷ dialog | Không đổi gì; sai hash báo tại chỗ; huỷ im lặng | — |
| Phiên live thiếu Proxy | `kind = live` | Báo không có Recording để tái tạo, không mở dialog | — |
| Job đang chạy | `/session/:id` của Job | Tiến độ phút audio, nút Huỷ, panel log mở/đóng liệt kê diễn biến (state, Chunk, key, lần thử, chờ quota) | Job commit → tự hiển thị Phiên |
| Cửa sổ hẹp | 1024–1279 px | Panel phụ 360 px ẩn, Transcript nhận phần dư | ≥ 1280 px hiện panel |
| Không còn | Id không tồn tại | "Tác vụ không còn" + link Home (giữ 2.4) | — |

</frozen-after-approval>

## Code Map

- `src-tauri/src/ipc/mod.rs` -- `library_session_get_inner` + `SessionLookup` (2.4/2.5; giữ nguyên cho điều hướng Job/Phiên), `transcribe_rerun`, mẫu dialog `save_diagnostics_bundle` (`rfd` qua `app.run_on_main_thread` + `std::sync::mpsc`, chờ trong `spawn_blocking`), `blocking(...)`, `track_ipc_error`, `AppState.data_dir`, `specta_builder()` `collect_commands!`; test `export_bindings` tái sinh `src/lib/bindings.ts`. Thêm `library_session_detail(session_id)` và `library_proxy_relink(session_id)`.
- `src-tauri/src/library/store.rs` -- `publish_proxy(root, sid, staged)` (fsync → rename vào `paths::proxy_path`), `SessionSummary`/`get`, `discard_staging`, fault injection. Thêm hàm đọc detail và `relink_proxy` (tạo Proxy trong staging theo `JobId` mới → publish → `sessions::set_proxy_ext`, dọn staging mọi nhánh).
- `src-tauri/src/db/repo/{sessions,transcripts,segments}.rs` -- `sessions::get` (`kind`, `source_hash`, `proxy_ext`, `recovered`, `created_at`, `duration_sec`), `set_proxy_ext`; `transcripts::primary_for_session`/`get`; `segments::list_for_transcript` (`idx`, kind, gap_reason, text).
- `src-tauri/src/media/{hash,proxy}.rs` -- `sha256_file`, `create_proxy(dir, src)`; `core/paths.rs` `proxy_path`, `staging_dir`.
- `src-tauri/tauri.conf.json` -- asset protocol đã bật, scope `$APPDATA/media/**` (không đổi).
- `src/routes/Session.svelte` (+`Session.test.ts`) -- view `loading|job|saved|notFound|error`, subscribe `jobsStore`, auto-reload khi Job rời registry, `handleCancel`, rerun `missing` (2.5). Tách thành component con thay vì phình một file.
- `src/lib/stores/jobs.svelte.ts` -- `jobs` Map, `rerun(sessionId, transcriptId, scope)`, event `updated|result|error|cancelled`; panel log tích luỹ từ các event của Job đang xem.
- `src/lib/time.ts` -- `displayTimestamp`/`formatTimestamp` (2.6). `@tauri-apps/api/core` `convertFileSrc` cho `src` của `<audio>`.
- `src/styles/tokens.css` -- đã có `--color-{accent,warning,info,recover,danger,memo,token}-soft`, `--text-badge-size`; `npm run check:ui` kiểm tương phản AA + allow-list elevation. `src/components/icons.ts` (Lucide), `Banner.svelte`.
- DESIGN.md (`_bmad-output/planning-artifacts/ux-designs/ux-transcriber_kun-2026-09-18/DESIGN.md` dòng 199–225 badge, 330/468 player, 397–401 layout) -- chỉ tham chiếu khi cần giá trị token.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/library/store.rs`, `db/repo/*` -- `SessionDetail { session_id, kind, title, created_at, duration_sec, recovered, source_name, proxy_path: Option<PathBuf> (chỉ khi `proxy_ext` có và file tồn tại), transcript: Option<{ id, variant, status, model, language, segments: [{ idx, start_sec, end_sec, kind, gap_reason, text }] }> }`; `relink_proxy(db, root, session_id, picked_path)` → `Relinked | HashMismatch | NotFileSession`; tests: detail có/không Proxy, relink khớp/sai hash/live, lỗi giữa chừng không để lại staging và giữ Proxy cũ.
- [x] `src-tauri/src/ipc/mod.rs` -- `library_session_detail(session_id) -> Option<SessionDetail>` và `library_proxy_relink(session_id) -> ProxyRelinkOutcome { relinked | hashMismatch | cancelled | liveUnsupported }` (dialog `rfd` mở file trên main thread; huỷ → `cancelled`); gate tách hàm test được; tái sinh `bindings.ts`.
- [x] `src/components/Badge.svelte` (+test) -- 7 biến thể theo token DESIGN.md, `min-width`, icon tuỳ chọn 14 px.
- [x] `src/routes/session/SessionHeader.svelte`, `PartialBanner.svelte`, `SegmentList.svelte`, `Player.svelte`, `JobProgress.svelte` (+tests) và `src/routes/Session.svelte` -- dựng detail theo Boundaries; grid `56px | 1fr`; player 64 px (nút tròn 40 px, thời gian `HH:MM:SS / HH:MM:SS`, slider, tốc độ, âm lượng); layout có aside 360 px ẩn dưới 1280 px (nội dung: nguồn, model, ngôn ngữ của Transcript); JobProgress giữ nội dung 2.4 + panel log `<details>`.
- [x] `src/lib/stores/jobs.svelte.ts` (+test) -- lưu log diễn biến ngắn theo `jobId` (tối đa ~200 dòng, không chứa path/transcript).
- [x] i18n vi/en/ja -- header, meta, badge, banner, nút Chạy lại, "Tốn token Gemini", trình phát (aria-label, tốc độ, âm lượng), "Xuống dòng đang phát", "Không có audio · Chọn lại file nguồn", kết quả relink, panel log.
- [x] Tests Vitest phủ I/O Matrix (mock `commands`, `HTMLMediaElement` play/pause/currentTime, sự kiện `timeupdate`/`error`/`wheel`) + Rust tests.

**Acceptance Criteria:**
- Given Phiên có Segment 0–10 s, 10–20 s và offset 0, when `timeupdate` với `currentTime = 12`, then chỉ Segment thứ hai có `aria-current="true"` và class nền accent-soft.
- Given Phiên partial, when bấm "Chạy lại khoảng này" trên dòng gap idx 3, then `transcribe_rerun` được gọi với `{ kind: 'gap', gapId: 3 }` và view chuyển sang tiến độ Job.
- Given `cargo test`, `cargo fmt --check`, `npm test`, `npm run check`, `npm run check:i18n`, `npm run check:ui`, when chạy, then pass và `bindings.ts` khớp bản sinh.

## Implementation Notes

Triển khai đúng theo Code Map, không lệch thiết kế. Điểm đáng chú ý:
- `library::store::get_detail`/`relink_proxy` là hai hàm mới duy nhất chạm Proxy/DB cho story này; `relink_proxy` tái dùng nguyên `publish_proxy` (kể cả điểm tiêm lỗi `fault::Point::PublishWrite/PublishRename` có sẵn) nên staging/lỗi giữa chừng hành xử giống hệt `commit_file_session` -- không có đường ghi Proxy thứ hai nào trong codebase.
- `SessionDetail`/`TranscriptDetail`/`SegmentDetail` là kiểu specta sống thẳng trong `library/store.rs` (không qua một wrapper `ipc::` riêng như `SessionLookup`/`SessionSummary`) vì Task đặt tên `SessionDetail` là chính kiểu trả về của lệnh IPC. `created_at` dùng `f64` (không phải `i64`) và `SegmentDetail.idx` dùng `i32` (không phải `i64`) vì specta-typescript cấm xuất kiểu BigInt -- cùng lý do `RerunScope::Gap::gap_id` đã dùng `i32` trước đó; `proxy_path` là `String` (không phải `PathBuf`) để khớp quy ước "đường dẫn qua IPC luôn là chuỗi" đã có (`transcribe_start(path: String)`).
- `decide_proxy_relink` (ipc/mod.rs) mirror đúng khuôn `decide_transcribe_start`/`decide_transcribe_rerun`: tra Phiên → chặn Phiên không phải `file` (live) *trước khi* mở dialog → mở dialog (huỷ → `Cancelled`) → so hash/publish. `pick_source_file` dùng lại kỹ thuật `run_on_main_thread` + kênh `std::sync::mpsc` của `save_diagnostics_bundle` (2.6), chỉ đổi `save_file()` thành `pick_file()`.
- Tự cuộn ở `SegmentList.svelte` **không** cần cờ "cuộn do code" như Design Notes gợi ý: vì chỉ nghe `wheel`/`touchmove`/phím cuộn (không nghe `scroll` chung), một `scrollIntoView` do code gọi không bao giờ tự kích các sự kiện đó -- bớt được một nguồn race/cờ hết hạn so với phương án ban đầu, hành vi quan sát được giống hệt.
- `Player.svelte` sở hữu `<audio>` thật; `SegmentList`/`Session.svelte` không đụng DOM audio trực tiếp -- gọi qua `bind:this` tới các hàm `seek`/`toggle` được `export` từ `Player`, và đọc `currentTime`/`duration`/`playing` qua `$bindable` hai chiều. `Session.svelte` là nơi duy nhất giữ các state đó vì cả `Player` lẫn `SegmentList` đều cần đọc/ghi.
- `jobsStore.logFor(jobId)` trả `JobLogEntry[]` có cấu trúc (snapshot/`error` thô, không phải chuỗi đã định dạng) -- `JobProgress.svelte` tự dịch qua i18n lúc hiển thị, tái dùng đúng các khoá `session.job.state*/chunkLabel/keyLabel/attemptLabel/waitingQuota` đã có thay vì một bộ chuỗi log riêng.
- Badge 7 biến thể dựng đủ trong `Badge.svelte`, nhưng màn `/session/:id` chỉ thực sự render `file` (luôn), `recover` (khi `recovered`), `partial` (khi transcript `status = partial`, cạnh banner) và `token` (trên nút Chạy lại) -- `memo`/`live`/`audio` không có chỗ dùng hợp lệ trong phạm vi story này (Never: không Memo, không side-by-side Live) nên không render, chỉ tồn tại như biến thể CSS sẵn sàng cho các epic sau.
- `check:ui`'s shadow-allow-list resolves `var(--shadow-*)` only against tokens defined in the *same* file being scanned (not against `tokens.css`), so `SegmentList.svelte`'s floating "Xuống dòng đang phát" button uses the literal `0 10px 32px rgb(17 24 39 / 12%)` value, matching the existing convention in `CloseConfirm.svelte` rather than `var(--shadow-floating)`.
- `sessions.saved.*` (2.4's old placeholder summary strings) removed từ cả ba catalog i18n -- không còn nơi nào dùng sau khi `Session.svelte` chuyển hẳn sang `SessionHeader`/`SegmentList`/`Player`.

## Spec Change Log

## Review Triage Log

## Design Notes

Seek ≤ 500 ms với file 90 phút phụ thuộc S8 (ADR 2.1, chưa đo trên WKWebView/WebView2): story này dựng player đúng hành vi và `preload="metadata"`; phép đo thật là QA thủ công theo quy trình trong ADR. Tự cuộn: đánh dấu cuộn do code bằng cờ ngắn hạn rồi chỉ coi `wheel`/`touchmove`/phím cuộn là cuộn tay, tránh để `scrollIntoView` tự tắt tự cuộn. `SessionLookup` giữ để quyết định Job/Phiên; detail tải sau khi biết là Phiên.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml --locked` -- expected: pass
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` -- expected: sạch
- `npm test && npm run check && npm run check:i18n && npm run check:ui` -- expected: pass

**Manual checks:**
- S8/FR-32: seek ≤ 500 ms trên Proxy 90 phút ở WKWebView và WebView2 (quy trình ADR 2.1) — cần máy đích.
