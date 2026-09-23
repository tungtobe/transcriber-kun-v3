---
title: '2.8 Nhận file — dialog, kéo thả, kiểm tra định dạng, phát hiện trùng'
type: 'feature'
created: '2026-09-24'
status: 'done'
baseline_commit: '33510c2b038fd65e859cce587af91c1f9c1f5b57'
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

**Problem:** `transcribe_start(path)` đã có nhưng người dùng chưa có cách nào đưa file vào app: nút "Chọn file" ở Home đang bị vô hiệu, kéo thả không được nhận, file sai định dạng/không có audio chỉ lộ lỗi khi Job đã chạy, và hai yêu cầu cùng nội dung khi Job chưa commit có thể gửi Gemini hai lần (FR-8, FR-9, FR-11).

**Approach:** Thêm lệnh Rust mở dialog chọn nhiều file (lọc đúng định dạng hỗ trợ) và nhận kéo thả toàn cửa sổ qua sự kiện drag-drop của webview Tauri; mọi đường vào dồn về một hàng xử lý tuần tự phía UI gọi `transcribe_start` từng file. `transcribe_start` kiểm định dạng/audio trước khi tạo Phiên, JobRegistry giữ reservation theo `source_hash` và trả Job hiện có, Job kiểm lại hash nguồn trước decode và trước commit.

## Boundaries & Constraints

**Always:** Thứ tự gate trong `transcribe_start`: Consent → đuôi file thuộc allow-list (không I/O) → hash + tra `source_hash` trong DB (trùng → `Existing`) → tra reservation JobRegistry (trùng → `ExistingJob`) → probe (có track audio, có frame) → có key dùng được → tạo Job. Tạo Job trong actor là nguyên tử: nếu cùng lúc đã có Job Transcribe chờ/chạy cùng `source_hash` thì trả Job đó, không tạo Job thứ hai; reservation tự giải phóng khi Job rời registry (commit/huỷ/lỗi). File trùng/Job trùng không bao giờ gọi Gemini và không cần key. Job khi bắt đầu chạy (kể cả sau khi chờ trong hàng) hash lại nguồn trước probe/decode và hash lại lần nữa sau decode trước commit; thiếu file hoặc hash khác → Job `error`, không commit, không lưu Phiên. Nhiều file (dialog chọn nhiều hoặc thả nhiều) xử lý tuần tự theo thứ tự nhận; mỗi file có kết quả riêng (Job mới / mở Phiên có sẵn / mở Job đang có / lỗi) và file lỗi không chặn file sau. Sau batch, app điều hướng tới `/session/<id>` của kết quả thành công đầu tiên (Job mới, Job có sẵn, hoặc Phiên có sẵn); không có kết quả thành công thì ở lại màn hiện tại. Kết quả hiển thị tại màn đích và màn hiện tại qua một danh sách thông báo có thể đóng: Phiên có sẵn → info "Mở lại phiên có sẵn — không tốn token"; sai định dạng → banner category "Định dạng" với câu "Định dạng không hỗ trợ. Hãy chuyển sang mp4, m4a hoặc mp3."; không có audio/file rỗng → category "Định dạng" câu riêng "File không có audio phát được."; thiếu key → warning có lối tắt `/settings/gemini`; lỗi khác → `errorTitle`/`errorHint`. Tên file hiển thị là basename. Kéo thả nhận ở **bất kỳ đâu** khi đang ở `/home` hoặc `/session/:id`: khi kéo vào hiện overlay drop-zone toàn cửa sổ (viền `1px dashed accent-border`, nền `accent-soft`, radius `2xl`), rời/thả thì ẩn. Home: nút "Chọn file" ở header và nút trong card file mở dialog; khi chưa có key dùng được thì cả hai vô hiệu kèm tooltip (`DisabledHint`) và banner lối tắt Settings có sẵn — thả file vẫn được nhận (file trùng mở Phiên, file mới báo thiếu key). `JobProgress` hiện "N file đang chờ" khi có Job Transcribe `queued`. Mọi chuỗi qua i18n vi/en/ja; log không chứa path đầy đủ.

