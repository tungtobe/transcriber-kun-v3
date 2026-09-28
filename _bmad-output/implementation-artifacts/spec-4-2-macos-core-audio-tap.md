---
title: 'Story 4.2: Capture âm thanh hệ thống trên macOS bằng Core Audio tap'
type: 'feature'
created: '2026-09-28'
status: 'done'
baseline_commit: '2b8ba0f959ccde3511f1276f2ff96c0bcacabba7'
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

**Problem:** macOS hiện báo `system_available=false`, khiến Live không thu được âm thanh cuộc họp từ app native hoặc trình duyệt.

**Approach:** Gắn global Core Audio process tap vào capture port của 4.1; loại trừ process của app, chuẩn bị/đóng tap đúng vòng đời và xử lý quyền System Audio Recording.

## Boundaries & Constraints

**Always:** macOS ≥14.4, `CATapDescription` global tap loại trừ process AudioObjectID của app và `AudioHardwareCreateProcessTap`; thu qua aggregate device/IOProc, chuyển samples/format cho pipeline 4.1; system và mixed cùng mic hoạt động. Lần đầu phải thử mở tap để OS hiện prompt; lỗi thực tế trả `AppError` category `permission` kèm đường dẫn System Settings và cho phép thử lại. Thêm `NSAudioCaptureUsageDescription` và hướng dẫn ký dev build bằng cert team `B2U85XPU55`. Ghi ADR S4 về thiết kế, vòng đời, quyền, kết quả kiểm thử thực tế và phần chưa xác minh.

**Never:** ScreenCaptureKit, quyền Screen Recording, suy ra thiếu quyền từ silence, thu âm do chính app phát, sửa clock/mix của 4.1 nếu không có lỗi được chứng minh.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Mở lần đầu | chưa có quyết định TCC | mở tap/aggregate để OS prompt, audio nếu cấp quyền | từ chối thực tế → permission + Settings |
| System | app khác phát audio | PCM qua port 4.1; app tự phát không quay lại | lỗi tap/IOProc → lỗi nguồn, cleanup |
| Mixed | tap và mic đã mở | hai luồng trộn theo output clock của 4.1 | lỗi một nguồn không làm sập process |
| Không có âm | tap chạy, source im lặng | chunk silence hợp lệ | không báo sai lỗi quyền |

</frozen-after-approval>

## Code Map

- `src-tauri/src/audio/mod.rs` -- `CaptureBackend`, `SourceInput`, `PreparedStream`, `CaptureController::production`, pipeline/clock và tests của 4.1.
- `src-tauri/src/audio/cpal_backend.rs` -- mic discovery/prepare; trên macOS cần bổ sung system adapter và `system_available` theo OS, còn Windows giữ seam cho 4.3.
- `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` -- dependency macOS có điều kiện cho objc2/Core Audio APIs.
- `src-tauri/tauri.conf.json`, `src-tauri/tauri.appstore.conf.json` -- bundle metadata; overlay App Store hiện trống, 1.11 TestFlight chưa triển khai.
- `docs/adr/` và hướng dẫn dev -- nơi lưu ADR S4/cách ký cố định.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/audio/` -- macOS process tap, aggregate/IOProc, PID exclusion, sample callback và cleanup/idempotent start.
- [x] `src-tauri/src/audio/cpal_backend.rs` -- wire source system và mixed trên macOS; thông báo availability đúng OS.
- [x] `src-tauri/Cargo.toml`, bundle config -- pin bindings cần thiết và usage description System Audio/Microphone, không yêu cầu Screen Recording.
- [x] `docs/adr/` và dev docs -- S4 ADR, hướng dẫn ký team B2, retry sau Settings, checklist ba máy/TestFlight ghi trạng thái có bằng chứng.
- [x] `src-tauri/src/audio/` -- test điều kiện OS, source lifecycle, exclusion/error cleanup và mixed seam.

**Acceptance Criteria:**
- Given macOS ≥14.4, when chọn system, then tạo global tap loại trừ app và phát PCM qua capture port.
- Given TCC chưa quyết định, when mở tap, then hệ thống có cơ hội prompt; Given từ chối, then permission với Settings, không crash.
- Given mic cùng tap, when chọn mixed, then hai luồng cùng hoạt động mà không mở lại nguồn đã có.
- Given build sandbox/TestFlight và ≥3 máy phù hợp, when thử Zoom, Teams, Chrome Meet, then ADR S4 ghi kết quả thực; thiếu môi trường phải ghi rõ chưa nghiệm thu.

## Implementation Notes

- Tap global loại trừ Core Audio process ID của app, dùng aggregate private làm CPAL input; tài nguyên huỷ theo thứ tự aggregate rồi tap. `system_available` phụ thuộc macOS 14.4+.
- Đã qua 19 audio tests, `cargo check`, `npm run check`, kiểm tra plist/config và ký bundle cục bộ bằng identity team B2. TCC prompt/retry, ba máy với Zoom/Teams/Meet và sandbox/TestFlight chưa có bằng chứng; xem ADR S4.

## Spec Change Log

## Review Triage Log

## Design Notes

Apple mô tả tap → aggregate device → IOProc và yêu cầu `NSAudioCaptureUsageDescription`; permission prompt phát khi bắt đầu ghi aggregate, không phải khi chỉ liệt kê nguồn. Story 1.11 đang backlog, nên code/ADR có thể hoàn tất trước nhưng không tự nhận đạt nghiệm thu TestFlight ba máy.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml audio` -- tests mới và cũ qua trên macOS.
- `cargo check --manifest-path src-tauri/Cargo.toml` -- macOS Rust build qua.
- `npm run check` -- frontend/bindings qua.

**Manual checks (nếu có môi trường):**
- Build ký bằng cert team B2, kiểm tra entitlement/usage description và test TCC, Zoom, Teams, Meet theo matrix S4; điền bằng chứng vào ADR.
