---
title: '2.9 Home — danh sách phiên và card job'
type: 'feature'
created: '2026-09-24'
status: 'done'
baseline_commit: '8310a538f3032fe57978fc35b94d5a692f3b2f4b'
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

**Problem:** Home vẫn là màn trống của Epic 1: người dùng không thấy các Phiên đã lưu, không thấy Job đang chạy/chờ, và sidebar chỉ có placeholder "Job đang chạy · 0" — rời màn Transcript giữa chừng là mất đường quay lại (FR-27, FR-15).

**Approach:** Thêm IPC liệt kê Phiên cho Home (một truy vấn, mới nhất trước, kèm số khoảng `chunk_failed` của transcript `primary`), store `library` phía UI tự tải lại khi có Job commit, Home hai trạng thái (trống / có phiên với drop-zone mỏng + virtual list), card job đầy đủ ở đầu danh sách và card thu gọn ở sidebar nhìn thấy từ mọi màn.

## Boundaries & Constraints

**Always:** Trạng thái trống (không có Phiên và không có Job): giữ card "Kéo file vào đây / Chọn file" (hành vi chọn file của 2.8) với mô tả liệt kê đủ định dạng hỗ trợ `mp3, m4a, wav, flac, ogg, aiff, caf, mp4, mov, mkv, webm`; card Live giữ nguyên dạng vô hiệu hiện có. Trạng thái có phiên: drop-zone mỏng (viền `1px dashed border-strong`, radius `2xl`, chữ "Kéo file vào đây hoặc" + nút "Chọn file" cùng quy tắc vô hiệu khi chưa có key của 2.8) nằm trên danh sách. Mỗi dòng Phiên: tên; ngay sau tên là badge `partial` "Thiếu N khoảng" (N = số Segment gap `chunk_failed` của transcript `primary`, chỉ khi N > 0) và badge `recover` "Phục hồi" khi `recovered`; ngày theo múi giờ cục bộ; badge `live`/`file` theo `kind`; thời lượng mono tabular (`HH:MM:SS` khi ≥ 1 giờ, ngược lại `MM:SS`). Không hiển thị model, số Segment, cột trạng thái. Sắp `created_at` giảm dần (tie-break theo `id` giảm dần). Mỗi dòng là liên kết tới `/session/:id` (click hoặc Enter). Virtual list chiều cao dòng cố định: chỉ render các dòng trong khung nhìn + đệm nhỏ, toàn bộ danh sách tải một lần (không phân trang, không infinite scroll). Lúc đang tải: header + sidebar hiện ngay, vùng danh sách là skeleton 6 dòng cùng bố cục dòng thật; lỗi tải → thông báo i18n + nút "Thử lại". Card job (kiểu warning, radius `xl`) ở đầu danh sách cho Job đang chạy (hoặc Job đầu hàng nếu chưa có Job chạy): tên file (`sourceName`; Job Chạy lại dùng nhãn "Đang chạy lại"), "32 / 90 phút · 36 %", dòng chi tiết Chunk · key · lần thử, "Đang chờ quota…" khi `waitingQuota`, "N file đang chờ" khi có Job Transcribe `queued` khác, nút "Mở" (`/session/:sessionId`) và "Huỷ" (`jobsStore.cancel`). Card job hiện cả ở trạng thái trống (có Job nhưng chưa có Phiên). Sidebar: thay placeholder bằng card thu gọn hiện trên mọi màn — số Job đang chạy/chờ, tên + % của Job đang chạy, click mở `/session/:sessionId`; không có Job → dòng "Không có job nào đang chạy.". `jobsStore` đếm tham chiếu `subscribe`/`unsubscribe` để sidebar (luôn gắn) và `/session/:id` cùng dùng một Channel mà không xoá state của nhau. Khi Job Transcribe hoặc Chạy lại commit (event `result`), danh sách tải lại không cần người dùng thao tác: Phiên mới lên đầu, badge "Thiếu N khoảng" cập nhật/biến mất, card job biến mất. Mọi chuỗi qua i18n vi/en/ja; tên/ngày không bao giờ ghép vào path.

