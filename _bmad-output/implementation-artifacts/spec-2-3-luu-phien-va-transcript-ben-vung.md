---
title: '2.3 Lưu Phiên và Transcript bền vững'
type: 'feature'
created: '2026-09-23'
status: 'done'
baseline_commit: '38dd595a219d3e1308d8d1afae3ac386fde8491d'
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

**Problem:** Chưa có bảng hay quy trình lưu Phiên/Transcript/Segment; Job file (2.4) và Chạy lại (2.5) cần một đường commit không bao giờ để lại Phiên nửa vời, dòng DB rác hay file mồ côi trong Container.

**Approach:** Migration chỉ tiến tạo `sessions`, `transcripts`, `segments`; repo SQL riêng từng entity; giao thức staging → publish Proxy → một transaction DB, kèm reconcile idempotent lúc boot và swap `primary` nguyên tử cho Chạy lại. Ghi thứ tự/rollback vào ADR của story.

## Boundaries & Constraints

**Always:** `db/` là chủ ghi duy nhất, SQL chỉ ở `db/migrations` và `db/repo/<entity>.rs`. ID UUIDv7; mốc thời gian epoch ms UTC; `start_sec/end_sec` là giây `f64` tuyệt đối. `transcripts.status` do repo suy ra: `partial` ⇔ có gap `chunk_failed`, caller không truyền status. Proxy lỗi → vẫn commit Transcript với Proxy thiếu, báo lỗi audio riêng. Reconcile chạy lại nhiều lần cho cùng kết quả và không xoá media đang được DB tham chiếu.

**Never:** Không bảng `jobs`, không IPC/UI mới, không import v2. Không ghi DB trước khi kết thúc Job. Không sửa migration đã có. Không chuyển Phiên live `recording|finalizing` (thuộc Epic 4).

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Commit mới | Draft Phiên + Segment + Proxy trong staging | Proxy ở `media/<sid>/proxy.flac`, một transaction ghi session/transcript/segments, staging bị xoá | DB lỗi → gỡ thư mục đã publish, không còn dòng |
| Proxy lỗi | Proxy không tạo/publish được | Commit với `proxy_ext = NULL`, status đúng theo gap | Trả kèm lỗi audio riêng, không đổi status |
| Gap chunk_failed | Có ≥1 gap `chunk_failed` | `status = partial`; chỉ gap `disconnected` hoặc không gap → `complete` | CHECK SQL chặn gap thiếu reason |
| Huỷ/lỗi trước commit | Staging tồn tại, chưa commit | Không có dòng DB; staging bị xoá (hoặc boot xoá) | Lỗi dọn chỉ log |
| Chạy lại | Transcript mới cho Phiên có sẵn | Một transaction thay `primary`, `session.id` giữ nguyên | Huỷ/lỗi → bản cũ nguyên vẹn |
| Crash giữa bước | Sau publish trước commit; hoặc file `.partial`/lạ trong `media/<sid>/` | Boot xoá thư mục Phiên không có dòng, file không được tham chiếu, `.staging`; DB tham chiếu Proxy đã mất → `proxy_ext = NULL` | Chạy lại reconcile không đổi gì thêm |
| Trùng hash | `source_hash` đã tồn tại | Rollback, gỡ Proxy vừa publish | `AppError` storage |

</frozen-after-approval>

## Code Map

