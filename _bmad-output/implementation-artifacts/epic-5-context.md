# Epic 5 Context: Dịch realtime, TTS & Nhận diện lại

<!-- Compiled from planning artifacts. Edit freely. Regenerate with compile-epic-context if planning docs change. -->

## Goal

Cho người dùng đọc bản dịch song song ngay trong họp, đổi ngôn ngữ đích (Target) hoặc chọn "Không dịch" giữa phiên mà không ngắt phiên, bấm "Nhận diện lại" khi khách đổi ngôn ngữ, và nghe bản dịch qua loa với volume hệ thống tự hạ (Ducking) rồi phục hồi. Epic dựa trên `LiveSession`/`LiveGeneration` của Epic 4. Không tìm thấy product brief trong planning artifacts; ngữ cảnh này dựa trên PRD, Architecture Spine và UX/Experience hiện có.

## Stories

- Story 5.1: Dịch realtime song song và đổi Target giữa phiên
- Story 5.2: Nhận diện lại ngôn ngữ
- Story 5.3: Đọc bản dịch (TTS) trong tiến trình app
- Story 5.4: Ducking volume hệ thống

## Requirements & Constraints

- Bản dịch chạy song song transcript gốc; đổi Target hoặc tắt dịch không ngắt phiên, không làm gián đoạn transcript gốc, không Segment lặp, timestamp không nhảy. Bản dịch mới bắt đầu từ câu kế tiếp. Target được chọn thì cột Dịch luôn hiện nội dung kể cả khi lời nói đã ở ngôn ngữ đích; điều này không tự bật TTS.
- Chỉ transcript gốc được lưu; Bản dịch không nằm trong `segments` (cần test khẳng định). Muốn bản dịch bền thì Transcribe lại rồi dịch bản đó.
- "Không dịch": view chỉ còn cột Gốc, TTS vô hiệu, mọi audio đầu ra lỡ nhận về bị bỏ. Chỉ hiện dòng "không tốn token cho dịch" khi spike S3 đã xác nhận với model thật; nếu model vẫn bắt buộc sinh audio thì chặn nghiệm thu và cập nhật quyết định nguồn.
- "Nhận diện lại" chỉ dùng được khi ngôn ngữ transcribe là `auto`. Kết nối mới với ngữ cảnh trống; nếu thất bại giữ kết nối cũ, banner cảnh báo nhẹ tại chỗ, phiên không gián đoạn. Đóng kết nối cũ ≤ 1 s sau khi kết nối mới sẵn sàng.
- TTS chỉ dùng audio đầu ra của model (PCM16 24 kHz mono), không thêm TTS local/OS/cloud nào (kiểm tra manifest). Tắt toggle loa chỉ bỏ việc phát, không giảm token dịch (tooltip phải nói rõ). Đổi thiết bị output phát tiếp ≤ 2 s; stall 2.5 s coi là thiết bị chết; Bluetooth tắt đột ngột chuyển ≤ 3 s. Bị ngắt lời (`audio_interrupted`) thì dừng phát và dọn buffer ngay.
- TTS không được bị thu lại: macOS loại trừ PID của app khỏi tap; Windows phải có cơ chế đã được spike chứng minh (Win 10 1809 và Win 11) mà vẫn thu đủ lời họp gốc, không tắt toàn bộ system capture để vượt test. Nghiệm thu: transcript gốc không chứa bản đọc lặp.
- Ducking hạ volume hệ thống còn 30 % khi model nói, phục hồi khi im. Người dùng tự chỉnh volume lúc đang duck thì bỏ mức gốc, giữ mức người dùng. Crash/force-quit khi đang duck thì lần mở sau phục hồi đúng device nếu marker còn hiệu lực; không ép volume cũ lên thiết bị khác.
- Lỗi model preview (category `model`) dùng lại banner tại chỗ. Không log nội dung transcript/bản dịch.

## Technical Decisions