**Never:** Không tìm theo tên, chip tag, lọc, footer "N / M phiên" (Epic 3), không đổi tên inline/menu ⋯/xoá/tải recording (Epic 3), không nút/luồng Live (Epic 4). Không thêm dependency npm/Cargo (virtual list tự viết). Không thêm bảng/cột DB hay migration. Không hiển thị badge memo/audio ở Home.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Trống | 0 Phiên, 0 Job | Card "Kéo file vào đây / Chọn file" + danh sách định dạng; không danh sách | — |
| Có phiên | 3 Phiên | Drop-zone mỏng + 3 dòng mới nhất trước; không model/segment/trạng thái | — |
| Partial | Phiên có 2 gap `chunk_failed` | Badge "Thiếu 2 khoảng" ngay sau tên | Gap `disconnected` không tính |
| Phục hồi | `recovered = 1` | Badge "Phục hồi" ngay sau tên | — |
| Mở | Click/Enter trên dòng | Điều hướng `/session/:id` | — |
| 500 phiên | 500 Phiên mẫu | Danh sách hiển thị ≤ 1 s; DOM chỉ chứa phần dòng trong khung nhìn (≪ 500) | — |
| Đang tải | IPC chưa trả | Skeleton 6 dòng; sidebar/header hiện | Lỗi → thông báo + "Thử lại" |
| Job chạy | 1 Job running 32/90 phút, 1 queued | Card warning đầu danh sách "32 / 90 phút · 36 %", chi tiết, "1 file đang chờ", Mở/Huỷ | Huỷ lỗi → giữ card, nút dùng lại được |
| Chờ quota | `waitingQuota = true` | Card hiện "Đang chờ quota…" | — |
| Rời màn | Ở `/settings/*` hoặc `/session/:id` khi Job chạy | Sidebar card thu gọn hiện, click mở view Job | — |
| Commit | Event `result` | Phiên mới đầu danh sách, card job biến mất, không reload tay | Tải lại lỗi → giữ danh sách cũ + thông báo |
| Chạy lại xong | Rerun đầy đủ các gap | Badge "Thiếu N khoảng" biến mất | — |
| Hai subscriber | Sidebar + `/session/:id` cùng subscribe, rời `/session/:id` | Sidebar vẫn có dữ liệu Job | — |

</frozen-after-approval>

## Code Map

- `src-tauri/src/db/repo/sessions.rs` -- `list` (ORDER BY `created_at DESC`), `SessionRow`, `SELECT_COLUMNS`; thêm hàm list cho Home dùng một truy vấn `LEFT JOIN transcripts (variant='primary')` + đếm `segments` `gap_reason='chunk_failed'` (subquery/GROUP BY) — không N+1. Schema ở `src-tauri/src/db/migrations/mod.rs` (sessions/transcripts/segments; không đổi).
- `src-tauri/src/ipc/mod.rs` -- mẫu `library_session_get`/`library_session_detail` (`blocking(...)`, `track_ipc_error`, `with_connection`); thêm `library_sessions_list() -> Vec<SessionListItem>` (`session_id`, `kind`, `title`, `created_at: f64` epoch-ms, `duration_sec`, `recovered`, `missing_gap_count: i32` — specta cấm BigInt, cùng lý do `SessionDetail.created_at` f64); đăng ký `specta_builder()`; tái sinh `bindings.ts` qua test `export_bindings`.
- `src/lib/stores/jobs.svelte.ts` -- `subscribe`/`unsubscribe` (hiện xoá state mỗi lần unsubscribe) → đếm tham chiếu; thêm bộ đếm `resultSeq` (tăng mỗi event `result`) để store khác phản ứng. `start/cancel` giữ nguyên. Test hiện có: `jobs.svelte.test.ts`.
- `src/routes/session/JobProgress.svelte` -- `minutes`/`percent`, `queuedTranscribeCount`, khoá i18n `session.job.*` (`progressLabel`, `chunkLabel`, `keyLabel`, `attemptLabel`, `waitingQuota`, `cancelAction`, `cancelling`, `kindRerun`, `queuedCount`) — tái dùng cho card Home, tách helper chung nếu cần thay vì chép.
- `src/routes/Home.svelte` (+`Home.test.ts`) -- trạng thái trống hiện có (card file 2.8 + card Live vô hiệu), `IntakeNotices`, banner thiếu key, `intakeStore.pick()`, quy tắc vô hiệu `keysStore`. `src/components/AppShell.svelte` -- `.job-placeholder` (khoá `app.shell.runningJobs`/`noRunningJobs`) → card thu gọn; AppShell đã có listener kéo thả 2.8.
- `src/components/Badge.svelte` (7 biến thể, có `partial`/`recover`/`live`/`file`), `src/lib/time.ts` (`formatTimestamp`), `src/routes/session/SessionHeader.svelte` (cách định dạng ngày cục bộ + thời lượng có sẵn — tái dùng).
- `src/routes/Session.svelte` -- gọi `jobsStore.subscribe/unsubscribe` theo view `job`; phải vẫn đúng sau khi đếm tham chiếu.
- DESIGN.md `rounded.xl` card (dòng 427), `drop-zone` (274–278); EXPERIENCE.md Dòng phiên/Card job (93–94), Cold load (132).

