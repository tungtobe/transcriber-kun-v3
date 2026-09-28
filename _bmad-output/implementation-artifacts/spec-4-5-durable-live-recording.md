---
title: 'Story 4.5: Recording bền và tạo Phiên live khi mở capture'
type: 'feature'
created: '2026-09-28'
status: 'done'
baseline_commit: '1b843b89a8d42316baafdd136a53d7ff6ad9002d'
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

**Problem:** Live hiện chưa ghi Recording hay tạo Phiên khi capture mở; lỗi mạng có thể khiến người dùng mất buổi họp. **Approach:** Tạo recording service sở hữu WAV và dòng `sessions`, tiêu thụ capture ở nhánh riêng, vá header định kỳ, và cấp trạng thái/lỗi cho LiveSession ở 4.6.

## Boundaries & Constraints

**Always:** Chỉ khởi tạo phiên sau khi thiết bị capture mở; tạo ngay `kind=live,status=recording` và `media/<id>/recording.wav` kể cả offline. WAV PCM16 LE mono 16 kHz; vá header và flush ít nhất mỗi 160,000 byte/5 giây để force-quit phát được phần đã chốt, mất tối đa 5 giây cuối. Writer riêng không phụ thuộc WS; network/model/key errors không dừng ghi. Tên mặc định là thời gian địa phương theo ngôn ngữ UI; tên file chỉ ID tự sinh. Trước khi thiết bị mở lỗi thì không có row/file; sau khi mở nhưng khởi tạo file/DB thất bại cũng dọn header-only/row. Khi writer lỗi, disk full, lag hoặc sample discontinuity khiến audio không còn bền: dừng capture, phát lỗi category `storage`, bảo toàn bytes đã ghi và giữ phiên cho finalize/recovery; không tiếp tục báo Recording. `source_errors` từ capture là lỗi thiết bị, dừng an toàn. Consent/key gate không bắt buộc kiểm tra online để bắt đầu offline.

**Never:** Cho writer chạy qua task WebSocket hoặc chặn callback capture trên I/O đĩa; che giấu sample đã mất; xóa Recording của phiên live đã có row trong boot reconcile; mở chế độ ghi không key/consent ngoài phạm vi.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Capture chưa mở | lỗi thiết bị | không row, không WAV/header-only | trả lỗi thiết bị và dọn |
| Bắt đầu offline | capture mở, consent/key hợp lệ | row `live/recording`, WAV dưới ID, writer nhận chunks | không list-model/key-test online |
| Ghi và force-quit | >5 s PCM liên tục | header phát được tới checkpoint gần nhất; ≤5 s cuối mất | kill-process test thực |
| WS/mạng hỏng | capture liên tục | WAV đủ samples, writer tiến độc lập | WS không tác động writer |
| Storage hỏng/consumer lag | write/flush lỗi hoặc mất chunks | dừng capture, category storage, giữ bytes/row đã bền | không báo tiếp recording |
| Reconcile boot | phiên live đang/đã ghi | `recording.wav` được giữ | entry lạ vẫn dọn |

</frozen-after-approval>

## Code Map

- `src-tauri/src/audio/mod.rs` -- `CaptureController::set_source/subscribe`, `PcmChunk` 100 ms/1600 samples; broadcast chỉ 32 chunks, cần xử lý Lagged và API stop capture (khóa `source_switch`, drop toàn bộ `opened`, ignore callback generation cũ).
- `src-tauri/src/db/repo/sessions.rs`, `src-tauri/src/db/mod.rs` -- `NewSession`, single WAL connection; DB SQL chỉ ở repo.
- `src-tauri/src/core/paths.rs`, `src-tauri/src/core/id.rs` -- safe SessionId path; thêm fixed recording path.
- `src-tauri/src/library/store.rs` -- `reconcile_session_dir` hiện xóa mọi file trừ proxy, cần giữ Recording cho live row.
- `src-tauri/src/settings/mod.rs` -- `UiLanguage`; system language cần resolve từ UI hoặc system locale rõ ràng, không đoán từ transcribe language.
- `src-tauri/src/live/mod.rs` -- thêm recording submodule/service để actor 4.6 dùng; không xây actor/IPC transcript ở story này.
- `src-tauri/src/core/error.rs` -- `Code::Storage` mapping.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/live/recording.rs` -- lifecycle start/write/checkpoint/stop/failure; độc lập WS, trả handle và thông báo terminal error cho 4.6.
- [x] `src-tauri/src/audio/mod.rs`, `src-tauri/src/core/paths.rs` -- stop/release capture an toàn và recording path cố định.
- [x] `src-tauri/src/db/repo/sessions.rs`, `src-tauri/src/library/store.rs` -- tạo/cleanup session đúng transaction và bảo toàn Recording qua reconcile.
- [x] `_bmad-output/planning-artifacts/prds/prd-transcriber_kun-2026-09-17/` -- đồng bộ ngoại lệ lỗi lưu trữ vật lý của FR-23 (C8), không sửa nghĩa lỗi mạng.
- [x] Tests covering all matrix rows, gồm process kill thật sau checkpoint và fault injection write/lag, kiểm tra RIFF bằng reader độc lập.

**Acceptance Criteria:**
- Given capture mở thành công, when start Recording, then UI/actor nhận session ID và durable file riêng trước khi nối WS.
- Given lỗi storage, when không thể ghi tiếp, then capture dừng và phiên chuyển sang luồng finalize/recovery, bytes đã lưu được giữ.

## Implementation Notes

- Recording worker chạy trong thread độc lập; 160 KB hoặc 5 giây thì flush header và sync file. Kill-process test còn hai chunk chưa checkpoint, xác nhận reader mới vẫn phát phần đã chốt.
- `RecordingHandle::terminal_errors()` cung cấp lỗi để LiveSession 4.6 chuyển UI khỏi trạng thái đang ghi và đi finalize/recovery. `start_with_locale` nhận ngôn ngữ UI thực từ IPC khi setting là System.
- Matrix audit: 11 test recording, 53 test library/store, 16 test audio qua. Bài test offline xác nhận recording không cần transport; gate consent/key ở `live_start` thuộc 4.6. Review selection `none` theo workflow đã render.

## Spec Change Log

## Review Triage Log

## Design Notes

Hound đã là dependency; `WavWriter::flush()` cập nhật header mà không đóng writer. Kiểm chứng bằng kill process thực và reader mới thay vì chỉ test gọi flush. `reconcile` phải biết `kind=live` từ DB, không giữ tên `recording.wav` trong thư mục phiên file.

## Verification

**Commands:**
- `cargo test --locked --manifest-path src-tauri/Cargo.toml live::recording` -- matrix và kill-process tests qua.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml library::store` -- reconcile regression qua.
- `cargo check --locked --manifest-path src-tauri/Cargo.toml` -- Rust build qua.
- `npm run check` -- frontend check qua.
