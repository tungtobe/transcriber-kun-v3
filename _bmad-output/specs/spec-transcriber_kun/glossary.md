# Glossary — trans-kun v3

Từ vựng neo cho SPEC và mọi companion. Viết hoa chữ đầu = thuật ngữ đã định nghĩa.

- **Phiên (Session)** — Đơn vị lưu trữ trong thư viện, kết quả của một lần Transcribe file hoặc một lần Live. Gồm `id`, tên, loại (file|live), ngày tạo, thời lượng, Transcript, Proxy phát lại; tuỳ chọn Recording, Ghi chú, Memo, Tag.
- **Transcript** — Danh sách Segment có thứ tự của một Phiên. Phiên live có thể có thêm Transcript thứ hai từ Transcribe lại.
- **Segment** — Đoạn text có `start`, `end` (giây, tuyệt đối trong Phiên), text và speaker (tuỳ chọn, chỉ có trong data/export, không hiện ở UI v3).
- **Bản dịch (Translation)** — Text do model Live trả về ở ngôn ngữ Target. Chỉ hiển thị trong màn Live, **không lưu** vào Phiên.
- **Target** — Ngôn ngữ đích của Bản dịch. Mặc định lấy từ Settings, đổi được giữa phiên; có lựa chọn "Không dịch".
- **Nguồn audio (Source)** — `system` (âm thanh hệ thống), `mic:<tên>`, hoặc `mixed:<mic>` (system + mic).
- **Recording** — File WAV 16 kHz mono PCM16 ghi liên tục trong Phiên live, lưu trong Container.
- **Proxy phát lại (Playback proxy)** — Bản audio FLAC 16 kHz mono tạo trong Container cho mọi Phiên. Trình phát dùng nó, độc lập với file nguồn.
- **Container** — Vùng lưu trữ riêng của app do sandbox/store cấp. Cố định, không tuỳ chỉnh.
- **Job** — Tác vụ dài đang chạy (transcribe file, Transcribe lại, sinh memo), có progress, cancel và lỗi có cấu trúc. Không sống qua lần khởi động lại app.
- **Chunk** — Đoạn audio N phút (mặc định 5, tối thiểu 1) gửi cho Gemini khi Transcribe file.
- **Khoảng thiếu (Missing range)** — Khoảng thời gian không có transcript: Chunk thất bại hẳn (file), hoặc audio vượt quá buffer gửi lại khi mất kết nối (live). Transcript có Khoảng thiếu do Chunk lỗi gọi là **Partial**, không được coi là hoàn chỉnh.
- **Key pool** — Tập Gemini API key người dùng nhập, xoay vòng theo chính sách lỗi (FR-6).
- **Model** — Tên model Gemini cho từng mục đích (transcribe file, live, memo). Người dùng chọn từ danh sách tải về hoặc nhập tự do.
- **Memo** — Văn bản Markdown do Gemini sinh từ Transcript (+ Ghi chú) theo một Template memo, lưu theo Phiên.
- **Template memo** — Prompt có placeholder bắt buộc `{transcript}` và tuỳ chọn `{notes}`. Có bộ mặc định theo ngôn ngữ UI; người dùng tự CRUD.
- **Ghi chú (Notes)** — Text tự do người dùng gõ trong hoặc sau Phiên, tự lưu, được nhúng vào prompt Memo.
- **Tag** — Nhãn gắn cho Phiên. Chuẩn hoá bằng trim, không phân biệt hoa thường. Tối đa 20 Tag/Phiên, mỗi Tag ≤ 80 ký tự.
- **Nhận diện lại (Re-detect)** — Dùng khi Live ở ngôn ngữ `auto`: mở kết nối model mới với ngữ cảnh trống, giữ nguyên Phiên, Recording và đồng hồ.
- **Transcribe lại (Re-transcribe)** — Chạy Transcribe file trên Recording của Phiên live, giữ Memo/Tag/Ghi chú.
- **Chạy lại (Re-run)** — Chạy lại Transcribe file (toàn bộ hoặc chỉ Khoảng thiếu) cho một Phiên file đã có.
- **Đọc bản dịch (TTS)** — Phát audio giọng nói của Bản dịch do model trả về, ngay trong tiến trình app.
- **Ducking** — Tự hạ volume hệ thống còn 30 % khi TTS nói, phục hồi sau đó.
- **Consent** — Sự đồng ý rõ ràng của người dùng, ghi nhận theo phiên bản văn bản, rằng audio/transcript sẽ được gửi tới Google Gemini bằng key của họ.
- **Ad slot** — Vùng hiển thị một Creative house ads, có nhãn "Sponsored", nút Báo cáo và "Vì sao tôi thấy quảng cáo này".
- **Creative** — Một mẩu quảng cáo: `{id, image ≤ 100 KB, title, sponsor, url, locale[], start, end, weight}`, tải từ `ads.json` có chữ ký.
- **Nhật ký chẩn đoán (Diagnostics log)** — Log content-free, xoay vòng, xuất được qua allow-list. Không bao giờ chứa transcript, bản dịch, ghi chú, memo hay key.