**Never:** Không thêm plugin Tauri, không thêm capability/permission mới ngoài `core:default` hiện có, không cấp fs/dialog cho frontend, không mở rộng asset scope. Không danh sách phiên/card job ở Home (2.9), không tìm/export (2.10), không Live. Không đổi định dạng Proxy, không thêm bảng `jobs`, không cho Job chạy song song. Không nhận file khi đang ở Onboarding/Settings.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Chọn file | Bấm "Chọn file", chọn 1 `.mp4` hợp lệ, có key | `Job` → điều hướng `/session/<sessionId>` (view Job) | Huỷ dialog → không làm gì |
| Thả 1 file | Thả `.m4a` trên Home/Transcript | Overlay active khi kéo; Job tạo; điều hướng | — |
| Thả nhiều | 3 file hợp lệ | 3 Job xếp hàng tuần tự; điều hướng Job đầu; JobProgress "2 file đang chờ" | — |
| Sai định dạng | `.avi/.wmv/.flv/.ts` | Banner "Định dạng" + câu gợi ý; không Phiên, không Job, không hash | — |
| Không audio / rỗng | `.mp4` không track audio, file 0 byte | Banner "Định dạng" "File không có audio phát được."; không Job | — |
| Trùng Phiên | `source_hash` khớp Phiên đã có | `Existing{sessionId}`; mở Phiên; info "Mở lại phiên có sẵn — không tốn token" | Không cần key, không Gemini |
| Trùng Job | Cùng nội dung đang chờ/chạy, chưa có Phiên | `ExistingJob{jobId, sessionId}`; mở view Job đó | Hai lời gọi đồng thời → đúng một Job |
| Job huỷ/lỗi rồi thả lại | Reservation đã giải phóng | Job mới được tạo | — |
| Thiếu key | Không key dùng được, file mới hợp lệ | Không Job; warning thiếu key + lối tắt Settings | File trùng vẫn mở Phiên |
| Batch hỗn hợp | hợp lệ + `.wmv` + trùng + rỗng | Mỗi file một thông báo riêng; file hợp lệ vẫn tạo Job | Lỗi một file không dừng batch |
| Nguồn đổi/mất | File bị xoá hoặc sửa trước khi Job chạy/decode xong | Job `error` (không commit, không Phiên, không lưu hash sai) | Staging bị dọn |
| Ngoài phạm vi | Thả khi ở `/settings/*` hoặc `/onboarding` | Bỏ qua, không overlay | — |

</frozen-after-approval>

## Code Map

