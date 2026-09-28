---
title: 'Story 4.1: Nền capture audio — nguồn, mic, trộn và gate mềm'
type: 'feature'
created: '2026-09-28'
status: 'done'
baseline_commit: '8e79dc5134e4a3a9edd293d1cc49cd63ac9aba22'
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

**Problem:** Live capture chưa có nền chung cho mic, system và mixed, nên người dùng chưa thể đổi nguồn trong phiên mà giữ audio và đồng hồ liên tục.

**Approach:** Tạo capture port có nguồn giả, pipeline PCM16 mono 16 kHz theo chunk 100 ms, mic adapter và điều phối nguồn; để system adapter ở 4.2/4.3 gắn vào cùng port.

## Boundaries & Constraints

**Always:** Nguồn `system`, `mic:<name>`, `mixed:<mic>`; `live_sources` liệt kê mic, mic mặc định và khả dụng của system, hỗ trợ refresh; đổi nguồn hiệu lực ≤1 s, chuẩn bị nguồn mới trước swap và giữ nguồn cũ nếu lỗi; gate nguồn đã mở bằng silence; mixed cộng có resample, căn sample và giới hạn biên độ. Đồng hồ tính theo output samples, không theo wall clock. Lỗi thiết bị/quyền mic là `AppError` category `permission` kèm hướng dẫn.

**Never:** Mở mic khi chỉ chọn system; đếm gấp đôi samples khi mixed; giả định silence là lỗi quyền; triển khai system capture hoặc giao diện Live của story sau trong story này.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Chọn mic | mic có quyền | PCM16 mono 16 kHz, 1600 samples/chunk | lỗi mở thiết bị → permission có hướng dẫn |
| Đổi nguồn | system/mic/mixed liên tiếp | không rơi hoặc lặp output samples, clock tăng đơn điệu | nguồn mới không mở được → giữ nguồn cũ |
| Trộn | nguồn khác sample rate | căn theo output timeline, cộng và clamp | mất một nguồn → lỗi riêng, không phá clock |

</frozen-after-approval>

## Code Map

- `src-tauri/src/audio/mod.rs` -- placeholder, nơi đặt capture port, pipeline, mock, mic và nguồn system platform seam.
- `src-tauri/src/ipc/mod.rs` -- typed commands và `specta_builder()`, nơi thêm `live_sources` / `live_set_source`.
- `src-tauri/src/ipc/boot.rs` -- `AppState`, nơi giữ capture controller dùng chung.
- `src-tauri/src/core/error.rs` -- `Code::Permission` hiện có, tái dùng thay vì thêm taxonomy.
- `src-tauri/src/media/resample.rs` -- mẫu dùng Rubato; xem trước khi tái sử dụng vì đang private cho file decode.
- `src-tauri/Cargo.toml` -- thêm `cpal` 0.18; không thêm crate system capture trước 4.2/4.3.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/audio/mod.rs` -- định nghĩa nguồn/port, chunk/timeline, gate, mixing, resampling và source mock để kiểm thử.
- [x] `src-tauri/src/audio/` -- mic discovery/capture bằng cpal 0.18, permission guidance và platform seam cho system.
- [x] `src-tauri/src/ipc/mod.rs`, `src-tauri/src/ipc/boot.rs` -- thêm state/commands typed cho liệt kê và đổi nguồn; đăng ký bindings.
- [x] `src-tauri/Cargo.toml` -- pin dependency và cập nhật lockfile.
- [x] `src-tauri/src/audio/` -- test fake sources: chunk format, đổi liên tục, lỗi prepare, đồng hồ, mixed rate/clamp.

**Acceptance Criteria:**
- Given capture port có fake, when tạo ba loại nguồn, then output chung là PCM16 mono 16 kHz theo 1600 samples/chunk.
- Given danh sách thiết bị, when gọi/refresh `live_sources`, then trả mic mặc định, mic khác và system availability.
- Given mic có quyền, when mở capture, then nhận PCM; Given quyền/thiết bị lỗi, then `permission` có hướng dẫn.
- Given nguồn đang chạy, when đổi giữa system/mic/mixed, then có hiệu lực ≤1 s, clock liên tục; lỗi chuẩn bị giữ nguồn cũ.

## Implementation Notes

- CaptureController giữ stream đã mở theo input identity; output clock phát chunk 100 ms và giữ samples cũ trước khi đổi nguồn. System adapter chưa có ở story này.
- Đã kiểm tra `cargo test ... audio` (15 test), `cargo check`, `npm run check`, `cargo fmt --check` và `git diff --check`. Chưa kiểm tra mic trên thiết bị thật hoặc build Windows.

## Spec Change Log

## Review Triage Log

## Design Notes

System adapter chưa có ở 4.1: availability phải phản ánh thực tế adapter đăng ký, và lỗi chọn system trên platform chưa hỗ trợ phải giữ capture cũ. Output timeline là chủ sở hữu clock; gate chỉ thay đóng góp mẫu, không đổi số output samples.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml audio` -- fake source và pipeline tests qua.
- `cargo check --manifest-path src-tauri/Cargo.toml` -- build Rust qua.
- `npm run check` -- bindings/frontend checks qua nếu script hiện có.
