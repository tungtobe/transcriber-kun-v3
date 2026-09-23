---
title: '2.5 Chịu lỗi Chunk, Khoảng thiếu và Chạy lại'
type: 'feature'
created: '2026-09-23'
status: 'done'
baseline_commit: '4342a51028e0fe9168f927cb52d4a8af0c1c10bc'
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

**Problem:** 2.4 đã biến Chunk lỗi thành gap `chunk_failed` và commit Transcript `partial`, nhưng người dùng không có cách lấp phần thiếu, ma trận retry của `gemini/` chưa được khoá bằng test đầu-cuối, và UI không biết Phiên nào partial.

**Approach:** Thêm Job loại "Chạy lại" vào đúng hàng đợi `JobRegistry`: lệnh `transcribe_rerun(session_id, transcript_id, scope)` với `scope ∈ {missing, all, gap(gap_id)}` decode Proxy trong Container, chỉ gửi các vùng cần retry, hợp nhất với Segment cũ rồi swap nguyên tử Transcript đích (có kiểm tra transcript đích chưa bị thay). Khoá chính sách retry hiện có bằng test transport giả, và lộ cờ `partial` + nút "Chạy lại phần thiếu" tối thiểu ở `/session/:id`.

## Boundaries & Constraints

**Always:** Bộ đếm lần gửi chỉ nằm ở `gemini/` (key pool, `MAX_ATTEMPTS = 4` gồm lần đầu + đổi key); chờ quota không tính là lần gửi. `gap_id` = `idx` của Segment gap `chunk_failed` trong đúng `transcript_id`; Rust tự tra range từ DB và kiểm tra transcript thuộc `session_id` — không nhận range từ UI. Nguồn audio Chạy lại luôn là Proxy đã publish (`sessions.proxy_ext` + `paths::proxy_path`), không cần file nguồn. Giữ nguyên mọi Segment ngoài vùng retry; Segment mới được kẹp trong vùng, phần vẫn lỗi thành gap `chunk_failed`; thời gian không lùi. Swap trong một transaction: chỉ khi transcript đích vẫn tồn tại với đúng id lúc bắt đầu, xoá nó và chèn bản mới cùng `variant`; `status` do repo suy ra (còn gap → `partial`). `sessions.status` không đổi, `session_id` giữ nguyên. Chạy lại bị huỷ/lỗi → Transcript cũ nguyên vẹn. Mỗi Phiên tối đa một Job Chạy lại đang chờ/chạy: gọi lại trả Job hiện có. Log không chứa transcript, path đầy đủ hay key.

**Never:** Không bảng `jobs`, không hàng đợi thứ hai, không chạy Job song song. Không đổi `TRANSCRIBE_CHUNK_TIMEOUT`/`MAX_ATTEMPTS`/cooldown hay chuyển bộ đếm ra khỏi `gemini/`. Không Chạy lại vào `primary` của Phiên `live` (chỉ `retranscribe`); không đụng gap `disconnected`. Không Transcript detail/trình phát (2.7), không danh sách Home (2.9), không Settings chunk/ngôn ngữ (2.6). Không migration mới.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| 429 xoay key | Key 1 trả 429, key 2 thành công | Chunk thành công, `attempt` 2, key 1 cooldown | — |
| 5xx | 3× 503 rồi 200 | Thành công ở lần gửi thứ 4 | 4× 503 → Chunk thành gap, Job tiếp tục |
| 400/404/451 | Lần gửi đầu | Không gửi lại; 400 → gap; 404 (`Model`) và 451 (`Blocked`) giữ phân loại fatal của 2.4 → lỗi Job | — |
| Timeout | Transport treo quá hạn | Kết thúc request, không gửi cùng audio sang key khác → gap | — |
| JSON cắt cụt | Response hỏng giữa mảng | Segment hợp lệ giữ lại, phần còn lại thành gap `chunk_failed` | Không Chunk nào biến mất âm thầm |
| 401 giữa chừng | Key 1 401 ở Chunk 2, còn key 2 | Key 1 disabled, Chunk 2 thành công bằng key 2 | Mọi key 401 → lỗi Job (`auth`), không commit |
| Chạy lại `missing` | Transcript partial, 2 gap | Chỉ 2 vùng được gửi; kết quả hợp nhất, hết gap → `complete` | Vẫn lỗi → vẫn `partial` với gap còn lại |
| Chạy lại `gap(id)` | `gap_id` hợp lệ | Chỉ vùng đó được gửi | `gap_id` không tồn tại / không phải `chunk_failed` / transcript khác Phiên → lỗi `Request`, không Job |
| Chạy lại `all` | Bất kỳ | Transcribe lại toàn bộ Proxy, bỏ Segment cũ | — |
| Không có gì để chạy | `missing` trên Transcript `complete` | `NothingToRerun`, không Job | — |
| Thiếu Proxy | `proxy_ext` NULL hoặc file mất | Không Job | Lỗi có category rõ ràng |
| Gọi trùng | Job Chạy lại của Phiên đang chờ/chạy | Trả `Existing { job_id }` | — |
| Kết quả tới muộn | Transcript đích đã bị thay khi Job commit | Không ghi gì | Lỗi Job `conflict`/`request` |
| Huỷ / lỗi Chạy lại | Bất kỳ pha | Transcript cũ nguyên vẹn | Event `cancelled`/`error` như 2.4 |
| Phiên live | `transcript_id` là `primary` của Phiên `live` | Không Job | Lỗi `Request` |
| `/session/:id` Phiên partial | Primary `partial` | Cảnh báo + nút "Chạy lại phần thiếu"; bấm → tiến độ Job | Lỗi hiển thị qua i18n |

