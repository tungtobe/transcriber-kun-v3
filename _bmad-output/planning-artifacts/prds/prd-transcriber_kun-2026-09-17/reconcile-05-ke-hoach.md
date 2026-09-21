# Đối chiếu doc 05 (kế hoạch rebuild) + README với PRD/Addendum trans-kun v3

Nguồn đối chiếu:
- INPUT 1: `docs/rebuild-v3/05-ke-hoach-rebuild.md`
- INPUT 2: `docs/rebuild-v3/README.md`
- PRD: `_bmad-output/planning-artifacts/prds/prd-transcriber_kun-2026-09-17/prd.md`
- ADDENDUM: `_bmad-output/planning-artifacts/prds/prd-transcriber_kun-2026-09-17/addendum.md`

## Khoảng trống

- 05 §Rủi ro "Model Gemini preview đổi tên/hành vi" (giảm thiểu: "thông báo trong app", "smoke test định kỳ") → PRD chỉ có FR-7 (chấp nhận tên model tự do + cảnh báo nhẹ khi nhập), không có yêu cầu chủ động phát hiện/thông báo khi model đang dùng đổi hành vi hoặc bị deprecate giữa chừng vòng đời app → nên vào PRD (FR mới trong nhóm 4.2, ví dụ "cảnh báo khi model liên tục lỗi Shape/Model") → **mức trung**.
- 05 Phase 7 "Theo dõi: crash-free, lỗi Gemini theo category (đo cục bộ, opt-in)" → PRD không có FR/NFR nào cho một cơ chế đo lỗi/crash opt-in; đồng thời NFR-1 phát biểu tuyệt đối "Không telemetry", tạo khoảng mờ: nếu "đo cục bộ, opt-in" ở 05 nghĩa là gửi số liệu tổng hợp về cho team (khác với việc chỉ hiển thị cho chính người dùng), nó mâu thuẫn tiềm tàng với NFR-1 → nên làm rõ trong PRD (thêm FR nếu trong phạm vi v3, hoặc ghi rõ vào §10 Non-goals nếu bị loại khỏi v3) và ghi quyết định vào addendum → **mức trung-cao** (ảnh hưởng lời hứa privacy NFR-1).
- 05 §Rủi ro "Apple từ chối vì BYOK... cân nhắc gói 'key do RELIPA cấp' qua ephemeral token backend ở phase sau" → PRD chỉ nhắc dưới dạng `[NOTE FOR PM: ... phải được kéo lên sớm]` ở §11.2, không có mô tả về hình dạng kỹ thuật (ephemeral token backend) hay tiêu chí kích hoạt phương án dự phòng này → nên vào addendum (mục contingency riêng, tham chiếu Q11/KeyProvider) để kiến trúc không bị bất ngờ nếu phải kéo sớm → **mức trung**.
- Spike S4 "định nghĩa xong": "Ghi được Zoom, Teams, Meet (Chrome)" → PRD FR-17 chỉ mô tả năng lực chung "system/mic/mixed" qua Core Audio tap / WASAPI, không đưa 3 nền tảng họp cụ thể này thành hệ quả kiểm thử được (UJ chỉ dùng Teams/Zoom riêng lẻ, không phải tiêu chí chính thức) → nên vào PRD (thêm một dòng hệ quả ở FR-17: xác nhận ghi được audio hệ thống khi họp qua Zoom, Teams, Google Meet trên Chrome) → **mức thấp-trung**.
- Phase 2 "Xong khi": "kết quả khớp bản cũ ± timestamp" (yêu cầu ngầm về độ chính xác/độ tương đồng transcript so với v2) → PRD SM-3 chỉ cam kết parity về năng lực (giữ tính năng), không có ngưỡng/tiêu chí đối chiếu độ chính xác transcript với baseline v2 → nên vào addendum (như một tiêu chí QA khi thay pipeline decode+Gemini call) hơn là PRD, vì đây là tiêu chí kiểm thử di trú kỹ thuật chứ không phải năng lực người dùng thấy → **mức thấp**.
- 05 Phase 7 "Thông báo người dùng v2 chuyển sang trans-kun trên store; v2 ngừng phát hành sau 3 tháng" → không xuất hiện trong PRD (đúng phạm vi vì đây là truyền thông/vận hành cho v2, không phải feature v3) → nếu team muốn PRD là nguồn đầy đủ cho rollout, có thể thêm một dòng ở addendum (không cần FR) → **mức thấp**.