- Chế độ dịch: setup có `outputAudioTranscription` ở top-level và `translationConfig {targetLanguageCode, echoTargetLanguage: true}`; xử lý sự kiện `delta_translated`, `segment_translated`. "Không dịch": không có `outputAudioTranscription`, target = ngôn ngữ nguồn (hoặc `ja` khi `auto`), không có `echoTargetLanguage`. Payload là ứng viên từ nguồn, cần kiểm chứng ở S3 (model ID, capability, hành vi `goAway`, resumption, ngắt lời, có sinh audio hay không); test khoá exact JSON shape.
- Đổi Target và Nhận diện lại đều là `LiveGeneration` mới: chờ `setupComplete`, swap sender dưới gate, drain generation cũ ≤ 1 s, bỏ event mang id generation cũ. Đồng hồ theo tổng sample nên timestamp không nhảy. `LiveSession` tuần tự hoá chuyển đổi (Target liên tiếp, Nhận diện lại, Dừng): chỉ lựa chọn mới nhất được swap, thất bại giữ generation/Target cũ và trả dropdown về giá trị thực, Dừng huỷ mọi candidate, deadline setup chốt ở S3. Tắt dịch/TTS dọn audio chờ và phục hồi Ducking.
- TTS: `audio::playback` (cpal) giải mã base64 và phát trực tiếp trong tiến trình Rust, không qua IPC; UI chỉ nhận `speaking: bool`. Có supervisor thiết bị output (poll 1 s). Test dùng transport giả.
- Ducking: `audio/output_volume` (port từ v2) do `audio/` sở hữu, backend volume giả lập nội bộ cho test, không thêm port kiến trúc thứ tư. Ghi bền marker `state/ducking.json` (device ID, mức gốc, mức app áp, hiệu lực phục hồi) trước khi đổi volume; lượt nói chồng nhau không ghi đè mức gốc bằng mức đã duck; chỉnh tay vô hiệu hoá marker; đổi output tạo baseline mới cho thiết bị mới.
- Boot là một routine duy nhất trong `ipc/`: migrate DB → phục hồi volume từ marker Ducking → dọn `media/.staging` → phục hồi phiên mồ côi. Không tạo boot handler thứ hai.
- Settings có nhóm Live với Target mặc định; LiveSetup có select "Dịch sang" gồm các ngôn ngữ và "Không dịch". Lỗi qua IPC dùng `AppError` với category ổn định.

## UX & Interaction Patterns

- Segmented "Gốc · Dịch · Cả hai"; hai cột "Gốc (ja · auto)" và "Dịch (→ vi)"; chế độ Gốc hoặc Dịch chỉ còn một cột. Ngôn ngữ UI, ngôn ngữ transcribe và Target là ba khái niệm độc lập, một phiên có thể trộn ja/vi/en.
- Dropdown Target đổi giữa phiên; thất bại thì trả về giá trị thực.
- Toggle loa TTS dùng `aria-pressed`, vô hiệu kèm tooltip khi "Không dịch". Pill "Đang đọc" nhỏ ở đầu cột Dịch bật/tắt theo lúc TTS thực sự phát. Badge "Tốn token Gemini" theo quy ước cho hành động tốn token.
- Nút "Nhận diện lại" hiện khi ngôn ngữ là `auto`, ẩn hoặc vô hiệu khi khác. Lỗi hiện inline gần nơi xảy ra, không dùng toast.

## Cross-Story Dependencies

- 5.1 phụ thuộc 4.8 (banner lỗi tại chỗ, `LiveGeneration`, transport Live 4.4/4.6); 5.2 phụ thuộc 5.1.
- 5.3 phụ thuộc 5.1 và capture system của 4.2 (macOS loại trừ PID) và 4.3 (Windows loopback); 5.4 phụ thuộc 5.3 và routine boot của 4.10.
- Spike S3 (Live WS, hành vi model thật) quyết định nghiệm thu 5.1 và 5.3. Spike Windows Phase 0 quyết định cơ chế chống thu lại TTS.
- Epic 1 cung cấp settings, error model, gateway Gemini; Epic 4 cung cấp `LiveSession`, snapshot + `seq`, Recording và boot.