</frozen-after-approval>

## Code Map

- `src-tauri/src/gemini/mod.rs:637` `post_job_observed` + `keys.rs:560-760` (`report`, `retry_or_finish`, `MAX_ATTEMPTS` từ `params.rs`) -- chính sách retry đã đúng AC: Quota/Auth/Server rotate trong ngân sách 4 lần, Request/Timeout kết thúc, deadline 120 s (gồm cả chờ quota, ≤ 180 s PRD). Chỉ thêm test; không sửa trừ khi test lộ lỗi. Helper test sẵn có trong module: `gateway_with`, fake transport theo danh sách response, fake clock.
- `src-tauri/src/transcribe/registry.rs` -- actor `JobRegistry`: `StartParams`, `Command`, `JobEntry`, `handle_start`, `start_pipeline_for`, `run_job_inner`, `decode_and_transcribe` (Chunker + kênh bounded, `is_fatal`, `merge.push_failed_chunk`), `RegistryObserver`, tests với `FakeTranscriber`/`wav_fixture`. Mở rộng thành Job có loại (file | rerun); tái dùng vòng Chunk/observer/huỷ.
- `src-tauri/src/transcribe/job.rs` -- `JobSnapshot`, `JobEvent` (tag `kind`, `rename_all_fields = "camelCase"`), `CancelOutcome`. Thêm `kind: JobKind` vào snapshot.
- `src-tauri/src/transcribe/merge.rs` -- `MergeBuilder` (`push_success`, `push_failed_chunk`, `push_forward` giữ đơn điệu, `finish`). Tái dùng cho vùng retry; thêm hàm hợp nhất Segment cũ + kết quả vùng.
- `src-tauri/src/media/chunk.rs` -- `Chunker`/`Chunk { start_sample, start_ms, duration_ms, … }` đánh thời gian từ 0: với vùng retry, tạo Chunker riêng cho từng vùng từ mẫu decode của Proxy và dời `start_ms`/`start_sample` về thời gian tuyệt đối trước khi gửi (adapter/parser dùng `chunk.start_ms` để ra giây tuyệt đối). `media::decode_mono_16khz` đọc được FLAC Proxy.
- `src-tauri/src/db/repo/transcripts.rs` -- `Variant`, `derive_status`, `insert_with_segments`, `replace_primary`. Thêm: đọc transcript theo id (session_id, variant, status, model, language), transcript primary của Phiên, và swap theo id trong transaction của caller.
- `src-tauri/src/db/repo/segments.rs` -- `list_for_transcript` (có `idx`), `SegmentDraft`, `GapReason`.
- `src-tauri/src/library/store.rs:286` `replace_primary_transcript` -- mẫu transaction + fault injection; thêm hàm swap có kiểm tra id đích, và `SessionSummary` thêm `partial` + `primary_transcript_id`.
- `src-tauri/src/db/repo/sessions.rs` -- `SessionRow` (`kind`, `proxy_ext`), `get`.
- `src-tauri/src/core/paths.rs:29` `proxy_path(root, session_id, ext)`.
- `src-tauri/src/ipc/mod.rs` -- mẫu `transcribe_start_inner` + `decide_transcribe_start` (gate Consent/key tách hàm để test), `SessionLookup::Session`, `library_session_get_inner`, `collect_commands!` ở `specta_builder()`; test `export_bindings` tái sinh `src/lib/bindings.ts`.
- Frontend: `src/routes/Session.svelte` (+ `Session.test.ts`), `src/lib/stores/jobs.svelte.ts`, i18n `src/i18n/{vi,en,ja}.json` (`npm run check:i18n`).

## Tasks & Acceptance