## Mâu thuẫn với quyết định đã chốt

- Không phát hiện mâu thuẫn trực tiếp nào giữa PRD/Addendum và 11 quyết định Q1–Q11: cả 11 quyết định đều được PRD trích dẫn đúng mã số (Q1 ở §10, Q2 ở Glossary/§11.2, Q3 ở §7/FR-17, Q4 ở §4.9/§9, Q5–Q8 ở §7/§10, Q9 ở tiêu đề, Q10 ở FR-7, Q11 ở §2.2/§9/§11.2) và không quyết định nào bị "mở lại" — các câu hỏi mở ở PRD §13 đều là chi tiết mới (phụ thuộc kết quả spike hoặc câu hỏi hành chính: key demo, trademark, hosting, máy test Arm64, nội dung template) chứ không đụng tới phạm vi Q1–Q11.
- Phát hiện một **mâu thuẫn nội tại trong chính INPUT** (không phải lỗi của PRD): README "Tóm tắt 10 dòng" (dòng cuối) viết "chuyển từ 9 loại file sidecar sang SQLite + file media; **import được cache cũ**", trong khi Q8 (cùng README, bảng quyết định) chốt "**Không import** cache v2; app mới độc lập hoàn toàn". PRD đã chọn đúng theo Q8 ("Không import dữ liệu v2" ở §10, §7, Glossary) — khuyến nghị sửa lại câu tóm tắt lỗi thời này trong README/05 để tránh gây hiểu nhầm cho người đọc sau.
- 05 bảng Q10 (dòng 90) vẫn ghi model dùng "cho transcribe/**radar/sniper**" dù Copilot/Radar/Sniper đã bị bỏ (Q1, chốt cùng ngày) — câu chữ lỗi thời trong 05, nhưng addendum mục A đã tự sửa đúng thành "(transcribe/memo)" nên không lan sang PRD. Không phải lỗi PRD, chỉ nên dọn lại 05 cho khỏi rối khi đọc lại.

## Đã phủ tốt

- Toàn bộ Q1–Q11 và các "Đã chốt thêm" của README được PRD/Addendum tôn trọng nguyên vẹn, đúng mã số, không mở lại.
- Tiêu chí "xong" Phase 4 (họp 60 phút, ≥ 6 reconnect, đổi target, re-detect, TTS không loop, force-quit vẫn còn recording) ánh xạ gần như 1-1 vào FR-19/20/21/22/23/24, thậm chí PRD lượng hóa chi tiết hơn 05 (vd. force-quit giữ dữ liệu "≤ 5 s trước khi chết").
- Spike S1, S2, S8 được PRD chuyển đúng thành Open Question 1, 2, 3 kèm liên kết FR bị ảnh hưởng (FR-9/14/32) — đúng tinh thần "spike quyết định gì thì để mở, việc đã rõ thì không mở".
- Rủi ro "WebView2 thiếu trên máy Win10 cũ" đã có hướng giải quyết rõ trong PRD §7 (kiểm tra runtime lúc chạy + hướng dẫn cài) — không phải khoảng trống như nghi ngờ ban đầu.
- Rủi ro "Sandbox mất quyền đọc file nguồn sau relaunch" được PRD phản ánh đầy đủ qua FR-10 (Proxy độc lập khỏi file nguồn).
- Tiêu chí chất lượng chung "không có transcript trong log" và "i18n key set" được PRD lượng hóa thành hệ quả kiểm thử được ở FR-41 (grep log tự động) và FR-47 (chặn CI khi thiếu key i18n), đúng bằng ngôn ngữ 05 dùng.
