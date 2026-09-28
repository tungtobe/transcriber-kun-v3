---
title: 'Story 4.3: Capture âm thanh hệ thống trên Windows bằng WASAPI loopback kép'
type: 'feature'
created: '2026-09-28'
status: 'done'
baseline_commit: 'e7c78df6d4d9bc80ffd57e1d2c69d614f74439da'
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

**Problem:** Windows hiện chỉ thu mic; audio từ Zoom/Teams native hoặc Meet trong Chrome không vào nguồn system của Live.

**Approach:** Thêm WASAPI loopback cho endpoint render mặc định của hai role console và communications, gộp thành một nguồn system cho pipeline 4.1; giữ mic qua CPAL cho mixed.

## Boundaries & Constraints

**Always:** Windows 10 1809+/11; lấy default render endpoint của cả `Console` và `Communications`; so device ID để tránh thu đôi khi trùng; WASAPI shared loopback qua crate `wasapi`; output một `InputSide::System` từ backend, PCM/clock cuối do 4.1 sở hữu. Mất/đổi endpoint phải tự thích ứng hoặc báo lỗi thiết bị riêng mà không làm dừng nguồn mic/Recording. Mic bị chặn trả `AppError` `permission` với đường dẫn Windows Settings đúng. Không cần admin hay capability system-audio riêng trong MSIX; capability `microphone` dành cho mic. Ghi checklist kiểm thử thực tế và trạng thái chưa kiểm chứng.

**Never:** Trả hai `PreparedInput` cùng `InputSide::System` vào pipeline 4.1; cộng trùng một endpoint; nhầm lời dẫn macOS vào lỗi Windows; coi compile/fake tests là bằng chứng Zoom/Teams/Meet hay MSIX hoạt động trên máy thật.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Hai role khác thiết bị | console + communications phát | cả hai đóng góp đúng một lần, một logical system input | lỗi một endpoint được báo riêng, không panic |
| Hai role cùng thiết bị | cùng endpoint ID | chỉ mở một loopback, không nhân đôi biên độ | lỗi mở giữ nguồn cũ |
| Đổi default output | thiết bị đổi trong phiên | cập nhật capture hoặc lỗi device rõ ràng; mic vẫn có thể tiếp tục | không dừng toàn bộ controller |
| Mic bị chặn | chọn `mic`/`mixed` | permission với hướng dẫn mở Microphone Settings | system-only không mở mic |

</frozen-after-approval>

## Code Map

- `src-tauri/src/audio/mod.rs` -- capture port 4.1; một logical `InputSide::System`, output clock/gate/mix; thông báo lỗi đang có chuỗi chỉ macOS cần tách theo OS.
- `src-tauri/src/audio/cpal_backend.rs` -- production backend: Windows `system_available=false`/`system_unavailable_error`, mic CPAL; wire adapter Windows và permission guidance.
- `src-tauri/src/audio/macos_tap.rs` -- 4.2 adapter chỉ macOS, giữ nguyên.
- `src-tauri/Cargo.toml`, `Cargo.lock` -- thêm `wasapi` target Windows; dùng đúng API phiên bản được pin.
- `src-tauri/tauri.msix.conf.json` -- overlay MSIX hiện trống, story 1.12 còn backlog; không tự nhận package identity đã kiểm chứng.
- `.github/workflows/test.yml` -- Windows CI build/test hiện có; có thể dùng để xác minh sau commit nhưng không có máy Windows cục bộ.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/audio/` -- adapter Windows quản lý hai WASAPI loopback, dedupe endpoint, gộp/resample/căn PCM và vòng đời nguồn.
- [x] `src-tauri/src/audio/cpal_backend.rs` -- wire adapter vào system/mixed và nguồn khả dụng, không ảnh hưởng macOS/mic.
- [x] `src-tauri/src/audio/mod.rs` -- lỗi thiết bị/quyền theo OS, đảm bảo mic còn chạy khi system endpoint lỗi.
- [x] `src-tauri/Cargo.toml` -- pin crate; cấu hình và kiểm thử package MSIX còn phụ thuộc story 1.12.
- [x] `src-tauri/src/audio/`, `docs/` -- test giả cho role/dedupe/failure/switch và checklist thực tế Zoom/Teams/Meet/MSIX.

**Acceptance Criteria:**
- Given hai default render endpoint khác nhau, when system capture chạy, then một logical stream cộng hai endpoint đúng một lần.
- Given hai role cùng ID, when capture chạy, then chỉ mở một loopback.
- Given mic bị từ chối, when chọn nguồn có mic, then permission chỉ đúng trang Windows Settings; system-only không mở mic.
- Given output default đổi, when capture đang chạy, then thích ứng hoặc lỗi nguồn rõ và mic tiếp tục.
- Given máy Windows sạch và package MSIX, when thu Zoom/Teams/Chrome Meet, then checklist ghi bằng chứng thật; nếu chưa chạy, ghi rõ chưa nghiệm thu.

## Implementation Notes

- WASAPI worker mở default render endpoints của Console và Communications, dedupe theo ID, gộp theo QPC timestamp vào một system input. Handshake yêu cầu ít nhất một endpoint mở trước khi `set_source` đổi nguồn; endpoint thay đổi được refresh trong lúc chạy.
- 33 audio tests, `cargo fmt --check`, `npm run check` và type check riêng module WASAPI cho Windows target qua. Full app Windows target check bị chặn bởi `aws-lc-sys` cần Windows SDK `windows.h` trên máy macOS; kiểm thử máy Windows/MSIX/Zoom/Teams/Meet chưa chạy, xem checklist.

## Spec Change Log

## Review Triage Log

## Design Notes

`CaptureBackend::prepare` trả một prepared input cho `system`; adapter Windows nội bộ gộp hai endpoints. `wasapi::DeviceEnumerator::get_default_device_for_role` và `Device::get_id()` là giao diện role/device cần dùng; loopback phát từ render endpoint theo WASAPI shared mode. Context7 không có ID phù hợp cho Rust `wasapi`; đối chiếu trực tiếp docs.rs và Microsoft Learn khi triển khai.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml audio` -- tests portable hiện có qua.
- `cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc` -- Windows target check nếu toolchain/link deps sẵn có.
- `npm run check` -- frontend/bindings qua.

**Manual checks (khi có Windows):**
- Test package MSIX trên Windows 10 1809+/11 x64 (Arm64 nếu có), Zoom/Teams/Meet, mic denial và đổi output device; ghi kết quả checklist.
