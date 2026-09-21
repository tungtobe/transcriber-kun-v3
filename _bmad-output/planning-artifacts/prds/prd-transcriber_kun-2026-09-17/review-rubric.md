# PRD Quality Review — trans-kun v3

## Overall verdict

PRD này đạt mức kỷ luật hiếm thấy so với PRD thông thường: gần như mọi FR có "Hệ quả" kiểm thử được với số cụ thể, Glossary được tuân thủ nhất quán tuyệt đối, ID (FR/NFR/UJ/SM) liền mạch không lỗ hổng, Assumption Index roundtrip 100%, và Open Questions thực sự mở, được gate vào spike Phase 0 thay vì bị "làm cho có". Rủi ro còn lại nằm ở vài điểm cụ thể chứ không phải cấu trúc: một ngưỡng "đáng kể" không định nghĩa ở FR-2 (liên quan Consent — nhạy cảm với review Apple 5.1.2), FR-28 (tìm kiếm) hoàn toàn không có hệ quả kiểm thử được, cam kết nền tảng Windows Arm64 (§7) treo lơ lửng cạnh một Open Question về máy test chưa được gate vào kế hoạch spike, và PRD không có lập trường nào về GDPR/CCPA dù phát hành công khai qua hai store toàn cầu. Đây là PRD đủ điều kiện chuyển tiếp cho UX/architecture với một danh sách sửa ngắn, không cần viết lại.

## Decision-readiness — strong

Vision (§1) nêu trade-off rõ ràng, có cái được và cái mất: "v3 chấp nhận thu hẹp phạm vi: chỉ Gemini (bỏ Whisper local), bỏ Copilot, bỏ Linux, bỏ kênh tải trực tiếp" — không né tránh. §13 Câu hỏi mở là các câu hỏi thật (phụ thuộc spike S1/S2/S8, phụ thuộc bên thứ ba như tên app/máy test), không phải câu hỏi tu từ có đáp án ngay câu sau. `[NOTE FOR PM]` duy nhất ở §11.2 đặt đúng chỗ căng thẳng thật (rủi ro Apple từ chối mô hình BYOK), không phải ở một checkpoint an toàn.

Một khoảng trống thực sự: §7 cam kết nền tảng "Windows... x64 + Arm64" và addendum §J lặp lại cam kết build MSIX Arm64, nhưng §13 Câu hỏi mở #7 hỏi "Máy test Windows Arm64 có sẵn không?" mà không nằm trong danh sách spike bắt buộc Phase 0 (S1–S8, addendum §K). Đây là một cam kết phạm vi (platform support) đứng trên một ẩn số chưa có kế hoạch xử lý nếu câu trả lời là "không" — khác với Opus/Files API/FLAC-trong-WebView đều đã được gate vào spike với phương án dự phòng nêu rõ (FR-9, §11.2).

