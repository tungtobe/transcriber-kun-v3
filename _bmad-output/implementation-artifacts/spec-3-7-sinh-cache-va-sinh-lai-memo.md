---
title: '3.7 Sinh, cache và sinh lại Memo'
type: 'feature'
created: '2026-09-28'
status: 'done'
baseline_commit: '17a7896d33496b79dda6e4a12c6cbd9fe17cfa4c'
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

**Problem:** Có transcript, ghi chú và template nhưng chưa sinh được memo; người dùng cần chọn template, sinh memo một lần (cache), sinh lại khi muốn, rồi copy/tải về để gửi khách.

**Approach:** Bảng `memos` cache theo (Phiên, Template) kèm provenance; module `memo/generate` dựng prompt từ transcript primary + ghi chú và gọi cổng `gemini/` (lớp `Memo`, hạn 90 s, huỷ được); tab "Memo" trong aside Transcript detail render Markdown an toàn, có Copy/Tải `.md`, lỗi inline theo category.

## Boundaries & Constraints

**Always:**
- Bảng `memos(session_id FK→sessions ON DELETE CASCADE, template_id TEXT (không FK — memo sống sót khi template bị xoá), body, created_at, transcript_id, transcript_status, notes_revision NULL, template_name, template_prompt, model, PRIMARY KEY(session_id, template_id))`. Mở lại memo chỉ đọc DB, không gọi Gemini.
- `memo_generate(session_id, template_id, locale)`: chụp đầu vào lúc bắt đầu (primary transcript + segments, ghi chú + revision, template name/prompt, `memo_model` từ settings); gọi `GeminiGateway::post_job_observed` với `ModelKind::Memo` (Priority `Memo`, hạn 90 s sẵn có), body text-only `{"contents":[{"role":"user","parts":[{"text":prompt}]}]}`, trả text Markdown. Chỉ khi thành công **và** commit DB mới thay memo cũ của cặp; lỗi/huỷ/timeout giữ memo cũ; không bao giờ đổi transcript/ghi chú/trạng thái Phiên.
- Mỗi cặp (Phiên, Template) tối đa một request đang chạy (registry trong `AppState`, chỉ `ipc/` điều phối): gọi trùng trả `AlreadyRunning` không tạo request mới; `memo_cancel(session_id, template_id)` huỷ qua `CancellationToken`; kết quả tới sau khi đã huỷ/thay bị bỏ. Kết quả lệnh: `Generated(Memo) | Cancelled | AlreadyRunning`; lỗi trả `AppError` đúng category (quota/auth/network/timeout/model…).
- Guard: từ chối khi `is_wiping` hoặc Phiên `is_session_deleting`; xoá Phiên và xoá toàn bộ huỷ các request memo đang chạy của Phiên liên quan, và commit sau đó (FK lỗi/Phiên không còn) coi như `Cancelled`, không tái tạo dữ liệu.
- Prompt: thay `{transcript}` bằng transcript primary dạng dòng `[mm:ss] text` (dùng offset hiển thị như export 2.10); đoạn gap thành một dòng thông báo thiếu nội dung có khoảng thời gian (theo locale); nếu transcript `partial` thì chèn trước một câu liệt kê các khoảng thiếu. `{notes}` (nếu có trong template) thay bằng header theo locale ("Ghi chú của người dùng:" / "User notes:" / "ユーザーのメモ:") + nội dung, hoặc câu "(không có ghi chú)" theo locale khi rỗng. Chuỗi này sống trong Rust (`memo/`), locale `vi|en|ja`.
- `memo_get(session_id, template_id)` trả memo + cờ `fromPreviousTranscript` (= `transcript_id` ≠ primary hiện tại) và `notesChanged` (revision ghi chú hiện tại ≠ `notes_revision`). Đổi đầu vào chỉ làm memo "cũ", không tự gọi Gemini.
- Vô hiệu nút Sinh kèm lý do (mẫu `DisabledHint`): chưa Consent, không có key dùng được, không có transcript primary, hoặc transcript không có đoạn text nào (chỉ gap). Trước khi sinh, flush ghi chú của Phiên (`notesStore.flush`).
- Panel Memo = tab thứ ba "Memo" trong aside 360 px (cạnh Thông tin/Ghi chú): select Template (danh sách từ store 3.6 theo locale UI), nút "Sinh"/"Sinh lại" + badge `token` "Tốn token Gemini", trạng thái đang sinh + nút Huỷ, dòng nguồn "Sinh từ bản {primary|chạy lại, đầy đủ|thiếu} + ghi chú · hh:mm" (bỏ "+ ghi chú" khi template không dùng `{notes}`), nhãn "Memo sinh từ bản trước" khi `fromPreviousTranscript`, gợi ý "Ghi chú đã đổi sau khi sinh" khi `notesChanged`. Lỗi inline trong panel theo category (dùng `errorTitle`/`errorHint` sẵn có) kèm hành động Thử lại.
- Render Markdown bằng `marked` rồi `DOMPurify.sanitize` (không script/style/iframe/sự kiện inline); link: chặn click, chỉ mở URL `http(s)` qua lệnh Rust `open_external_url` dùng opener từ Rust (không nới capability frontend). Nút Copy (`navigator.clipboard.writeText` body Markdown gốc) và "Tải .md" qua lệnh `memo_export` dùng đúng mẫu hộp thoại lưu `rfd` của `save_transcript_export` (Rust ghi file, tên file mặc định từ tên Phiên đã làm sạch + `.md`, không đưa đường dẫn về WebView).
- Khi request hoàn tất mà người dùng không còn thấy panel Memo của Phiên đó (đổi tab, rời route), hiện toast ngắn "Memo đã sẵn sàng" (hoặc lỗi) ở cấp app; store memo là singleton nên promise vẫn được xử lý sau khi unmount.
- Badge `memo` trong meta `SessionHeader` khi Phiên có ít nhất một memo (`SessionDetail.hasMemo`).
- Chuỗi UI mới đủ vi/en/ja; nội dung memo/prompt/ghi chú không vào log (`Sensitive<String>`).

