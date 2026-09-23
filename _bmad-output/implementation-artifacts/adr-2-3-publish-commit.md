# ADR 2.3: Thứ tự publish/commit và luật reconcile

**Status:** Decided, implemented.
**Date:** 2026-09-23

## Decision

Một Phiên file được lưu bền vững qua đúng bốn bước, theo thứ tự cố định, cài
trong `library::store::commit_file_session`:

1. **Staging** (ngoài phạm vi hàm này — job 2.4 gọi `media::create_proxy`
   trong `media/.staging/<job-id>/`, sinh `proxy-<uuid>.flac`).
2. **Publish Proxy**: tạo `media/<session-id>/`, `fsync` nội dung file
   staging, rồi `rename` nó thành tên cố định `media/<session-id>/proxy.flac`
   (AD-5 — không bao giờ tên `proxy-<uuid>` ở đích cuối). `fsync` thư mục cha
   sau `rename` là best-effort, chỉ trên Unix (không có API `std` tương đương
   an toàn trên Windows; NTFS coi `rename` gần-atomic hơn nên bỏ qua ở đó chấp
   nhận được).
3. **Commit DB**: một transaction duy nhất ghi `sessions` + `transcripts` +
   `segments`.
4. **Dọn staging**: xoá `media/.staging/<job-id>/`. Lỗi ở bước này chỉ log
   (`tracing::warn!`), không làm hỏng kết quả — DB đã commit xong là thành
   công thật, phần staging còn sót sẽ bị reconcile lúc boot dọn nốt.

Publish (bước 2) **luôn xảy ra trước** commit (bước 3) — quyết định cốt lõi
của ADR này.

### Vì sao publish trước commit

Hai thứ tự khả dĩ:

- **Publish trước commit (chọn):** cửa sổ crash giữa bước 2 và 3 để lại một
  thư mục `media/<sid>/` có Proxy nhưng **không có dòng DB nào tham chiếu**.
  Reconcile chỉ cần so khớp: liệt kê `sessions.id`, thư mục nào không khớp id
  nào → xoá hẳn. Không cần biết gì thêm về trạng thái nội bộ của thư mục đó.
- **Commit trước publish (loại):** cửa sổ crash để lại một dòng
  `sessions`/`transcripts` **tham chiếu một Proxy chưa tồn tại** — không phân
  biệt được với trường hợp hợp lệ "Proxy lỗi" (`proxy_ext = NULL` có chủ ý).
  Reconcile sẽ phải suy luận thêm (dòng này có phải đang "chờ publish" hay là
  "Proxy lỗi vĩnh viễn"?) mà không có tín hiệu nào phân biệt hai trường hợp.

Thứ tự đã chọn giữ reconcile là một phép so khớp id thuần, không trạng thái
trung gian nào cần suy luận thêm.

## Rollback

- **Publish lỗi** (ghi/`fsync`/`rename` thất bại, hoặc staging còn chưa từng
  tạo được Proxy): không phải lỗi commit. `commit_file_session` tiếp tục ghi
  Transcript với `proxy_ext = NULL`, trả `proxy_error` riêng cho caller hiển
  thị lỗi audio tách biệt (spec I/O Matrix "Proxy lỗi") — `status` của
  Transcript không bị ảnh hưởng bởi việc thiếu Proxy.
- **Commit lỗi** (constraint SQL — ví dụ `source_hash` trùng, spec I/O Matrix
  "Trùng hash" — hoặc lỗi ghi DB khác): transaction rollback tự động (Rust
  `Drop` của `rusqlite::Transaction` khi closure trả `Err` trước khi gọi
  `commit()`), nên không có dòng `sessions`/`transcripts`/`segments` nửa vời
  nào tồn tại được. Nếu Proxy đã publish thành công ở bước 2,
  `commit_file_session` gỡ ngay `media/<session-id>/` (không đợi reconcile
  boot) — không mồ côi thư mục sau một lần retry hash trùng.
- **Chạy lại** (`replace_primary_transcript`): xoá primary cũ + chèn primary
  mới trong cùng một transaction; huỷ/lỗi giữa chừng giữ nguyên bản cũ vì cùng
  cơ chế rollback tự động — không chạm file media nào (retranscribe không tạo
  Proxy mới).

## Luật reconcile (idempotent, chạy sau `note_boot` mỗi lần khởi động)

Root cho reconcile là `app_data_dir` — cùng gốc `Db::open` dùng, chứa cả
`app.db` lẫn `media/`.

1. Xoá hẳn `media/.staging` — publish luôn xảy ra trước commit (quyết định ở
   trên) nên bất kỳ staging nào còn sót lúc boot đều là nháp thừa, bỏ được an
   toàn dù job đó đã publish xong Proxy hay chưa.
2. Với mỗi entry trực tiếp dưới `media/`:
   - Tên không parse được thành `SessionId` hợp lệ (không phải UUIDv7) → xoá
     hẳn (file/thư mục lạ, bao gồm cả `.staging` tái tạo bởi một tiến trình
     khác giữa bước 1 và bước này).
   - Tên là một `SessionId` hợp lệ nhưng **không có dòng `sessions` nào** →
     xoá hẳn thư mục (cửa sổ crash giữa publish và commit).
   - Tên là một `SessionId` có dòng `sessions` → giữ thư mục, xử lý tiếp bước
     3.
3. Với mỗi thư mục Phiên có dòng DB:
   - `sessions.proxy_ext` khác `NULL` nhưng `media/<sid>/proxy.<ext>` không
     còn tồn tại trên đĩa → null hoá `proxy_ext` qua
     `repo::sessions::set_proxy_ext` (DB tham chiếu Proxy đã mất).
   - Mọi entry khác trong thư mục đó, ngoài đúng tên Proxy hiện đang được
     tham chiếu (sau khi đã null hoá ở trên nếu cần) → xoá hẳn (file
     `.partial`/tên lạ sót lại từ một lần publish nửa vời).

Không bước nào ở trên xoá hoặc sửa một Phiên/Proxy đã commit thành công và
còn nguyên trên đĩa — reconcile chỉ dọn phần không còn được DB tham chiếu.
Chạy lại nhiều lần cho cùng kết quả: sau lần đầu không còn gì để dọn nên các
lần sau không đổi thêm gì (idempotent) — xác nhận bằng test tiêm lỗi tại từng
ranh giới (`library/store.rs` tests) rồi gọi `reconcile` hai lần liên tiếp.

## Evidence

- `src-tauri/src/library/store.rs` — cài đặt bốn bước và `reconcile`, cộng bộ
  test tiêm lỗi (`fault::Point::{PublishWrite,PublishRename,Commit,Discard}`)
  phủ từng ranh giới của I/O Matrix.
- `src-tauri/src/db/migrations/mod.rs` — migration 3 (`sessions`,
  `transcripts`, `segments`), CHECK/UNIQUE INDEX ép các bất biến schema mô tả
  ở trên (một `primary`/Phiên, `gap_reason` chỉ có mặt khi `kind = 'gap'`).