- `src-tauri/src/ipc/mod.rs` -- `decide_transcribe_start` (gate thuần, closure tiêm cho test) + `transcribe_start_inner` + `TranscribeStartOutcome` (thêm `ExistingJob`); mẫu dialog `save_diagnostics_bundle`/`pick_source_file` (`rfd` qua `app.run_on_main_thread` + `std::sync::mpsc`, chờ trong `spawn_blocking`) → thêm `transcribe_pick_files() -> Vec<String>` dùng `rfd::FileDialog::add_filter(..).pick_files()`; `specta_builder()` `collect_commands!`; test `export_bindings` tái sinh `src/lib/bindings.ts`. Tests gate hiện có ở ~dòng 1317–1423 là khuôn cho test mới.
- `src-tauri/src/transcribe/registry.rs` -- `StartParams.source_hash`, `Command::Start`/`handle_start` (tạo `JobEntry`, đẩy `order`), `JobEntry` (thêm `source_hash: Option<String>` cho Job Transcribe), `handle_finished`/`handle_cancel` (xoá entry = giải phóng reservation). Đổi `JobRegistryHandle::start` trả `StartOutcome { Started{job_id,session_id} | Existing{job_id,session_id} }`; thêm truy vấn `find_transcribe_by_hash(hash) -> Option<(JobId, SessionId)>`. `run_job_inner` (~dòng 792): thêm kiểm hash nguồn đầu job và sau `decode_and_transcribe` trước `commit_file_session`. Test mẫu: `a_second_start_is_queued_until_the_first_job_leaves_the_registry`.
- `src-tauri/src/media/{probe,hash,mod}.rs` -- `supported_extension` (allow-list, `pub(super)` → cần mở cho ipc qua hàm `pub`), `probe` (lỗi "No playable audio track/frames"), `format_error`/`SUGGESTED_FORMATS`, `sha256_file`. `core/error.rs` `Code` → thêm `NoAudio` (category `Format`) cho lỗi không track/không frame/file rỗng; giữ `Format` cho đuôi không hỗ trợ.
- `src/lib/stores/jobs.svelte.ts` -- `start(path)` (giữ), `jobs` Map (đếm `queued`). `src/lib/errors.ts` -- `errorTitle/errorHint`; lưu ý `error.format.hint` hiện nói về API key nên thông báo nhận file cần khoá i18n riêng.
- `src/routes/Home.svelte` -- card file đang dùng `DisabledHint` placeholder; `keysStore.hasUsableKey`/`status`; `BannerStack` banner thiếu key. `src/components/AppShell.svelte` -- `location()`; nơi gắn overlay kéo thả toàn cửa sổ. `src/routes/Session.svelte` + `session/JobProgress.svelte` -- view `job`, điều hướng `/session/:id` đã phân giải Job/Phiên qua `librarySessionGet`.
- `@tauri-apps/api/webview` `getCurrentWebview().onDragDropEvent(e => e.payload.type: 'enter'|'over'|'drop'|'leave', paths)` -- chỉ cần `core:event` có trong `core:default`; bọc trong module riêng để mock trong Vitest.
- DESIGN.md `drop-zone` (dòng 274–278), EXPERIENCE.md Drop-zone (dòng 95), "Minh bạch chi phí" (dòng 181).

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/core/error.rs`, `src-tauri/src/media/{probe,mod}.rs` -- thêm `Code::NoAudio` (→ `Format`), dùng cho lỗi không track audio/không frame/file rỗng; xuất `pub fn check_supported_extension(path)` -- tách lỗi "không audio" khỏi "sai định dạng" để UI nói đúng câu.
- [x] `src-tauri/src/transcribe/registry.rs` -- reservation theo `source_hash` trong actor (`StartOutcome`, `find_transcribe_by_hash`), kiểm hash nguồn đầu job + trước commit; tests: hai `start` cùng hash (kể cả đồng thời) → một Job; reservation giải phóng sau cancel/error/commit; nguồn bị xoá/sửa → `error`, không Phiên, staging sạch.
- [x] `src-tauri/src/ipc/mod.rs` -- gate mới trong `decide_transcribe_start` (extension → hash → lookup DB → reservation → probe → key → start), `TranscribeStartOutcome::ExistingJob`, lệnh `transcribe_pick_files`; tests gate: sai đuôi không hash, trùng Job không probe/không key, không audio không start, thiếu key với file trùng vẫn `Existing`; tái sinh `bindings.ts`.
- [x] `src/lib/dragdrop.ts` -- bọc `onDragDropEvent` (trả unlisten) -- seam mock.
- [x] `src/lib/stores/intake.svelte.ts` (+test) -- `pick()`, `submit(paths)` tuần tự (chuỗi hoá các lần gọi chồng), map kết quả → `notices` (id, variant, tiêu đề, câu, lối tắt), `dismiss(id)`, điều hướng tới kết quả thành công đầu tiên.
- [x] `src/components/DropOverlay.svelte` (+test), `src/components/IntakeNotices.svelte` (+test), `src/components/AppShell.svelte` -- overlay chỉ hoạt động ở `/home` và `/session/:id`; notices render trên Home và Session.
- [x] `src/routes/Home.svelte` (+test) -- nút "Chọn file" ở header + card file mở dialog; vô hiệu khi chưa có key dùng được; mô tả card liệt kê định dạng hỗ trợ.
- [x] `src/routes/session/JobProgress.svelte` (+test) -- "N file đang chờ".
- [x] i18n vi/en/ja -- nút, overlay, các thông báo nhận file, "Mở lại phiên có sẵn — không tốn token", "N file đang chờ".

**Acceptance Criteria:**
- Given hai lời gọi `transcribe_start` đồng thời cùng nội dung mới, when cả hai hoàn tất, then đúng một Job tồn tại và cả hai trả cùng `jobId`.
- Given thả 3 file (một `.wmv`, một trùng Phiên, một hợp lệ), when xử lý xong, then có 3 thông báo riêng, 1 Job mới, và app mở Phiên có sẵn (kết quả thành công đầu tiên theo thứ tự).
- Given `cargo test`, `cargo fmt --check`, `npm test`, `npm run check`, `npm run check:i18n`, `npm run check:ui`, when chạy, then pass và `bindings.ts` khớp bản sinh.

## Implementation Notes

- Backend: `Code::NoAudio` thêm vào `core/error.rs` (category vẫn `Format`, serialize `"noaudio"`); `media::probe`/`open_media`/`make_decoder` dùng `no_audio_error` cho "không track"/"không frame", giữ `format_error` cho sai đuôi và lỗi decode giữa luồng; `media::decode_mono_16khz` (pipeline full decode, không phải gate intake) giữ nguyên `format_error` -- ngoài phạm vi spec này. `SUPPORTED_EXTENSIONS` đổi `pub` và re-export qua `media::` để `ipc::transcribe_pick_files` dùng chung filter với gate thật.
- `transcribe::registry`: `StartOutcome::{Started,Existing}` thay cho `(JobId, SessionId)` cũ; `handle_start` tự kiểm `find_transcribe_job_by_hash` trước khi tạo entry (nguyên tử vì actor xử lý command tuần tự, không `.await` giữa kiểm và tạo); `JobEntry.source_hash` mới; `run_job_inner` gọi `verify_source_hash` hai lần (đầu job trước `probe`, và sau `decode_and_transcribe` trước commit). Test fixture `wav_fixture` đổi để nội dung phụ thuộc `name` (seed vào pha sine) vì `source_hash` giờ phải là hash thật của file -- hai fixture khác tên nhưng cùng nội dung cũ sẽ đụng reservation.
- `ipc::decide_transcribe_start` thêm gate `check_extension` (đồng bộ, không I/O) và `lookup_existing_job`/`probe_media` theo đúng thứ tự spec; `transcribe_pick_files` dùng `rfd::FileDialog::add_filter(..).pick_files()` cùng kỹ thuật main-thread/`std::sync::mpsc` như `pick_source_file`/`save_diagnostics_bundle` sẵn có -- không thêm plugin/capability.
- Frontend: `intake.svelte.ts` chuỗi hoá batch qua một `Promise` chain (`chain = chain.then(...)`), map từng `TranscribeStartOutcome`/lỗi thành một `IntakeNotice` (title = basename, không bao giờ full path), điều hướng bằng `push` từ `@keenmate/svelte-spa-router` tới kết quả thành công đầu tiên. Câu "Mở lại phiên có sẵn — không tốn token", "Định dạng không hỗ trợ. Hãy chuyển sang mp4, m4a hoặc mp3.", "File không có audio phát được." lấy đúng nguyên văn từ spec; câu cho `Job` mới ("Đã thêm vào hàng đợi transcribe.") và `ExistingJob` ("Đang mở tác vụ transcribe đang chạy cho file này.") là copy tự soạn vì spec không cho nguyên văn hai trường hợp này -- có thể cần review UX sau.
- `AppShell.svelte` đăng ký đúng một `onDragDropEvent` listener toàn app; `dropEnabled` (route `/home`, `/`, hoặc `/session/:id`) đọc lại mỗi lần sự kiện tới nên luôn đúng route hiện tại, không phải route lúc đăng ký.
- `JobProgress.svelte`: "N file đang chờ" đếm mọi Job `kind: 'transcribe'` + `state: 'queued'` trong `jobsStore.jobs` (không lọc theo Job đang xem), bỏ qua Job Chạy lại.
- Sửa ở bước kiểm diff (agent chính): file 0 byte trước đó rơi vào lỗi probe chung `Format` ("sai định dạng"); `media::probe::open_media` nay trả `Code::NoAudio` khi file rỗng, kèm test `zero_byte_file_is_a_no_audio_error_not_wrong_format` (I/O Matrix "Không audio / rỗng").

## Spec Change Log

## Review Triage Log

## Design Notes

Kéo thả dùng sự kiện drag-drop native của webview (Tauri v2 mặc định `dragDropEnabled: true` nên HTML5 `drop` không có path) — nhận path tuyệt đối ở frontend là chấp nhận được vì `transcribe_start(path: String)` đã là hợp đồng từ 2.4. Quyền đọc trong sandbox: file do người dùng chọn/thả đọc được suốt phiên chạy app, nên giữ path cho file chờ là đủ; nếu nguồn biến mất/đổi, kiểm hash đầu job và trước commit bắt được. Probe đặt sau tra trùng để file trùng không tốn decode.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml --locked` -- expected: pass
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` -- expected: sạch
- `npm test && npm run check && npm run check:i18n && npm run check:ui` -- expected: pass

**Manual checks:**
- Kéo thả thật trên macOS (WKWebView) và Windows (WebView2): overlay hiện khi kéo, thả nhiều file xếp hàng đúng.
