# Epic 2 Context: Transcribe file & xem kết quả

<!-- Compiled from planning artifacts. Edit freely. Regenerate with compile-epic-context if planning docs change. -->

## Goal

Cho phép người dùng nhập file ghi âm/video và nhận transcript có timestamp, nghe lại từ proxy trong app, tìm kiếm, export/copy, đồng thời thấy và chạy lại rõ ràng các đoạn transcribe thất bại. Phiên đã tạo không phụ thuộc file nguồn; mở lại file trùng nội dung không gửi Gemini và không tốn token. Home xuất hiện trong epic này. Không tìm thấy product brief riêng; phần dưới đây được chắt lọc từ PRD, kiến trúc và UX.

## Stories

- Story 2.1: Media pipeline Rust và kiểm chứng phát FLAC
- Story 2.2: Gemini adapter theo họ model và parser
- Story 2.3: Lưu Phiên và Transcript bền vững
- Story 2.4: Job transcribe file, hàng đợi, tiến độ và huỷ
- Story 2.5: Lỗi Chunk, Khoảng thiếu và Chạy lại
- Story 2.6: Settings Chunking, offset và ngôn ngữ
- Story 2.7: Transcript detail, trình phát và click-to-seek
- Story 2.8: Nhận file, kiểm tra định dạng và phát hiện trùng
- Story 2.9: Home, danh sách Phiên và card Job
- Story 2.10: Tìm transcript, export và copy

## Requirements & Constraints

- Nhận file qua dialog hoặc kéo thả ở Home/Transcript; nhiều file xử lý tuần tự, mỗi file tạo một Phiên. Từ chối định dạng/codec không hỗ trợ trước khi tạo Phiên và nêu cách chuyển đổi phù hợp. Hash SHA-256 nội dung để mở Phiên có sẵn khi trùng, không gọi Gemini.
- Hỗ trợ các định dạng audio mp3, m4a, wav, flac, ogg/vorbis, aiff, caf và video mp4, mov, mkv, webm (chỉ track audio). Hỗ trợ Opus trong webm/mkv còn phụ thuộc kết quả spike S1.
- Mọi Phiên có proxy playback trong Container để phát, seek và chạy lại mà không cần file nguồn. Lỗi/mất proxy không được làm hỏng Transcript.
- Transcript dùng Segment thời gian tuyệt đối, không lùi thời gian; offset chỉ áp dụng khi hiển thị/export. Chunk lỗi phải hiện thành Khoảng thiếu, Transcript mang trạng thái partial đến khi được lấp; cho chạy lại phần thiếu hoặc toàn bộ và giữ nguyên kết quả cũ nếu lượt chạy mới bị huỷ/lỗi.
- Job báo tiến độ theo thời lượng audio, có thể huỷ, và tiếp tục hiển thị khi người dùng rời màn. Không gửi Chunk mới quá 2 giây sau khi huỷ; Job không chạy nền sau khi app thoát. Mọi lỗi có category và hành động dễ hiểu, không lộ key, URL hay stack trace.
- Decode/cắt Chunk đạt ít nhất 20× realtime trên máy 4 nhân; Home mở trong 2 giây, 500 Phiên hiển thị trong 1 giây, tìm trong transcript khoảng 700 Segment trong 100 ms, seek trong file 90 phút trong 500 ms. Timestamp so với transcript v2 cùng model lệch không quá 2 giây. Các ngưỡng gắn `[ASSUMPTION]` phải được kiểm chứng theo nguồn.
- Chunk gửi inline tới Gemini theo NFR-8; không upload trung gian hay thêm Files API nếu chưa có quyết định sản phẩm/kiến trúc. Dữ liệu họp chỉ gửi tới Google Gemini bằng key của người dùng; không telemetry.

## Technical Decisions

- Media decode, resample, chunk, hash và proxy chạy bằng Rust trong một tiến trình, không phụ thuộc ffmpeg/binary ngoài. Adapter chọn giao thức theo họ model nhưng chuẩn hoá về cùng dạng Segment; parser phải giữ phần Segment hợp lệ từ response hỏng mà không coi phần không xác định là thành công.
- `JobRegistry` là actor duy nhất có một hàng đợi tuần tự cho transcribe/chạy lại; state Job ở bộ nhớ, không khôi phục Job sau restart. Gemini đi qua cổng chung có Consent, quota priority, retry/timeout và phân loại lỗi tập trung.
- `db/` là chủ ghi SQLite duy nhất. Dùng UUIDv7 cho ID và tên file; staging theo Job, rồi commit dữ liệu Phiên/Transcript cùng publish proxy theo quy trình phục hồi sau crash. Partial tương ứng với gap `chunk_failed`; chạy lại dựng bản mới rồi swap nguyên tử.
- WebView chỉ đọc media qua asset protocol giới hạn ở `$APPDATA/media/**`. Đồng bộ Job bằng snapshot + event có `seq`; log không chứa transcript hoặc bí mật.

## UX & Interaction Patterns

- Home đồng thời là thư viện: trạng thái trống có hướng dẫn chọn/kéo file; khi có Phiên, drop-zone thu gọn phía trên danh sách. Có thể thả file ở bất kỳ đâu trên màn. Job đang chạy hiện ở đầu Home và thu gọn trong sidebar khi rời màn.
- Dòng Phiên thể hiện tên, ngày giờ địa phương, loại file/live, thời lượng và badge thiếu/phục hồi; theo quyết định UX/epic, không hiển thị Model, số Segment hoặc cột Trạng thái. Click dòng mở Transcript detail.
- Transcript detail hiển thị Segment cùng timestamp `HH:MM:SS` (bỏ giờ nếu dưới một giờ), speaker ẩn; click Segment seek tới đầu đoạn và đoạn đang phát được highlight. Khoảng thiếu/mất kết nối nằm đúng vị trí trong luồng; partial có cảnh báo cùng thao tác chạy lại.
- Tìm transcript có số kết quả, highlight, cuộn tới match và điều khiển prev/next. Export `.txt`, `.srt`, `.json` và copy lấy transcript đang chọn, áp offset; export `.txt` ghi chú khoảng thiếu. Các thao tác chính dùng được bằng bàn phím.

## Cross-Story Dependencies

Epic cần nền tảng Epic 1: DB, settings, Consent/key, Gemini gateway và IPC bindings. Luồng chính: 2.1 cung cấp media cho 2.2/2.3; 2.2 + 2.3 mở đường cho 2.4; 2.5 xây trên Job để chạy lại; 2.7 dùng Job/Transcript/proxy; 2.8 phụ thuộc Transcript detail; 2.9 phụ thuộc nhận file; 2.10 phụ thuộc Transcript detail. Story 2.6 có thể triển khai độc lập sau Settings Epic 1.

Các gate cần giữ: S1/OQ1 chốt xử lý Opus; S2/OQ2 phải chứng minh model `*-transcribe` nhận audio inline, nếu không phải chờ quyết định thay đổi phạm vi; S8/OQ3 xác minh FLAC playback/seek trên WKWebView và WebView2 trước khi chốt player. Epic 4 về sau tái sử dụng hàng đợi Job và proxy cho Transcribe lại từ Recording.
