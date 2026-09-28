# Epic 4 Context: Live transcribe bền vững

<!-- Compiled from planning artifacts. Edit freely. Regenerate with compile-epic-context if planning docs change. -->

## Goal

Cho phép người dùng ghi và theo dõi transcript cuộc họp realtime từ âm thanh hệ thống, micro hoặc cả hai; mạng/model gặp lỗi không làm gián đoạn Recording, và phiên có thể phục hồi sau force-quit. Khi dừng, người dùng vào thẳng chi tiết phiên để phát, transcribe lại từ Recording và tải tệp. Không tìm thấy product brief trong planning artifacts; ngữ cảnh này dựa trên PRD, Architecture Spine và UX/Experience hiện có.

## Stories

- Story 4.1: Nền capture audio — nguồn, mic, trộn và gate mềm
- Story 4.2: Capture âm thanh hệ thống trên macOS (Core Audio tap, spike S4)
- Story 4.3: Capture âm thanh hệ thống trên Windows (WASAPI loopback kép)
- Story 4.4: Gemini Live — transport WebSocket, resumption và reconnect (spike S3)
- Story 4.5: Recording bền và tạo Phiên live khi mở capture
- Story 4.6: LiveSession — transcript realtime và trạng thái
- Story 4.7: Màn Live — bắt đầu và đang ghi
- Story 4.8: Live — mất kết nối, lỗi setup và trạng thái lỗi trên giao diện
- Story 4.9: Dừng Live và lưu Phiên
- Story 4.10: Boot và đóng cửa sổ — phục hồi phiên mồ côi, chặn thoát
- Story 4.11: Transcribe lại từ Recording và xem cạnh bản live
- Story 4.12: Tải Recording WAV hoặc FLAC

## Requirements & Constraints

- Capture `system`, `mic:<name>` hoặc `mixed:<mic>`; đổi nguồn có hiệu lực trong ≤1 s và không đặt lại đồng hồ. Hỗ trợ âm thanh cuộc họp native/trình duyệt trên macOS/Windows. macOS chỉ xin quyền System Audio Recording và Microphone, không xin Screen Recording; lỗi quyền phải có hướng dẫn.
- Hiện transcript trong ≤2 s sau khi kết câu ở mạng bình thường. Reconnect vô hạn khi phiên chạy, backoff có jitter tới tối đa 30 s; phát lại audio chưa được xác nhận và giữ tối đa 60 s audio. Phần vượt ngưỡng thành gap có timestamp. Sau 5 lần liên tiếp bị từ chối setup, cho chọn chỉ ghi âm hoặc dừng.
- Recording WAV trong Container không phụ thuộc mạng/Gemini/key; cập nhật header định kỳ để force-quit chỉ mất tối đa 5 s cuối. Chỉ Dừng hoặc lỗi thiết bị audio mới dừng ghi. Phiên bắt đầu sau khi mở capture, kể cả offline; lỗi mở capture phải dọn tệp rỗng. Nếu lỗi lưu trữ ngăn ghi tiếp, dừng an toàn, giữ bytes đã lưu và báo lỗi storage (làm rõ Epic cần đồng bộ vào PRD).
- Boot phục hồi phiên chưa finalize cùng Recording/segments, tạo Proxy nếu thiếu và không chặn Home. Dừng finalize Recording/Proxy rồi lưu phiên. Transcribe lại giữ bản live, Ghi chú, Tag, Memo; chỉ tải WAV/FLAC.
- Không telemetry hoặc log nội dung họp/key. Gemini đi qua cổng có Consent hiện hành, ưu tiên quota Live. Lỗi có category và hướng xử lý, không lộ key/URL/stack trace. RAM khi Live ≤300 MB.

## Technical Decisions

- Rust sở hữu state/DB trong một modular monolith; WebView chỉ render và gửi lệnh, `ipc/` là nơi duy nhất điều phối qua feature. `LiveSession` là tokio actor; audio source và Gemini transport là port/trait có bản giả.
- Chuẩn hoá capture thành PCM16 mono 16 kHz, chunk 100 ms. Fan-out không chặn capture sang WAV writer và WebSocket sender; WS có ring buffer 60 s. Tràn buffer thành gap `disconnected`. Restart tạo `LiveGeneration`; bỏ event cũ và drain generation trước trong ≤1 s.
- `db/` sở hữu một kết nối SQLite WAL; flush segment định kỳ và dùng DB segments làm nguồn phục hồi. Trạng thái session: `recording → finalizing → complete`; boot hoàn tất session còn dở với `recovered=true`. Dùng UUIDv7 và lưu Recording/Proxy trong Container theo ID phiên.
- Đồng clock theo tổng sample/sample rate; segment timestamp là giây tuyệt đối, không dùng wall clock. UI nhận snapshot + delta `seq` đơn điệu qua generation; hụt seq thì subscribe lại. Live dùng transcript `primary`; Transcribe lại tạo `retranscribe`. Tag/Ghi chú/Memo gắn với session. Job dùng hàng đợi file tuần tự, chạy song song với tối đa một LiveSession. Boot/close do `ipc/` điều phối.

## UX & Interaction Patterns

- LiveSetup chọn nguồn/mic/Tag và xử lý quyền; thiếu quyền có banner và nút mở System Settings.
- Live có hai pill riêng: Recording/đồng hồ và trạng thái kết nối (transcribe, reconnect kèm thời gian, hoặc transcript dừng). Không báo đang transcribe nếu chưa có kết nối. Gap xuất hiện inline theo thời gian.
- Transcript tự cuộn; cuộn lên thì dừng và hiện nút về nội dung mới nhất. Panel Ghi chú mở mặc định và tự lưu.
- Dừng → overlay lưu → `/session/:id`; side-by-side cho bản live và bản transcribe lại. Home menu phiên live tải WAV/FLAC qua dialog hệ thống. Đóng app khi ghi cần xác nhận và finalize; phiên mồ côi tự xuất hiện trong Home với badge Phục hồi.

## Cross-Story Dependencies

- Phase 0 chạy spike S3 (Live WS 60 phút) và S4 (Core Audio tap trong sandbox, ≥3 máy; dùng harness S5). Capture 4.1–4.3 cấp audio cho Recording/WS; transport 4.4 phục vụ LiveSession 4.6/4.8.
- 4.5 cấp Recording cho finalize 4.9 và recovery 4.10 qua DB/`ipc/`. 4.11 dùng queue, Proxy và Transcript detail của Epic 2 cùng Ghi chú/Tag của Epic 3; 4.12 dùng Home của Epic 2.
- Epic 1 cung cấp Consent, Gemini gateway, settings, error model, DB, IPC và logging. Epic 5 tiếp tục dùng `LiveSession`/`LiveGeneration` cho dịch và TTS.