### Findings
- **[high]** Cam kết Windows Arm64 không có kế hoạch dự phòng nếu thiếu máy test (§7 "Nền tảng"; §13 Câu hỏi mở #7) — §7 và addendum §J liệt kê Arm64 như phạm vi đã chốt để submit store, nhưng Câu hỏi mở #7 cho thấy khả năng test còn chưa xác nhận, và mục này không nằm trong spike Phase 0 (S1–S8) như các Open Question khác. Nếu không có máy test trước ngày submit, không rõ Arm64 bị rút khỏi §11.1 hay app ship chưa kiểm thử trên kiến trúc đó. *Fix:* thêm spike/gate riêng cho Arm64 (mượn máy, cloud CI Arm64, hoặc QEMU) vào Phase 0 addendum §K, hoặc thêm `[NOTE FOR PM]` ở §7/§11.1 nêu phương án nếu không có máy test kịp (ví dụ: ship x64 trước, Arm64 sau).

## Substance over theater — strong

Không có persona theater: chỉ 3 vai trò (Linh, Minh, reviewer Apple/MS), mỗi vai đều đẩy ra quyết định cụ thể — headphone Bluetooth của Linh kéo ra yêu cầu chuyển thiết bị output trong Ducking (FR-21 hệ quả), force-quit của Linh kéo ra phục hồi phiên mồ côi (FR-24), proxy Zscaler của Minh kéo ra NFR-6. Không có mục "differentiation" viết cho có. NFR (§5) không rơi vào boilerplate "phải scalable/secure" — mỗi NFR có cơ chế cụ thể gắn với FR (vd. NFR-8 giới hạn đúng 3 luồng tốn token, NFR-4 liệt kê chính xác cái không được có). Vision không thể hoán đổi sang PRD khác — gắn chặt với niche cụ thể (BrSE Việt-Nhật, lời hứa "cài từ store, mở lên là dùng" đối lập trực tiếp với nỗi đau vận hành của v2).

## Strategic coherence — strong

Thesis rõ: loại bỏ toàn bộ nguyên nhân sinh ticket hỗ trợ môi trường của v2 (Python, ffmpeg, PATH, quyền admin) bằng cách gói mọi thứ vào một tiến trình Rust, đổi lại thu hẹp phạm vi tính năng. Toàn bộ NFR-4, NFR-7, §7, §11 phục vụ đúng thesis này. SM-1/SM-2 đo trực tiếp thesis (lên store không sửa kiến trúc; hết ticket môi trường) thay vì số liệu hoạt động chung chung (không có DAU/MAU). Counter-metric SM-C1/SM-C2 tồn tại và đối trọng đúng các cách "gian lận" số liệu dễ xảy ra nhất (tăng ad impression, bỏ retry để tăng tốc). Ad slot (§4.9) — dù thoạt nhìn như tính năng ngoài lề — được buộc lại vào thesis qua chính Vision ("không làm hỏng trải nghiệm B2B") và các guardrail cụ thể (không hiện ở Live, cap tần suất), không phải backlog rời rạc.

## Done-ness clarity — adequate

Phần lớn PRD vượt chuẩn thông thường: FR-6, FR-12–15, FR-21–24 có số cụ thể (60s cooldown, ≤2s, 30% volume, ≥6 lần reconnect trong 60 phút...) — đúng tinh thần "testable consequence". Nhưng có khoảng trống thật khi soi kỹ từng FR:

### Findings
- **[high]** "đáng kể" không định nghĩa cho việc hỏi lại Consent (FR-2, §4.1) — "đổi văn bản Consent đáng kể → hỏi lại" không có ngưỡng hay tiêu chí nào cho "đáng kể" (sửa lỗi chính tả khác gì thêm một bên nhận dữ liệu mới?). Đây là adjective-không-bound đúng loại rubric yêu cầu bắt, và nó nằm ở luồng Consent — nhạy cảm với review Apple 5.1.2 và với chính NFR-1 (Riêng tư). *Fix:* định nghĩa "đáng kể" bằng tiêu chí cụ thể (vd. đổi bên nhận dữ liệu, đổi phạm vi dữ liệu gửi đi → hỏi lại; sửa câu chữ/dịch thuật → không) hoặc chuyển thành versioning rule tường minh (mọi bump major version của văn bản Consent → hỏi lại).
- **[medium]** FR-28 (Tìm kiếm theo tên) không có mục "Hệ quả" nào (§4.5, dòng ~296–297) — toàn bộ FR chỉ có một câu mô tả, không nêu: khớp chuỗi con hay từ đầu, có phân biệt hoa/thường không, lọc theo thời gian gõ (debounce) hay cần Enter, thông báo gì khi không có kết quả. Đây là FR duy nhất trong toàn bộ §4 hoàn toàn thiếu hệ quả kiểm thử được, nổi bật vì các FR liền kề (FR-27, FR-29) đều có. *Fix:* thêm Hệ quả tối thiểu: kiểu khớp (substring, không phân biệt hoa/thường), độ trễ lọc (debounce ms), hành vi khi 0 kết quả.
- **[low]** Một số FR ở §4.5–4.6 thiếu mục Hệ quả dù nội dung mô tả đủ để hiểu ý đồ (FR-30 "Đổi tên, xoá Phiên"; FR-33 "Tìm trong Transcript"; FR-34 "Hiển thị Transcript") — không nghiêm trọng vì thân FR đã mang hệ quả implicit, nhưng phá vỡ nhất quán cấu trúc "Mô tả + Hệ quả" mà phần lớn §4 tuân theo, gây khó cho việc trích xuất acceptance criteria tự động ở bước epics/stories. *Fix:* bổ sung Hệ quả ngắn cho ba FR này để nhất quán cấu trúc, hoặc nêu rõ trong §0 rằng "Hệ quả" là tuỳ chọn khi mô tả đã đủ tường minh.

## Scope honesty — strong

§10 Non-goals và §11.2 Ngoài phạm vi MVP làm việc thật: mỗi mục nêu lý do bị loại (Q1–Q11, hoặc "sau khi có người dùng", hoặc phụ thuộc spike), không phải danh sách trang trí. `[ASSUMPTION]` inline xuất hiện đúng 13 lần và cả 13 đều có mặt ở Chỉ mục giả định §14 (roundtrip 100% — xem Mechanical notes). Mật độ open-items (8 câu hỏi mở + 13 assumption + 1 NOTE FOR PM) là hợp lý cho một PRD chain-top chuẩn bị submit hai store, đặc biệt vì phần lớn được gate vào spike Phase 0 (addendum §K) thay vì để trôi tự do.

### Findings
- **[medium]** Không có lập trường về GDPR/CCPA hay phạm vi địa lý phát hành (§6.1 Privacy, §6.2 Tuân thủ store) — PRD dành hẳn hai mục cho tuân thủ Apple/Microsoft rất chi tiết (privacy label, entitlement, ATT) nhưng phát hành qua Mac App Store/Microsoft Store về bản chất là toàn cầu, và tài liệu không đề cập GDPR (EU), CCPA, hay có giới hạn địa lý phát hành nào không. Đây là im lặng chứ không phải loại bỏ có chủ đích — không có `[NON-GOAL]` hay `[ASSUMPTION]` nào phủ nó, trong khi các omission khác trong PRD đều được gắn thẻ cẩn thận. *Fix:* thêm một dòng ở §6.1 — hoặc xác nhận trans-kun không nhắm thị trường EU/California ở v3 (Non-goal), hoặc xác nhận Consent + Privacy Policy hiện tại đã đủ đáp ứng GDPR/CCPA cơ bản (right to deletion qua "Xoá toàn bộ dữ liệu" FR-40 có thể đã đủ, nhưng cần nói rõ).

## Downstream usability — strong

Glossary (§3, ~25 mục) được dùng nhất quán tuyệt đối xuyên suốt FR/UJ/NFR — không phát hiện từ đồng nghĩa lệch nghĩa (vd. "Proxy phát lại" luôn tách bạch với "Recording", "Bản dịch" luôn tách bạch với "Transcript", đúng định nghĩa gốc). ID liên tục và không trùng: FR-1→47 (47/47, không lỗ hổng), NFR-1→11, UJ-1→5, SM-1→5 + SM-C1/C2 — đã verify bằng grep trực tiếp. Mọi UJ đều được ít nhất một nhóm tính năng ở §4 tham chiếu qua "Thực hiện UJ-X" (không có UJ mồ côi). Cross-reference nội bộ dùng ID (FR-6, FR-12, FR-15...) chứ không dùng "xem ở trên", giúp từng section đọc tách rời vẫn hiểu được.

## Shape fit — strong

Sản phẩm desktop có UX quan trọng, đối tượng đa dạng vai trò (BrSE, sales) và có gate compliance cứng (App Store review) — 5 UJ có tên riêng (Linh, Minh) cộng một kịch bản reviewer là lựa chọn hình dạng đúng: đủ để làm bệ cho UX/architecture, không over-formalize (không có hàng chục UJ cho một sản phẩm ít vai trò). Là brownfield rebuild, PRD xử lý ranh giới cũ/mới trung thực ở tầng vĩ mô: bundle ID mới, không import dữ liệu v2 (Q8, §10), SM-3 chỉ định rõ năng lực nào phải giữ.

### Findings
- **[low]** Không phân biệt tường minh FR nào là hành vi kế thừa từ v2 và FR nào là cải tiến mới của v3 (toàn bộ §4) — vì đây là viết lại hoàn toàn bằng stack khác, coi mọi FR là "spec mới" là hợp lý, nhưng một số hành vi rõ ràng là sửa lỗi/cải tiến so với v2 đã biết (vd. FR-22 reconnect trong suốt, addendum §L liệt kê "Deferred bug v2 không tái tạo") mà PRD không gắn cờ để team build biết đâu là parity bar tối thiểu và đâu là nâng cấp có thể trễ nếu cần cắt giảm thời gian. *Fix:* cân nhắc gắn nhãn ngắn (vd. "[cải tiến so với v2]") cho các Hệ quả mà đến từ bài học/bug v2 cụ thể, giúp ưu tiên khi cần cắt phạm vi trong 12–15 tuần.

## Mechanical notes

- **Glossary drift:** không phát hiện. Các cặp thuật ngữ dễ nhầm (Proxy phát lại vs Recording, Bản dịch vs Transcript, Nhận diện lại vs Transcribe lại) đều được dùng đúng định nghĩa gốc trong mọi FR liên quan.
- **ID continuity:** đã verify bằng script — FR-1 đến FR-47 liền mạch không lỗ hổng/trùng lặp; NFR-1–11, UJ-1–5, SM-1–5 (+SM-C1/C2) đều liền mạch.
- **Assumption Index roundtrip:** hoàn hảo — 13 thẻ `[ASSUMPTION]` inline (FR-2, FR-8, FR-18, FR-27, FR-32, FR-35, FR-38, FR-40, FR-42, FR-44, NFR-5, NFR-10, NFR-11) đều có mặt ở §14, và không có mục nào ở §14 thiếu thẻ inline tương ứng.
- **Cross-reference "Câu hỏi mở 9":** đã kiểm tra cụ thể theo yêu cầu — không còn dấu vết trong `prd.md`/`addendum.md`. Tham chiếu duy nhất còn lại nằm trong `.memlog.md` (nhật ký làm việc, không phải tài liệu PRD), ghi rõ câu hỏi này đã được chủ sản phẩm chốt và gộp vào FR-12 ("đóng app khi Job transcribe → huỷ sạch"). Không có dangling reference.
- **UJ protagonist naming:** UJ-1, UJ-2, UJ-4 dùng chung nhân vật Linh (BrSE) với bối cảnh nhất quán; UJ-3 dùng Minh (sales) với bối cảnh riêng (Windows, proxy Zscaler). UJ-5 dùng vai trò "Reviewer của Apple" không có tên riêng — chấp nhận được vì đây là kịch bản compliance/store-review, không phải persona người dùng mục tiêu, nhưng khác biệt với 4 UJ còn lại nên downstream (UX) cần biết UJ-5 không cần persona sheet đầy đủ như Linh/Minh.
- **Required sections:** đầy đủ cho stake "chain-top, public launch, rebuild": Vision, Users/JTBD/UJ, Glossary, FR theo nhóm, NFR, Constraints/guardrail, Platform, IA, Monetization, Non-goals, MVP scope, Success metrics (định tính, đúng lựa chọn của chủ sản phẩm), Open Questions, Assumption Index — không thiếu mục nào so với stake đã thống nhất.