## Tasks & Acceptance

**Execution:**
- [ ] `src-tauri/src/db/repo/sessions.rs` -- hàm list cho Home trả `created_at`, `recovered`, `missing_gap_count` trong một truy vấn; tests: thứ tự mới nhất trước, đếm chỉ `chunk_failed` của `primary` (bỏ `disconnected`, bỏ `retranscribe`), Phiên không có transcript → 0, 500 Phiên đọc nhanh.
- [ ] `src-tauri/src/ipc/mod.rs` -- `SessionListItem` + lệnh `library_sessions_list`; tái sinh `src/lib/bindings.ts`.
- [ ] `src/lib/stores/jobs.svelte.ts` (+test) -- đếm tham chiếu subscribe/unsubscribe; `resultSeq`.
- [ ] `src/lib/stores/library.svelte.ts` (+test) -- `load()` (`loading|ready|error`), `sessions`, tự tải lại khi `jobsStore.resultSeq` đổi; tải lại lỗi giữ danh sách cũ.
- [ ] `src/components/JobCard.svelte` (+test) -- biến thể `full` (Home) và `compact` (sidebar) theo Boundaries.
- [ ] `src/routes/home/SessionList.svelte`, `SessionRow.svelte`, `SessionListSkeleton.svelte` (+tests) -- virtual list chiều cao cố định, dòng là liên kết, skeleton 6 dòng.
- [ ] `src/routes/Home.svelte` (+test) -- hai trạng thái, drop-zone mỏng, card job đầu danh sách, subscribe `jobsStore` khi mount; mô tả card file liệt kê đủ định dạng.
- [ ] `src/components/AppShell.svelte` (+test) -- card job thu gọn thay placeholder, subscribe `jobsStore` suốt vòng đời shell.
- [ ] i18n vi/en/ja -- "Thiếu {count} khoảng", "Phục hồi" (nếu chưa có), drop-zone mỏng, "Mở", lỗi tải + "Thử lại", nhãn sidebar, mô tả định dạng.

**Acceptance Criteria:**
- Given 500 Phiên được mock trả từ `librarySessionsList`, when Home render trong jsdom với khung nhìn 800 px, then số dòng trong DOM < 60 và dòng đầu là Phiên có `createdAt` lớn nhất.
- Given Home đang hiện 2 Phiên và 1 Job chạy, when `jobsStore` nhận event `result` cho Job đó, then `librarySessionsList` được gọi lại, Phiên mới xuất hiện đầu danh sách và card job không còn.
- Given `cargo test`, `cargo fmt --check`, `npm test`, `npm run check`, `npm run check:i18n`, `npm run check:ui`, when chạy, then pass và `bindings.ts` khớp bản sinh.

## Implementation Notes
- Sửa ở bước kiểm diff (agent chính): (1) `Home.svelte` chỉ khẳng định trạng thái trống khi `jobsStore.synced` (hoặc subscribe lỗi) — tránh chớp hai card lớn trước snapshot Job đầu; (2) `libraryStore.load()` gọi trong lúc một lần tải đang bay nay xếp đúng một lần tải bổ sung thay vì trả promise cũ (kết quả cũ có thể thiếu Phiên vừa commit); (3) thêm test "Huỷ lỗi → giữ card, nút dùng lại được" (I/O Matrix "Job chạy").

## Spec Change Log

## Review Triage Log

## Design Notes

Virtual list tự viết đủ dùng vì chiều cao dòng cố định: `scrollTop` → chỉ số đầu = `floor(scrollTop / rowHeight) - overscan`, container trong có chiều cao `count × rowHeight`, dòng hiển thị dịch bằng `transform: translateY`. jsdom không có layout nên test cần cấp chiều cao khung nhìn qua prop/biến có thể ghi đè (mặc định đọc `clientHeight`, fallback hợp lý khi bằng 0). Đếm tham chiếu `jobsStore`: `subscribe()` lần đầu mở Channel, các lần sau chỉ tăng đếm (nếu đang `error` thì mở lại); `unsubscribe()` chỉ xoá state khi đếm về 0.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml --locked` -- expected: pass
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` -- expected: sạch
- `npm test && npm run check && npm run check:i18n && npm run check:ui` -- expected: pass

**Manual checks:**
- NFR-5: đo thật trên máy đích — app mở tới Home ≤ 2 s, 500 Phiên hiển thị ≤ 1 s.