**Never:** Không đưa memo vào hàng đợi Job. Không tự sinh memo khi mở panel hay khi đầu vào đổi. Không dùng `{@html}` với HTML chưa sanitize. Không nới `opener:allow-open-url` trong capability. Không thêm plugin clipboard/dialog/fs. Không dùng tên Phiên/Template làm đường dẫn trong Container.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Sinh lần đầu | transcript complete, template có `{notes}`, ghi chú rev 3 | `Generated`; lưu memo với transcript_id, notes_revision 3, snapshot template, model | — |
| Mở lại | memo đã cache | Hiện memo, 0 request Gemini | — |
| Sinh lại lỗi | memo cũ tồn tại, Gemini 429 | Memo cũ giữ nguyên; lỗi `quota` inline | category `quota` |
| Trùng request | đang chạy cặp (S,T), gọi lại | `AlreadyRunning`, 1 request duy nhất | — |
| Huỷ | đang chạy, `memo_cancel` | `Cancelled`, memo cũ giữ | — |
| Kết quả muộn | xoá Phiên trong lúc sinh | Không có dòng `memos`, không lỗi dữ liệu | — |
| Partial | transcript có gap 05:00–06:00 | Prompt liệt kê khoảng thiếu; dòng nguồn ghi "thiếu" | — |
| Chỉ gap | mọi segment là gap | Nút Sinh vô hiệu + lý do; Rust từ chối | `Request` |
| Chạy lại transcript | memo sinh từ transcript cũ | `fromPreviousTranscript = true`, nhãn hiện | — |
| Template bị xoá | memo đã sinh từ template T | Memo vẫn đọc được với tên snapshot | — |
| XSS | body chứa `<script>`/`onerror=` | Không chạy; bị loại khi render | — |
| Consent/key | chưa Consent hoặc không key | Nút vô hiệu + lý do | Rust từ chối nếu vẫn gọi |

</frozen-after-approval>

## Code Map

