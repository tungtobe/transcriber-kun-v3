---
title: '2.4 Job transcribe file — hàng đợi, tiến độ và huỷ'
type: 'feature'
created: '2026-09-23'
status: 'done'
baseline_commit: 'd1f404faa5eab77bb2ac8d2db6cb7538c9bcbdd8'
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

**Problem:** Đã có media (2.1), adapter Gemini (2.2) và commit bền (2.3) nhưng chưa có gì nối chúng thành một Job: người dùng không thể đưa file vào, xem tiến độ thật, huỷ, rời màn hay đóng app an toàn.

**Approach:** Actor `JobRegistry` (tokio task, mpsc + oneshot) giữ một hàng đợi tuần tự trong bộ nhớ, chạy pipeline hash → Proxy staging → decode/chunk → Gemini tuần tự → merge → `commit_file_session`. IPC `transcribe_start`, `jobs_subscribe` (Snapshot + `JobEvent` có `seq`), `jobs_cancel`; route `/session/:id` tối thiểu và xác nhận đóng app khi có Job.

## Boundaries & Constraints

**Always:** Thứ tự gate của `transcribe_start`: Consent → hash + tra `source_hash` (trùng → `Existing { session_id }`, không Job, không Gemini) → có key dùng được → tạo Job. Chụp `transcribe_model`, ngôn ngữ (auto) và Chunk 5 phút lúc nhận Job. Tiến độ = ms audio đã xử lý / tổng. Huỷ: ngừng gửi Chunk mới ≤ 2 s, huỷ được lúc chờ quota/decode/hash, kết quả tới muộn không commit, dọn staging; cancel sau commit trả "đã hoàn tất". `seq: u32` đơn điệu cho cả registry; snapshot và đăng ký Channel trong cùng một lệnh actor. Registry cấp trước `session_id` dự kiến; commit dùng đúng id đó. Log không chứa path đầy đủ, transcript hay key.

**Never:** Không bảng `jobs`, không khôi phục Job sau restart, không chạy Job song song, không `Arc<Mutex>` bọc state registry. Không Chạy lại/`transcribe_rerun` và retry-policy mới (2.5), không Settings chunk/offset/ngôn ngữ (2.6), không nhận file bằng dialog/kéo thả hay card Job ở Home (2.8/2.9), không Transcript detail (2.7). Không thêm plugin Tauri.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Happy path | File hợp lệ, Consent + key | Job UUIDv7 + `session_id` dự kiến; progress theo phút; `result { session_id }`; Segment không lùi thời gian | — |
| Nhiều Job | 3 lần `transcribe_start` | Chạy tuần tự; Job chờ ở trạng thái `queued` | Huỷ Job đang chờ → bỏ khỏi hàng, Job khác tiếp tục |
| Trùng | Hash đã có trong `sessions` | `Existing { session_id }` | Không tạo Job |
| Chưa Consent / thiếu key | — | Không tạo Job | `AppError` (category `auth`) để UI dẫn tới Settings |
| Chờ quota | Mọi key đang nghỉ | Event `waitingQuota`, không báo lỗi | Hết ngân sách → xử lý như Chunk lỗi |
| Chunk lỗi | Lỗi không phải Auth/Model/Consent | Khoảng Chunk thành gap `chunk_failed`, Job tiếp tục, commit `partial` | Auth/Model/Consent/đọc nguồn/DB → `error`, không commit |
| Phần chưa cứu trong Chunk | `unresolved` từ parser | Thành gap `chunk_failed` | — |
| Huỷ | Lúc hash/decode/gửi/chờ quota | `cancelled` ≤ 2 s, không dòng DB, staging bị xoá | Cancel đến sau commit → trả kết quả hoàn tất |
| Remount UI | `jobs_subscribe` lại | Snapshot đủ Job hiện tại rồi event tiếp theo | Hụt `seq` → UI subscribe lại |
| Đóng app | Có Job chạy/chờ | Chặn đóng, UI hỏi xác nhận; đồng ý → huỷ sạch rồi thoát | Không Job → đóng ngay |
| `/session/:id` | Id của Job / Phiên đã lưu / không còn | Tiến độ Job / tên Phiên / "Tác vụ không còn" + link Home | — |

</frozen-after-approval>

## Code Map

