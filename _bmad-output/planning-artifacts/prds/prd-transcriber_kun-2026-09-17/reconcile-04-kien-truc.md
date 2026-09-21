# Đối chiếu doc 04 (Kiến trúc mới Rust-only) với PRD + Addendum

Nguồn INPUT: `docs/rebuild-v3/04-kien-truc-moi-rust-only.md`
Đối chiếu với: `prd.md` (năng lực người dùng) và `addendum.md` (chi tiết kỹ thuật).

## Khoảng trống

- INPUT §3.2 (LiveGeneration state, restart) → cấu trúc `LiveGeneration { id, ws_task, clock_offset }` và ngưỡng "drain generation cũ ≤ 1 s" khi restart/Nhận diện lại không xuất hiện ở addendum (mục D chỉ có "Generation guard cho WS restart" chung chung, không có số) → nên vào **addendum** (mục D, cạnh bảng "Tham số vận hành"); số ≤1s cũng nên trở thành hệ quả kiểm thử được của FR-20 ở PRD → mức **trung**.
- INPUT §3.2 "TTS audio (`audio` event) không đi qua IPC: đẩy thẳng vào `audio::playback`; UI chỉ nhận `speaking: bool`" → đây là hành vi người dùng thấy được (màn Live cần một chỉ báo "đang nói"/"speaking" trong lúc TTS phát) nhưng PRD không có FR nào mô tả chỉ báo này (FR-21 chỉ nói bật/tắt TTS + Ducking, §8 kiến trúc thông tin chỉ liệt kê "TTS" như một mục chung) → nên thêm **FR mới vào PRD §4.4** (cạnh FR-21) mô tả chỉ báo "đang nói" hiển thị trên UI trong khi giữ chi tiết "không qua IPC" ở addendum (đã có nhưng sơ sài, mục H) → mức **cao** (đúng ví dụ nêu trong yêu cầu).
- INPUT §4 "i18n: `i18next` hoặc tự viết ... migrate key hiện có (bỏ nhóm setup/whisper/copilot, còn ≈400 key)" → addendum không có mục nào nêu lựa chọn thư viện i18n hay con số ≈400 key; PRD FR-47 chỉ nói chung "bộ key i18n đồng bộ, thiếu key chặn CI" → nên vào **addendum** (mục B hoặc mục mới) → mức **trung**.
- INPUT §4 "Giữ design token hiện tại (`--accent`, `--panel`, …)" → không xuất hiện ở cả PRD lẫn addendum → nếu là ràng buộc thật (giữ liên tục thị giác so với v2) nên vào **addendum**, mức **thấp**.
- INPUT §3.1 field request cụ thể cho model `*-transcribe`: `POST /v1beta/interactions` với body `input: [{type:"audio", data/uri, mime_type}]` + `generation_config.transcription_config` → addendum D chỉ nêu `transcription_config {mode: verbatim, timestamp_granularities:[word]}, language_codes`, không nêu tên field bọc ngoài `input` → nên bổ sung **addendum** mục D, mức **thấp**.
- INPUT §3.2 "bộ đếm diag" (diagnostic counters cho phiên Live) → chưa có trong addendum/PRD → nên vào **addendum**, mức **thấp**.
- INPUT §3.4 lý do kỹ thuật "media luôn copy/transcode vào container ngay khi tạo phiên, vì sandbox không giữ quyền đọc file nguồn sau relaunch" → addendum G chỉ có schema SQL, thiếu rationale này (PRD FR-10 đã mô tả đúng hệ quả observable nhưng không có lý do) → nên bổ sung ngắn gọn vào **addendum** mục G, mức **thấp**.
- INPUT §3.1 ví dụ JSON body cụ thể (`contents`/`parts`/`inline_data`) cho `generateContent` → addendum D mô tả bằng lời, không có JSON mẫu như INPUT → có thể hữu ích cho dev, nên thêm vào **addendum**, mức **thấp**.

## Chi tiết triển khai lọt vào PRD