- `src-tauri/src/db/migrations/mod.rs` -- `MIGRATIONS: [M; 6]`; thêm migration 7 `memos`, cập nhật mọi assert version (file này + `db/mod.rs`).
- `src-tauri/src/db/repo/memos.rs` (mới, trong `repo/mod.rs`) -- get, upsert, `exists_for_session`.
- `src-tauri/src/gemini/mod.rs` -- `post_job_observed` (637), `ModelKind::Memo` (107), `priority_for` (826), `CancellationToken` (130), `require_consent` (72), `classify_http_status` (861); fake `test_support` (963: `FakeTransport`, `gateway_with`). `gemini/keys.rs:57-69` `Priority::Memo` + `MEMO_DEADLINE` 90 s đã có — không thêm lớp mới.
- `src-tauri/src/transcribe/adapter.rs:93-160` -- mẫu dựng path/body và parse response `generateContent` (lấy text từ candidates); memo dùng body text-only.
- `src-tauri/src/settings/mod.rs:93` -- `memo_model` (mặc định `gemini-flash-lite-latest`); IPC transcribe đọc consent/settings thế nào thì memo làm y vậy.
- `src-tauri/src/memo/mod.rs` + `memo/generate.rs` (mới) -- chụp đầu vào, dựng prompt (chuỗi locale), gọi gateway, commit; `memo/templates.rs` thêm `get(db, id)` (repo `memo_templates::get` 119).
- `src-tauri/src/library/store.rs` -- `get_detail` (496) thêm `has_memo`; `get_export_data` (541) mẫu đọc segments + gap; `delete_session`/`wipe_all` cascade dọn `memos`. `repo::transcripts::primary_for_session`, `repo::segments::list_for_transcript` (111). `library/notes.rs:73` `get` → revision.
- `src-tauri/src/ipc/mod.rs` -- `AppState` (`ipc/boot.rs`) thêm registry memo đang chạy; lệnh `memo_generate`, `memo_get`, `memo_cancel`, `memo_export`, `open_external_url`; `save_transcript_export` (490) mẫu `rfd`; `library_session_delete_inner`/`library_wipe_all_inner` huỷ memo đang chạy; guard `is_wiping`/`is_session_deleting`; đăng ký `specta_builder`; `npm run bindings`.
- `package.json` -- thêm `marked`, `dompurify` (không nằm trong danh sách cấm của `scripts/check-forbidden-deps.mjs`).
- `src/routes/Session.svelte:69-72,525-561` -- `asideTab` union + `switchAsideTab`; thêm tab `memo` cùng mẫu (panel luôn mount, ẩn bằng CSS).
- `src/components/MemoPanel.svelte` (mới, +test), `src/lib/markdown.ts` (mới, +test: marked + DOMPurify), `src/lib/stores/memo.svelte.ts` (mới, +test, mẫu `createXStore` + mock `commands`).
- `src/routes/session/SessionHeader.svelte` + `src/components/Badge.svelte:10` -- biến thể `memo`/`token` đã có; thêm prop `hasMemo`.
- `src/lib/stores/settings.svelte.ts:547` `consentStatus`, `src/lib/stores/keys.svelte.ts:257-262` `status`/`hasUsableKey`, `DisabledHint` (cách dùng ở `Home.svelte:58,162`).
- Toast: không có component toast chung — tìm toast export trong `Session.svelte` (2.10) và nâng thành host cấp app nhỏ (vd `src/lib/stores/toast.svelte.ts` + `src/components/ToastHost.svelte` gắn ở `App.svelte`), hoặc tái dùng `BannerStack` nếu phù hợp.
- `src/lib/stores/memoTemplates.svelte.ts`, `src/lib/stores/notes.svelte.ts` (`flush`), `src/lib/time.ts` (`formatLocalTime`).

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/db/migrations/mod.rs`, `src-tauri/src/db/repo/{memos,mod}.rs` (+test) -- bảng, cascade, upsert.
- [x] `src-tauri/src/memo/{mod,generate,templates}.rs` (+test với `FakeTransport`) -- prompt (notes/partial/gap/locale), thành công commit, lỗi/huỷ giữ cũ, chỉ-gap bị từ chối, provenance & cờ stale.
- [x] `src-tauri/src/ipc/{boot,mod}.rs`, `src-tauri/src/library/store.rs` (+test) -- registry chống trùng/huỷ, guard, huỷ khi xoá/wipe, `has_memo`, export `.md`, `open_external_url` chỉ http(s); bindings.
- [x] `package.json`, `src/lib/markdown.ts` (+test XSS) -- marked + DOMPurify.
- [x] `src/lib/stores/memo.svelte.ts` (+test), toast host (+test) -- trạng thái per cặp, AlreadyRunning, huỷ, toast khi không nhìn panel.
- [x] `src/components/MemoPanel.svelte` (+test), `src/routes/Session.svelte`, `src/routes/session/SessionHeader.svelte` (+test) -- tab Memo, lý do vô hiệu, dòng nguồn, nhãn bản trước, Copy/Tải, badge.
- [x] `src/i18n/{vi,en,ja}.json` -- chuỗi mới.

**Acceptance Criteria:**
- Given Phiên có transcript, when mở tab Memo và bấm Sinh, then thấy trạng thái đang sinh, rồi memo Markdown đã sanitize kèm dòng nguồn và nút Copy/Tải .md.
- Given đang sinh, when chuyển sang tab khác hoặc rời Phiên, then khi xong có toast và mở lại tab Memo thấy memo đã cache.
- Given memo tồn tại, when đóng và mở lại app, then memo hiện ngay, không có request Gemini nào.

## Implementation Notes

- `GeminiGateway::post_job_observed` giữ nguyên chữ ký/hành vi cũ (`Priority::Job` + `TRANSCRIBE_CHUNK_TIMEOUT`, vẫn dùng bởi `transcribe::adapter`) -- thêm `post_job_observed_for(kind: ModelKind, ...)` bên dưới, tham số hoá qua `priority_for`/`deadline_for` mới (`ModelKind::Memo` → `Priority::Memo` + `MEMO_DEADLINE`). `memo::generate::run` gọi `post_job_observed_for(ModelKind::Memo, ...)` -- đúng tinh thần spec ("gọi `post_job_observed` với `ModelKind::Memo`") mà không phải sửa chữ ký của hàm cũ và mọi test/call site hiện có (`transcribe::adapter`, test trong `gemini/mod.rs`).
- Registry "đang sinh" hiện thực bằng `AppState::memo_running: Mutex<HashMap<(SessionId, MemoTemplateId), CancellationToken>>` -- vì luôn tối đa một token/cặp (gọi trùng trả `AlreadyRunning`, không tạo token thứ hai), không cần một "mã request hiện hành" riêng như Design Notes phác thảo: kiểm `cancellation.is_cancelled()` ngay sau khi `await` gateway xong (trước khi `repo::memos::upsert`) là đủ để loại kết quả muộn, vì không bao giờ có hai token cùng sống cho một cặp.
- `repo::memos::upsert` trả `UpsertOutcome::SessionGone` (bắt lỗi FK vi phạm) thay vì lỗi -- `memo::generate::run` ánh xạ sang `GenerateOutcome::Cancelled`, khớp spec "Phiên không còn ... coi như `Cancelled`".
- `SessionDetail::has_memo`/badge `memo` không tự tải lại `librarySessionDetail` sau khi sinh memo lần đầu -- `MemoPanel` gọi `onMemoAvailable` (prop) mỗi khi đang hiện một memo, `Session.svelte` cập nhật `view.detail.hasMemo` tại chỗ.
- Toast cấp app là store + component mới (`src/lib/stores/toast.svelte.ts` + `src/components/ToastHost.svelte`, gắn ở `App.svelte`) -- không tái dùng `BannerStack` (khác vị trí/positioning: fixed góc dưới-phải, tự động biến mất, không phải banner inline trong luồng nội dung).
- `src/components/MemoPanel.test.ts` (12 test, mock trực tiếp `memoStore`/`memoTemplatesStore`/`keysStore`/`notesStore`/`settingsStore` + `commands.{memoExport,openExternalUrl}`, cùng mẫu `NotesPanel.test.ts`): 4 lý do vô hiệu nút Sinh (consent/key/no-transcript/only-gaps, kèm xác nhận click không gọi `generate`), memo cache hiện ngay không gọi `generate`, nhãn "Memo sinh từ bản trước", XSS (`<script>`/`onerror=` không lọt vào DOM), lỗi `quota` inline giữ memo cũ + Thử lại gọi lại `generate`, và click link http(s) gọi `openExternalUrl` thay vì điều hướng (+ link `javascript:` không gọi lệnh gì). Không phát hiện bug thật nào từ bộ test này -- mọi assertion pass ngay từ lần chạy đầu, không cần sửa `MemoPanel.svelte`.

## Spec Change Log

## Review Triage Log

## Design Notes

Registry memo đang chạy nằm trong `AppState` (song song `deleting`/`wiping`), khoá bằng (Phiên, Template) và giữ `CancellationToken` + mã request; commit chỉ khi mã request vẫn là mã hiện hành, nên kết quả muộn không ghi đè. Không cần revision riêng cho transcript: chạy lại luôn tạo `TranscriptId` mới, so khớp id là đủ phát hiện "bản trước". Lệnh `memo_generate` await tới khi xong (≤ 90 s + chờ key) — frontend store singleton giữ promise nên rời màn không mất kết quả.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml --locked` -- pass
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check && cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` -- sạch
- `npm test` (exit code 0, không có "Unhandled Errors") `&& npm run check && npm run check:i18n && npm run check:ui && npm run check:deps` -- pass
