# Store & compliance — trans-kun v3

Ràng buộc nền tảng, privacy và review store mà mọi epic phải tôn trọng. Chi tiết build, chứng chỉ và pipeline CI: `addendum.md` §J, `ARCHITECTURE-SPINE.md` AD-17.

## Nền tảng

| | macOS | Windows |
|---|---|---|
| Phiên bản | ≥ 14.4 (Q3) | 10 1809+ và 11 |
| Kiến trúc | universal (Apple Silicon + Intel) | x64; Arm64 chỉ khi có máy test thật trước Phase 6 (Q7), nếu không có thì bổ sung sau |
| Phân phối | Mac App Store | Microsoft Store (MSIX) |
| System audio | Core Audio process tap global, loại trừ PID của app | WASAPI loopback endpoint console + communications |
| WebView | WKWebView | WebView2 Evergreen: kiểm tra runtime lúc chạy, hướng dẫn cài nếu thiếu |
| Beta | TestFlight for Mac | Package flight |

Bundle ID `com.transkun.app`. Không có Linux, kênh tải trực tiếp hay tự cập nhật.

## Sandbox / capability

- **macOS:** app-sandbox, network.client, device.audio-input, files.user-selected.read-write; keychain-access-groups nếu `keyring` cần trong sandbox (spike S5). Usage description cho `NSAudioCaptureUsageDescription` và `NSMicrophoneUsageDescription`. Binary phải ký; TCC khoá theo Team ID nên dev build cũng ký bằng cert team.
- **Windows:** package identity MSIX, capability `microphone`, WACK pass.
- Muốn thêm entitlement, capability hay plugin thì phải sửa architecture spine trước.

## Privacy

- Consent trước mọi request tới Google (FR-2).
- Privacy Policy công khai bằng vi/en/ja, có link trong app (Consent, About, Settings) và trong hồ sơ store.
- Privacy label: Audio Data / User Content gửi tới Google để cung cấp tính năng, không liên kết danh tính, không tracking. Advertising Data: none. Không cần ATT.
- Lập trường GDPR/CCPA: chúng ta không thu thập hay lưu dữ liệu cá nhân nào (không tài khoản, không telemetry). Người dùng gửi dữ liệu họp tới Google theo điều khoản Gemini API của chính họ. Privacy Policy nêu cách xoá dữ liệu cục bộ (FR-40).
- Nội dung an toàn: Markdown của Memo được sanitize; Creative chỉ gồm ảnh + text tĩnh, không HTML/script từ server.

## Apple

- App Sandbox (2.4.5); không tải/chạy code (2.5.2); không self-update.
- Mọi mở khoá trả phí phải qua IAP (3.1.1). v3 không có mở khoá trả phí.
- Quảng cáo có nút báo cáo và phần giải thích (2.5.18).
- Privacy Policy có trong app và trong App Store Connect (5.1.1); consent cho AI bên thứ ba (5.1.2).
- `ITSAppUsesNonExemptEncryption=false`.
- App Review Notes gồm key demo và 3 bước dùng thử (UJ-5; rủi ro ở Q4).
- Dự phòng nếu Apple từ chối BYOK: `addendum.md` §K (ephemeral token qua `KeyProvider`). Không triển khai trừ khi phương án này được kích hoạt.

## Microsoft

- MSIX nộp qua Partner Center, Store ký. Tuân chính sách quảng cáo 10.x. Có Privacy Policy. Notes for certification có key demo. WACK pass.

## Hồ sơ submit (cả hai store)

Tên app, category Productivity, age rating 4+, screenshot theo kích thước từng store, mô tả vi/en/ja, Privacy Policy URL, support URL, review notes + key demo. Nội dung app không nhắc tới kênh tải ngoài store hay license riêng.