**Execution:**
- [ ] `src-tauri/src/gemini/mod.rs` (tests) -- thêm test chuỗi response cho: 429 xoay key, 3×5xx rồi thành công, 4×5xx cạn ngân sách, 400/404/451 không gửi lại, timeout không fan-out, 401 key 1 rồi key 2 thành công -- khoá AC retry ở đúng tầng sở hữu bộ đếm.
- [ ] `src-tauri/src/db/repo/{transcripts,segments}.rs`, `library/store.rs` -- `transcripts::get`, `primary_for_session`, `swap(conn, expected_id, new_id, …)` (không khớp → lỗi stale, không ghi); `store::swap_transcript` (transaction, fault injection như `replace_primary_transcript`); `SessionSummary` thêm `partial`, `primary_transcript_id`.
- [ ] `src-tauri/src/transcribe/merge.rs` -- hàm hợp nhất: Segment cũ ngoài các vùng + kết quả từng vùng (kẹp trong vùng, phần lỗi/unresolved → gap `chunk_failed`), đơn điệu, không trùng text/gap.
- [ ] `src-tauri/src/transcribe/{job,registry}.rs` -- `JobKind { Transcribe, Rerun }` trên snapshot; `RerunParams { session_id, transcript_id, ranges | all, proxy_path, model, consent }`; `start_rerun` trả `Started`/`Existing` (một Job Chạy lại mỗi Phiên); pipeline rerun decode Proxy, chỉ chunk các vùng, tiến độ = ms vùng đã xử lý / tổng ms vùng, commit bằng `store::swap_transcript`, huỷ/lỗi không ghi và dọn staging.
- [ ] `src-tauri/src/ipc/mod.rs` -- lệnh `transcribe_rerun(session_id, transcript_id, scope)` với gate tách hàm test được: Consent → tra Phiên/transcript thuộc Phiên → từ chối `primary` của Phiên live → giải scope thành vùng (`gap_id` phải là gap `chunk_failed`) → `NothingToRerun` nếu rỗng → Proxy tồn tại → có key → Job hiện có hoặc start. Chụp `transcribe_model` từ Settings. `SessionLookup::Session` thêm `partial`, `transcriptId`. Tái sinh `bindings.ts`.
- [ ] `src/routes/Session.svelte`, `src/lib/stores/jobs.svelte.ts`, i18n 3 ngôn ngữ -- Phiên partial hiện cảnh báo + nút "Chạy lại phần thiếu" (scope `missing`); `Started`/`Existing` → hiển thị tiến độ Job như 2.4 và nhãn Chạy lại; lỗi/`NothingToRerun` hiện thông điệp đã dịch.
- [ ] Tests Rust (registry với `FakeTranscriber` + test tích hợp `GatewayTranscriber` qua transport giả cho 4 kịch bản AR-36) và Vitest cho Session partial/nút/lỗi -- phủ I/O Matrix.

**Acceptance Criteria:**
- Given Transcript partial với hai gap và fake transcriber thành công, when `transcribe_rerun(missing)`, then chỉ đúng hai vùng được gửi (start_ms tuyệt đối), Segment ngoài vùng giống hệt trước, Transcript mới `complete`, `session_id` giữ nguyên và transcript cũ không còn.
- Given Chạy lại đang chạy, when huỷ hoặc transcriber trả lỗi fatal, then transcript và segments cũ trong DB không đổi và staging không còn.
- Given Job Chạy lại đang chờ/chạy cho Phiên X, when gọi `transcribe_rerun` lần nữa, then trả `Existing` cùng `job_id`, registry vẫn chỉ có một Job cho X.
- Given transcript đích đã bị thay giữa chừng, when Job Chạy lại commit, then không ghi gì và Job kết thúc `error`.
- Given `cargo test`, `npm test`, `npm run check`, `npm run check:i18n`, `npm run check:ui`, when chạy, then pass và `bindings.ts` khớp bản sinh.

## Implementation Notes