- `src-tauri/src/media/{decode,chunk,hash,proxy,probe}.rs` -- API đồng bộ, streaming: `decode_mono_16khz(path, emit)`, `Chunker::new(ChunkOptions::default())` (300 s) + `push`/`finish`, `Chunk { start_ms, duration_ms, flac_base64, … }`, `sha256_file`, `create_proxy(dir, src)`, `probe` (tổng thời lượng). Chạy trong `tauri::async_runtime::spawn_blocking`, đẩy Chunk qua kênh bounded (≤ 2) sang vòng async.
- `src-tauri/src/transcribe/adapter.rs:109` -- `transcribe_chunk(gateway, model, chunk, consent, cancellation) -> Result<ChunkTranscript, TranscribeFailure>`; `parser.rs` trả `segments` (giây tuyệt đối) + `unresolved`. Chưa có merge liên Chunk.
- `src-tauri/src/gemini/mod.rs` -- `post_job` (deadline 120 s gồm chờ key, retry nội bộ), `CancellationToken`, `require_consent`, trait `GeminiTransport` (fake cho test). `keys.rs:181` `acquire_with_budget` chờ im lặng, `KeyLease.key_id` — chưa có tín hiệu chờ quota/số thứ tự key/số lần thử.
- `src-tauri/src/library/store.rs` -- `commit_file_session` tự sinh `SessionId`; `discard_staging`; `FileSessionDraft`/`SegmentDraft`/`GapReason::ChunkFailed`.
- `src-tauri/src/db/repo/sessions.rs` -- chưa có tra theo `source_hash`.
- `src-tauri/src/ipc/{mod,boot}.rs` -- `specta_builder()` `collect_commands!`/`collect_events!`, test `export_bindings` sinh `src/lib/bindings.ts`; `AppState` (db, key_pool, gateway); actor dùng `tauri::async_runtime::spawn`; `ipc/spike_channel.rs` mẫu `Channel<T>` (seq `u32`). Chưa có handler đóng cửa sổ (`lib.rs`).
- `src-tauri/src/settings/mod.rs` -- `Settings.transcribe_model`; đọc qua `settings::load(&db)`.
- Frontend: `src/lib/router.ts` (thêm `/session/:id`), store runes `src/lib/stores/app.svelte.ts` làm mẫu, i18n `src/i18n/{vi,en,ja}.json` + `npm run check:i18n`, Vitest + testing-library.
- Tokio không có `rt-multi-thread`/`fs`: I/O chặn chỉ qua `spawn_blocking`.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/gemini/{mod,keys}.rs` -- thêm observer tuỳ chọn cho `post_job` phát `WaitingQuota`, `KeyInUse { ordinal }` (1-based, không lộ key), `Attempt { n }`; `None` giữ hành vi cũ -- nguồn cho event tiến độ.
- [x] `src-tauri/src/transcribe/adapter.rs` -- chuyển observer xuyên qua `transcribe_chunk`.
- [x] `src-tauri/src/db/repo/sessions.rs`, `library/store.rs` -- `find_by_source_hash`, `get` tóm tắt Phiên; `commit_file_session` nhận `session_id` từ caller.
- [x] `src-tauri/src/transcribe/{job,registry,merge}.rs` -- `JobRegistry` actor (`start`, `cancel`, `subscribe`, `is_busy(session_id)`, `snapshot`), pipeline sau trait `ChunkTranscriber` để test bằng fake, merge Chunk → `SegmentDraft` đơn điệu + gap.
- [x] `src-tauri/src/ipc/{mod,boot}.rs`, `lib.rs` -- lệnh `transcribe_start`, `jobs_subscribe`, `jobs_cancel`, `library_session_get`, `app_close_confirm`; event `CloseRequested`; `JobRegistry` trong `AppState`; handler `CloseRequested` chặn khi registry bận; tái sinh `bindings.ts`.
- [x] `src/lib/stores/jobs.svelte.ts`, `src/routes/Session.svelte`, `src/lib/router.ts`, `src/components/CloseConfirm.svelte` (gắn ở `AppShell`), i18n 3 ngôn ngữ -- store bỏ `seq` đã áp dụng, subscribe lại khi hụt, dọn khi remount; trang hiển thị "32 / 90 phút · 36 %", Chunk hiện tại, key thứ mấy, số lần thử, "đang chờ quota", nút Huỷ.
- [x] Tests Rust (fake transport/transcriber, fixture media nhỏ) + Vitest cho store/route/dialog -- phủ I/O Matrix.

**Acceptance Criteria:**
- Given fake transcriber chậm, when huỷ ở từng pha, then `cancelled` phát ≤ 2 s, không Chunk mới nào được gửi sau huỷ, không dòng DB và thư mục staging không còn.
- Given Job đang chạy, when gọi `jobs_subscribe` hai lần liên tiếp, then mỗi Channel nhận snapshot chứa Job rồi các event có `seq` tăng liên tục, không mất event.
- Given `cargo test` và `npm test`, when chạy, then mọi test pass và `bindings.ts` khớp bản sinh.

## Implementation Notes

- Subagent Sonnet triển khai. Gate `transcribe_start` tách thành `decide_transcribe_start` để test thứ tự Consent → hash → trùng → key → start.
- Hash chạy trong lệnh IPC trước khi có Job, nên chưa huỷ được ở pha hash (file 90 phút hash ~1 s); huỷ ở decode/Proxy/Gemini/chờ quota có kiểm tra, test chuyên biệt chỉ phủ pha Gemini.
- Kiểm tra key = `secrets.list()` không rỗng; key bị vô hiệu/cooldown vẫn qua gate và sẽ thành `waitingQuota`/lỗi quota trong Job.
- Luồng đóng cửa sổ (`on_window_event` → `prevent_close` → `CloseRequested`/`exit`) không test được bằng unit test; phía UI và `app_close_confirm` có test.
- ID newtype có `Serialize`/`specta::Type` thủ công; enum IPC dùng `rename_all_fields = "camelCase"` để khớp TS.
- AR-53 (so với transcript v2 file 90 phút) cần key thật + file mẫu: QA thủ công.
- Review: `none` (pinned). 224 unit + 4 integration Rust, 190 Vitest, svelte-check/i18n/ui pass, bindings ổn định.

## Design Notes

Chunk lỗi → gap ngay trong 2.4 (thay vì fail cả Job) để không bao giờ bỏ công việc đã xong; 2.5 thêm Chạy lại và ma trận retry. Lỗi mang tính hệ thống (Auth/Model/Consent) dừng Job vì mọi Chunk sau đều sẽ lỗi. Proxy tạo trong staging trước khi transcribe; lỗi Proxy chỉ thành `proxy_error`. Đóng app: luôn `prevent_close`, hỏi registry async; rảnh → thoát, bận → emit `CloseRequested`.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml --locked` -- expected: pass
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` -- expected: sạch
- `npm test && npm run check && npm run check:i18n && npm run check:ui` -- expected: pass

**Manual checks:**
- AR-53 (file 90 phút so với transcript v2, lệch ≤ 2 s) cần key thật + file mẫu v2, không có trong môi trường — QA thủ công sau merge.