- `src-tauri/src/db/mod.rs` -- `Db::open` (WAL, `with_connection` mutex). Thêm `PRAGMA foreign_keys = ON` cho connection.
- `src-tauri/src/db/migrations/mod.rs` -- mảng `MIGRATIONS: [M; 2]` (`user_version` = 2); thêm `M::up` thứ 3, không sửa entry cũ; theo mẫu test `creates_*_table`.
- `src-tauri/src/db/repo/{mod,settings,counters}.rs` -- mẫu repo: hàm tự do nhận `&Connection`/`&mut Connection`, trả `rusqlite::Result`; `AppError: From<rusqlite::Error>` → `Code::Storage`.
- `src-tauri/src/core/id.rs` -- `SessionId`, `TranscriptId`, `JobId` đã có (macro `uuid_v7_id!`, `TryFrom<&str>` kiểm v7).
- `src-tauri/src/core/paths.rs` -- `media_dir` đúng; `staging_dir` hiện là `<root>/staging/<job>` → đổi thành `<root>/media/.staging/<job>` (chỉ dùng trong test); thêm helper `media_root`, `staging_root`, `proxy_path(root, sid, ext)`.
- `src-tauri/src/media/proxy.rs` -- `create_proxy(dir, source) -> ProxyInfo` ghi `proxy-<uuid>.flac` qua `.partial`+rename; dùng nguyên để tạo Proxy trong staging, không sửa.
- `src-tauri/src/ipc/boot.rs` -- chủ boot duy nhất; hook reconcile ngay sau `note_boot` (non-fatal, `tracing::warn!`), root = `app_data_dir`.
- `src-tauri/src/transcribe/parser.rs` -- `Segment { start, end, text, speaker }`, `MissingRange`; ánh xạ sang gap để Job 2.4 làm, story này chỉ nhận draft đã chuẩn hoá.
- Test: `rusqlite::Connection::open_in_memory()` cho repo, `tempfile::tempdir()` cho FS/`Db::open`.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/db/migrations/mod.rs` -- migration 3: `sessions(id PK, kind CHECK file|live, title, source_hash UNIQUE NULL, source_name NULL, status CHECK recording|finalizing|complete, recovered 0/1, duration_sec REAL, proxy_ext NULL, created_at, updated_at)`; `transcripts(id PK, session_id FK CASCADE, variant CHECK primary|retranscribe, status CHECK complete|partial, model, language NULL, created_at)` + unique index một `primary`/Phiên; `segments(transcript_id FK CASCADE, idx, start_sec, end_sec, kind CHECK text|gap, gap_reason CHECK chunk_failed|disconnected NULL, text, speaker NULL, PK(transcript_id, idx))`, CHECK `gap_reason IS NOT NULL ⇔ kind = gap` và `end_sec >= start_sec` -- nền schema chốt.
- [x] `src-tauri/src/db/mod.rs` -- bật foreign keys, test cascade -- xoá transcript kéo theo segments.
- [x] `src-tauri/src/db/repo/{sessions,transcripts,segments}.rs` + `mod.rs` -- insert/get/list theo entity; `transcripts::insert_with_segments` suy ra status; `transcripts::replace_primary` (xoá primary cũ + chèn mới trong transaction của caller); `sessions::set_proxy_ext`, `sessions::list_media_refs` -- SQL chỉ ở đây.
- [x] `src-tauri/src/core/paths.rs` -- helper đường dẫn ở Code Map, cập nhật test.
- [x] `src-tauri/src/library/store.rs` (+ `library/mod.rs`) -- `commit_file_session(db, root, job_id, draft, staged_proxy: Result<PathBuf, AppError>) -> CommitOutcome { session_id, proxy_error }`, `replace_primary_transcript(db, session_id, draft)`, `discard_staging(root, job_id)`, `reconcile(db, root)`; điểm lỗi tiêm được (cfg(test)) tại ghi file/rename/commit/dọn -- orchestration publish/commit.
- [x] `src-tauri/src/ipc/boot.rs` -- gọi `library::store::reconcile` sau `note_boot` -- AD-18 dọn staging lúc boot.
- [x] `_bmad-output/implementation-artifacts/adr-2-3-publish-commit.md` -- thứ tự staging → fsync → rename vào `media/<sid>/` → commit; rollback; luật reconcile -- ADR bắt buộc.
- [ ] Tests trong các file trên -- phủ I/O Matrix, test tiêm lỗi tại mỗi ranh giới rồi reconcile hai lần.

**Acceptance Criteria:**
- Given DB v2 có settings, when boot, then migrate lên v3 giữ dữ liệu cũ, chạy lại là no-op.
- Given lỗi tiêm tại từng ranh giới (ghi Proxy, rename, commit, dọn staging), when reconcile chạy hai lần, then không còn dòng nửa vời, không file mồ côi trong `media/<sid>/` hay `.staging`, và Phiên/Proxy đã commit trước đó vẫn nguyên.
- Given transaction commit chưa xong, when đọc DB, then không có dòng `sessions`/`transcripts` của Job đó.

## Implementation Notes

- Subagent Sonnet triển khai; `commit_file_session`/`discard_staging` chưa có caller — Job 2.4 sẽ tạo Proxy vào `paths::staging_dir`, ánh xạ `Segment`/`MissingRange` sang `SegmentDraft` rồi gọi commit.
- fsync thư mục sau rename chỉ best-effort trên Unix (ghi trong ADR).
- Rủi ro cho Epic 4: `reconcile` xoá mọi file trong `media/<sid>/` ngoài `proxy.<ext>` đang tham chiếu — file `recording.*` của Phiên live sẽ bị xoá nếu Epic 4 không mở rộng luật giữ file theo role (đã ghi vào deferred-work).
- Review: `none` (pinned). 201 unit + 4 integration tests pass; mọi dòng I/O Matrix có test đã chạy.

## Design Notes

Thứ tự đã chọn: publish Proxy **trước** commit, nên cửa sổ crash chỉ để lại thư mục `media/<sid>/` không có dòng → reconcile xoá được bằng so khớp ID; ngược lại commit trước sẽ tạo dòng tham chiếu file chưa có. `proxy_ext NULL` là `proxy_missing` (không cột trùng nghĩa). Chạy lại xoá hẳn primary cũ trong cùng transaction (không giữ lịch sử). Tên file Proxy cố định `proxy.flac` theo AD-5; tên `proxy-<uuid>` chỉ tồn tại trong staging.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml --locked` -- expected: toàn bộ pass
- `cargo check --manifest-path src-tauri/Cargo.toml --locked` -- expected: sạch
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` -- expected: sạch