- `JobKind::Rerun` chia sẻ đúng một hàng đợi/`JobRegistryHandle` với `Transcribe`; `pending_rerun_params` là map thứ hai song song `pending_params`, `start_pipeline_for` chọn đúng pipeline theo map nào có entry.
- `transcribe::rerun` module mới: `RerunScope`/`resolve_ranges` (thuần, không I/O) tách khỏi `decode_ranges_and_chunk` (decode Proxy một lần, định tuyến mẫu vào `Chunker` riêng từng vùng theo con trỏ `active` tăng dần — spec Design Notes). `gap_id` xuất ra IPC là `i32` (specta-typescript cấm `i64`/BigInt), quy đổi sang `i64` khi so khớp `idx` trong DB.
- `transcripts::swap`/`store::swap_transcript` so khớp `expected_transcript_id` đúng lúc ghi (đọc `primary_for_session` trong cùng transaction rồi mới `DELETE`+insert) — không khoá, không cần biết gì về gap_id cũ đã hết hiệu lực.
- `AppState` có thêm `data_dir` (trước đây chỉ boot cục bộ giữ) để `transcribe_rerun` tự kiểm Proxy tồn tại trước khi tạo Job, không qua round-trip registry actor cho một việc thuần đọc đường dẫn.
- Test registry dùng `FakeTranscriber` (đã thêm `chunks: Mutex<Vec<Chunk>>` để khẳng định `start_ms` tuyệt đối) cho cả 4 kịch bản Acceptance Criteria: 2 gap chỉ gửi đúng 2 vùng, lỗi fatal/huỷ giữ transcript cũ, gọi trùng trả `Existing`, và commit với `expected_transcript_id` lỗi thời không ghi gì.
- 4 test mới ở `gemini::mod` khoá đúng AC retry còn thiếu (429 xoay key, 3×5xx rồi thành công ở lần 4, 4×5xx cạn ngân sách, 401 xoay key) — 400/404/451 và timeout không fan-out đã có sẵn từ 2.2/2.4.
- Clippy baseline: `insert_with_segments`, `run_job`/`run_job_inner` (8 tham số, đã có từ 2.3/2.4) và một test cũ dùng `unwrap_err()` trên literal đã fail `cargo clippy --all-targets -- -D warnings` **trước cả thay đổi này** (xác nhận bằng `git stash` rồi chạy lại trên baseline) — không thuộc phạm vi 2.5, không sửa. Hàm mới của story này (`swap`, `run_rerun_job`, `decode_and_transcribe_ranges`) được gắn `#[allow(clippy::too_many_arguments)]` để không cộng dồn nợ kỹ thuật.
- Chưa QA thủ công bằng key Gemini thật cho Chạy lại (chỉ FakeTranscriber) — tương tự AR-53 của 2.4, decode/chunk/merge/swap đã khoá bằng test nhưng phản hồi model thật cho một Proxy dài chưa được thử.
- Sau audit diff, sửa 3 khoảng hở: (1) `transcripts::swap` giờ đọc lại transcript theo `expected_id` (không hard-code `Variant::Primary`/`primary_for_session`) và giữ nguyên `variant` của bản cũ khi swap, cộng test swap một transcript `retranscribe`. (2) Tách helper test của `gemini/` (`FakeProvider`/`FakeClock`/`key`/`gateway_with`/`FakeTransport`) vào `gemini::test_support` (`#[cfg(test)] pub(crate)`) để `transcribe::registry::tests` dựng một `GeminiGateway` thật (transport giả, không mạng) bọc trong `GatewayTranscriber` thật, chạy qua `JobRegistry` thật cho cả 5 kịch bản AR-36 (429 xoay key, 4×5xx thành gap vẫn commit partial, JSON cắt cụt giữ segment hợp lệ + gap phần còn lại, 401 mọi key → lỗi Job không dòng DB, 401 giữa Job ở Chunk 2 thật — dùng fixture im lặng 301 giây để vượt mốc 300 giây của `Chunker` thật — xoay key thành công) kèm assertion "không Chunk nào mất tích" (segment phủ kín, liên tục). (3) Thêm test registry: Chạy lại `missing` với một gap vẫn lỗi (non-fatal) → transcript mới vẫn `partial`, đúng gap đó còn lại, gap kia đã vá, segment ngoài vùng giữ nguyên.

## Spec Change Log

## Review Triage Log

## Design Notes

Retry: giữ nguyên deadline 120 s/Chunk của 2.2 (bao gồm chờ quota) — chặt hơn trần 180 s của PRD nên vẫn thoả AC; 2.5 chỉ khoá bằng test. Chạy lại decode toàn bộ Proxy (≥ 20× realtime) nhưng chỉ đẩy mẫu thuộc vùng vào Chunker riêng từng vùng, rồi dời thời gian:

```rust
let mut chunk = chunk;               // Chunker của vùng bắt đầu từ 0
chunk.start_ms += range.start_ms;    // về thời gian tuyệt đối
chunk.start_sample += range.start_sample;
```

Chống kết quả tới muộn: swap so khớp `expected_transcript_id` trong transaction thay vì khoá — vì id transcript đổi sau mỗi swap, `gap_id` (idx) của bản cũ tự hết hiệu lực.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml --locked` -- expected: pass
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` -- expected: sạch
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` -- expected: sạch
- `npm test && npm run check && npm run check:i18n && npm run check:ui` -- expected: pass