- FR-6 (Key pool xoay vòng): liệt kê nguyên văn mã HTTP `429/401/403/400/404` — đây là chi tiết triển khai (đã có ở addendum D/L với đầy đủ tham số vận hành: cooldown 60s, max wait 180s, max attempts 4). PRD nên diễn đạt theo hành vi quan sát được (vd. "khi một key bị giới hạn tạm thời, app tự chuyển key khác trong một khoảng nghỉ ngắn"; "key sai bị loại khỏi vòng") và để mã lỗi HTTP cụ thể ở addendum.
- FR-17 (Nguồn audio): nêu thẳng tên API kỹ thuật "Core Audio process tap" và "WASAPI loopback" trong phần Hệ quả. Tên kỹ thuật này đã có sẵn ở addendum F; PRD chỉ cần mô tả hành vi/quyền người dùng thấy ("xin quyền System Audio Recording, không phải Screen Recording"; "âm thanh hệ thống bắt được đủ cả khi gọi qua ứng dụng giao tiếp") mà không cần lặp lại tên API.
- §11.2 (Ngoài phạm vi MVP) và §13 (Câu hỏi mở): tham chiếu trực tiếp mã spike `S1, S2, S7, S8` — đây là chi tiết lộ trình/kế hoạch triển khai thuộc addendum K (nguồn doc 05), không phải năng lực sản phẩm. Nên diễn đạt câu hỏi mở bằng ngôn ngữ sản phẩm, đặt mã spike làm chú thích liên kết chéo tới addendum thay vì đưa thẳng vào PRD.
- FR-15: "mỗi Chunk thử tối đa 4 lần" trùng số với addendum D/L ("chunk max attempts 4"). Có hệ quả observable (khi nào coi là Khoảng thiếu) nên có thể giữ, nhưng nên coi addendum là nguồn duy nhất của con số để tránh lệch khi tham số đổi — mức thấp, chỉ cần lưu ý khi bảo trì.

## Mâu thuẫn

1. **Chứng chỉ ký macOS không khớp tên.** INPUT §5 (bảng build) ghi: "Ký: Apple Distribution + Mac Installer Distribution" cho `store-mac`. Addendum J (nguồn doc 03) ghi: `productbuild` ký "3rd Party Mac Developer Installer". Đây là hai họ chứng chỉ Apple khác nhau — "Apple Distribution"/"Mac Installer Distribution" thường dùng cho phân phối Developer ID (ngoài store), còn "3rd Party Mac Developer Application/Installer" mới là tên đúng cho nộp Mac App Store. Vì v3 chỉ phân phối qua store (Q5), cần xác nhận lại tên chứng chỉ chính xác trước khi dựng pipeline `store-mac` — hiện hai tài liệu gốc (04 và 03) đang không khớp nhau.
2. **INPUT tự mâu thuẫn nhẹ về công thức hash phiên.** Module map §3 mô tả `media/probe.rs`: "hash (SHA-256 của audio stream đã decode, hoặc của file)" — để ngỏ hai phương án. Nhưng §3.3 lại chốt dứt khoát: "hash phiên = SHA-256 của file nguồn". Addendum (mục E) và PRD (FR-11) đều đi theo phương án "file nguồn" — tức đã chọn đúng hướng, nhưng ghi chú ở module map của doc 04 nên được sửa lại cho nhất quán để tránh gây hiểu nhầm khi implement `probe.rs`.

## Đã phủ tốt

- Toàn bộ shape Gemini REST/Live (endpoint, header, giới hạn 20 MB, thinking config, phân loại lỗi, tham số vận hành: max attempts/wait/cooldown/timeout) đã sang addendum D đầy đủ, khớp INPUT.
- Media pipeline (duration, decode, chunk, proxy FLAC/AAC tuỳ chọn, hash, định dạng hỗ trợ/không hỗ trợ, Opus mở) đã sang addendum E, khớp FR-9/FR-10/FR-11 ở PRD.
- Schema SQLite, IPC commands + events (kể cả danh sách event Live chi tiết hơn cả INPUT) đã sang addendum G/H.
- Loại bỏ Copilot, quyết định Q1–Q11, stack (Tauri/Rust/Svelte/SQLite/keyring/reqwest/symphonia…) đã khớp giữa ba tài liệu.
- Build & phân phối (overlay entitlements, MSIX, CI 2 job, TestFlight/Store flight) khớp giữa INPUT và addendum J, trừ điểm mâu thuẫn chứng chỉ nêu trên.
