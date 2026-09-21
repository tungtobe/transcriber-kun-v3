---
review: reconcile-ux
target: architecture/architecture-transcriber_kun-2026-09-18/ARCHITECTURE-SPINE.md
sources:
  - ux-designs/ux-transcriber_kun-2026-09-18/EXPERIENCE.md
  - ux-designs/ux-transcriber_kun-2026-09-18/DESIGN.md
  - ux-designs/ux-transcriber_kun-2026-09-18/.memlog.md
updated: 2026-09-18
---

# Reconcile — ARCHITECTURE-SPINE vs UX (EXPERIENCE.md / DESIGN.md)

## Tóm tắt

Phần lớn hành vi UX đã có chỗ đứng rõ trong spine (8 category lỗi ↔ AD-7 khớp từng chữ; ẩn Ad ở route Live ↔ AD-13; đồng hồ Phiên live không phải wall clock ↔ AD-9; recovery mồ côi ↔ AD-4; Live+Job song song có ưu tiên ↔ AD-11...). Khoảng trống thật sự tập trung ở bốn chỗ: (1) không có nơi lưu bền các dòng "khoảng thiếu / mất kết nối" trong luồng segment sau khi Phiên finalize, (2) FR-42 (diff cấu hình đề xuất) thiếu hẳn data/command contract dù UX đã chốt hành vi chi tiết, (3) trạng thái kết nối 3 bậc kèm "thời gian đã chờ" và nhánh ngoại lệ 5-lần-từ-chối-setup chưa có chỗ trong event schema, và (4) phím tắt toàn cục `⌘⇧L` chưa rõ là OS-level hay app-level — hai lựa chọn kéo theo kiến trúc và rủi ro sandbox khác hẳn nhau.

## Findings

### 1. [HIGH] Không có chỗ lưu bền cho dòng "Khoảng thiếu" / "Mất kết nối" trong luồng segment

**Nguồn UX:** EXPERIENCE.md, Component Patterns → *Danh sách segment*: "Khoảng thiếu và mất kết nối là **dòng nằm trong luồng**, đúng vị trí thời gian — không phải banner tách rời." State Patterns → *Transcript partial*: "**Không bao giờ ẩn, không bao giờ lưu như hoàn chỉnh.**" DESIGN.md, Components → dòng "Thiếu mm:ss–mm:ss" (nền warning-soft) và "Mất kết nối mm:ss–mm:ss" (nền surface-sunken) là hai kiểu dòng riêng biệt phải render đúng vị trí trong cả Live **và** Transcript detail sau khi mở lại.

**Gap:** AD-4 định nghĩa `db/` là chủ ghi duy nhất, "Segment live flush định kỳ vào `segments` và đây là **nguồn duy nhất** để phục hồi" — nhưng ER diagram và schema addendum §G (`segments(transcript_id, idx, start, end, text, speaker)`) không có cột hay entity nào phân biệt một segment văn bản thật với một dòng gap ("khoảng thiếu" do lỗi API hoặc "mất kết nối" do mạng). AD-3 (snapshot+delta) chỉ nói UI nhận lại state qua `<domain>_subscribe`, không nói dữ liệu gap này có được ghi vào `segments`/bảng riêng để tồn tại qua finalize + restart + mở lại `/session/:id` hay không. Nếu implement theo đúng spine hiện tại, dòng "Mất kết nối 45:02–48:10" chỉ tồn tại trong bộ nhớ actor `LiveSession` lúc live và biến mất khi Phiên finalize — trái với yêu cầu UX là nó phải nằm cố định trong transcript đã lưu.

**Đề xuất:** Bổ sung vào AD-4 (hoặc AD mới) một cách biểu diễn gap bền: hoặc thêm cột `segments.kind ∈ {text, missing, disconnected}` (segment "ảo" không có `text` thật, chỉ có `start/end` + kind), hoặc bảng `segment_gaps(transcript_id, start, end, kind)` join theo thời gian khi render. Nêu rõ: gap được ghi vào `db/` tại thời điểm phát sinh (không chỉ giữ trong actor), và đây cũng là nguồn cho badge "Thiếu N khoảng" ở dòng phiên.

### 2. [HIGH] FR-42 (diff cấu hình đề xuất) thiếu data/command contract

**Nguồn UX:** `.memlog.md` (UX) dòng cuối — quyết định chốt 2026-09-18: giữ preview diff đúng PRD §4.8 FR-42, "render **inline** trong chính nhóm Cài đặt → Cấu hình đề xuất... mỗi dòng dạng 'nhãn · giá trị hiện tại → giá trị đề xuất', chỉ liệt kê thiết lập thực sự đổi, hai nút Áp dụng/Huỷ, **không bao giờ liệt kê hay ghi đè key**". EXPERIENCE.md, Component Patterns → *Đồng bộ cấu hình đề xuất* lặp lại chi tiết này.

**Gap:** AD-14 chỉ nói `remote/` tải `recommended-settings.json`, xác minh ed25519, cache, fallback khi chữ ký sai — không có: (a) module nào tính diff (so `settings/` hiện tại với payload đã tải — thuộc `settings/` hay `remote/`?), (b) command nào trả diff cho UI (`<domain>_...`), (c) command áp dụng chọn lọc từng dòng hay áp dụng toàn bộ, (d) cơ chế **enforce cứng** "không bao giờ ghi đè key" — hiện tại chỉ là quy tắc UX, không có rule kiến trúc nào đảm bảo `recommended-settings.json` không thể chứa trường liên quan tới key/secrets trước khi tới bước diff.

**Đề xuất:** Thêm rule vào AD-14 (hoặc AD-8 vì đụng `settings/`): định nghĩa command dạng `settings_recommended_diff() -> Vec<{key, label, current, recommended}>` và `settings_apply_recommended(fields: Vec<String>)`, xác định module sở hữu phép so sánh (khuyến nghị `settings/`, gọi `remote/` chỉ để lấy payload đã xác minh), và validate loại trừ mọi trường liên quan API key ngay trong `remote/`/`settings/` trước khi tính diff — không dựa vào kỷ luật ở tầng UI.

### 3. [MEDIUM-HIGH] Trạng thái kết nối 3 bậc + thời gian chờ + nhánh "Đã dừng transcript" chưa có chỗ trong event schema

**Nguồn UX:** EXPERIENCE.md, Component Patterns → *Chỉ báo Live*: "kết nối, ba trạng thái — Đang transcribe / Đang nối lại (+ thời gian đã chờ) / Đã dừng transcript." Mục *Độ bền phiên*: "Ngoại lệ duy nhất: server từ chối setup 5 lần liên tiếp... App dừng thử, hiện banner và cho chọn **'Tiếp tục chỉ ghi âm'** hay **'Dừng'**."

**Gap:** Conventions liệt kê event variant mẫu (`delta, segment, deltaTranslated, speaking, restarting, gap, …`) không có biến thể tương ứng rõ ràng cho 3 trạng thái kết nối hay cho nhánh "đã dừng transcript sau 5 lần từ chối" (khác hẳn "đang nối lại" bình thường — đây là trạng thái *dừng hẳn*, cần lựa chọn của người dùng). Ngoài ra, "thời gian đã chờ" (2:10) là **wall-clock**, trong khi AD-9 quy định "Đồng hồ Phiên live = số sample đã capture ÷ sample rate, **không phải wall clock**" — AD-9 không nêu ngoại lệ cho bộ đếm chờ-nối-lại này, dễ khiến người viết story hiểu nhầm là cấm luôn cả timer này. Vì AD-3 quy định "UI không bao giờ là nguồn sự thật", event mang trạng thái reconnect cần có trường thời điểm bắt đầu mất kết nối (hoặc elapsed đã tính sẵn) để UI không tự đếm sai lúc remount.

**Đề xuất:** (a) Thêm vào Conventions một enum rõ ràng cho trạng thái kết nối (vd `transcribing | reconnecting{since} | stopped_awaiting_choice`), (b) thêm command tương ứng lựa chọn "Tiếp tục chỉ ghi âm"/"Dừng" theo quy ước đặt tên `live_*`, (c) bổ sung một câu vào AD-9 làm rõ: quy tắc "không wall clock" chỉ áp dụng cho đồng hồ Phiên/segment timestamp, không áp dụng cho bộ đếm chờ-nối-lại hiển thị UI (đồng hồ này lấy từ `since` do Rust gửi).

### 4. [MEDIUM] Job state "đang chờ quota" chưa có biến thể event

**Nguồn UX:** EXPERIENCE.md, State Patterns → *Mọi key đang nghỉ (429)*: "Job **chờ** tối đa 180 s/chunk thay vì fail ngay; card job hiện 'đang chờ quota'."

**Gap:** AD-6 nêu đúng cơ chế phía sau (`KeyPool` ưu tiên Live>Job>Memo, Job ngừng gửi chunk mới khi Live 429) nhưng AD-3/Conventions không liệt kê biến thể event cho domain Job tương ứng với "đang chờ quota" khác với "đang xử lý chunk bình thường" — ví dụ trong Conventions hiện chỉ có ví dụ minh hoạ cho `LiveEvent`, không có `JobEvent`. Card job cần phân biệt hai trạng thái này để hiện đúng text theo UX.

**Đề xuất:** Bổ sung ví dụ biến thể cho `JobEvent` (vd `progress`, `waiting_quota{until}`, `retrying`, `failed_range`) song song với `LiveEvent` trong bảng Conventions.

### 5. [MEDIUM] Thứ tự thực thi cho dedup theo hash chưa được ràng buộc kiến trúc

**Nguồn UX:** DESIGN.md/EXPERIENCE.md, *Drop-zone*: "File trùng nội dung (theo hash) → **mở phiên có sẵn, không gọi Gemini**." Mục *Minh bạch chi phí*: "Mở lại một phiên có sẵn (trùng hash file) **không gọi Gemini** và giao diện nói rõ điều đó."

**Gap:** AD-5 chỉ nói `source_hash` là cột unique "dùng phát hiện trùng" — không có rule minh thị rằng lệnh nhận file **phải** kiểm tra hash và trả về phiên có sẵn *trước khi* chạm tới `JobRegistry`/`gemini/`. Đây là một cam kết minh bạch chi phí cốt lõi (không phải chi tiết vặt), nên nên được ràng buộc ở tầng kiến trúc thay vì phó mặc cho thứ tự gọi hàm trong story.

**Đề xuất:** Thêm một câu vào AD-5 hoặc AD-11: "Command nhận file mới tính `source_hash` và tra cứu trước khi tạo `FileJob`; trùng hash → trả `session_id` có sẵn, không khởi tạo Job, không gọi `gemini/`."

### 6. [MEDIUM] Contract cho overlay "Đang lưu phiên" → điều hướng, và đường lỗi khi finalize thất bại

**Nguồn UX:** EXPERIENCE.md, IA → "Sau khi Dừng Live → điều hướng thẳng sang `/session/:id`... qua một overlay 'Đang lưu phiên…'." State Patterns → *Đang lưu phiên*: "Overlay nhỏ 'Đang lưu phiên… finalize recording, tạo proxy' rồi điều hướng."

**Gap:** AD-4 đã có `status ∈ {recording, finalizing, complete, partial}` (tốt, đúng hướng), nhưng không có rule nào nói rõ: lệnh Dừng Live có block cho tới khi finalize+tạo proxy xong rồi mới trả `session_id` (đơn giản cho overlay), hay trả ngay và FE phải theo dõi `finalizing → complete` qua subscribe? Cũng không có đường lỗi nào được định nghĩa cho trường hợp finalize thất bại giữa chừng (vd Container đầy đĩa — thuộc category "Lưu trữ" theo AD-7) trong lúc overlay đang hiện.

**Đề xuất:** Thêm rule vào AD-4 hoặc AD-10: định nghĩa `live_stop` là command đồng bộ trả về khi finalize+proxy xong (đơn giản nhất, khớp với overlay không có progress chi tiết), và nêu rõ hành vi khi finalize lỗi: Phiên giữ nguyên ở trạng thái `recording`/mồ côi để lần sau phục hồi theo cơ chế AD-4 sẵn có, overlay đổi thành banner category tương ứng thay vì điều hướng.

### 7. [MEDIUM] Phím tắt toàn cục `⌘⇧L` — chưa rõ OS-level hay app-level, ảnh hưởng sandbox

**Nguồn UX:** EXPERIENCE.md, Interaction Primitives: "`⌘⇧L` / `Ctrl+⇧+L` — Bắt đầu / Dừng Live (**toàn cục**)."

**Gap:** Spine không có mục nào (Stack, AD) đề cập tới cơ chế đăng ký phím tắt. Nếu "toàn cục" nghĩa là hoạt động kể cả khi app không focus (OS-wide), cần plugin `tauri-plugin-global-shortcut` + entitlement tương ứng, và trên macOS App Sandbox việc đăng ký global hotkey có thể bị giới hạn hoặc cần quyền Accessibility — mâu thuẫn tiềm tàng với cam kết "entitlement tối thiểu... Không xin quyền Screen Recording" (EXPERIENCE.md, Foundation) và có rủi ro review store. Nếu chỉ là "toàn cục trong app" (mọi màn trong khi app đang focus), đây chỉ là JS keydown listener ở app-shell, không cần plugin nào — khác hẳn về mặt kiến trúc và rủi ro.

**Đề xuất:** Chốt phạm vi "toàn cục" (khuyến nghị: app-focused only, không OS-wide, để tránh rủi ro sandbox) và ghi vào spine — nếu chọn OS-wide thật, thêm plugin vào Stack + entitlement vào Deferred/AD tương ứng và đánh giá tương thích App Sandbox trước Phase 0.

### 8. [MEDIUM] Khôi phục trạng thái ducking âm lượng sau crash chưa có chỗ lưu bền

**Nguồn UX:** EXPERIENCE.md, *Sống sót qua crash*: "Volume hệ thống bị ducking dở cũng được phục hồi ở lần mở sau."

**Gap:** Structural Seed có `audio/output_volume` nhưng không AD nào nói volume gốc (trước khi duck) được ghi xuống đâu để sống sót qua force-quit, hay logic khôi phục chạy ở đâu lúc khởi động (tương tự cơ chế "Phiên mồ côi" của AD-4, nhưng đây là state hệ điều hành, không phải state DB).

**Đề xuất:** Thêm một câu vào AD-10 (hoặc AD mới cho `audio/`): volume gốc trước khi duck được ghi vào `settings`/một file trạng thái nhỏ trong Container ngay khi bắt đầu duck; lúc khởi động, nếu có giá trị "duck dở" tồn đọng → khôi phục volume rồi xoá cờ.

### 9. [LOW] Route `/onboarding` — mâu thuẫn nhỏ giữa Conventions và bảng IA của UX

**Nguồn UX:** EXPERIENCE.md, bảng IA (Bề mặt/Route): ba bước Onboarding đều ghi Route = "—" (không có route).

**Gap:** Spine's Conventions table liệt kê `/onboarding` như một route thật: "Route: `/onboarding`, `/home`, `/session/:id`, `/live`, `/settings/:group`". Không rõ Onboarding có URL-addressable hay là overlay/state trước khi vào app shell (ảnh hưởng E2E test, deep link, lịch sử back/forward).

**Đề xuất:** Thống nhất một trong hai: nếu Onboarding không có route (theo IA), sửa Conventions bỏ `/onboarding` khỏi danh sách route hoặc ghi rõ nó là state gate chứ không phải route SPA.

### 10. [LOW] Chiến lược cold-load ≤ 2 s chưa có quyết định kiến trúc

**Nguồn UX:** EXPERIENCE.md, State Patterns → *Cold load*: "App mở tới Trang chủ ≤ 2 s (NFR-5)... vùng danh sách hiện skeleton 6 dòng." Component Patterns → *Dòng phiên*: "Virtual list — 500 dòng render ≤ 1 s."

**Gap:** Không AD nào bàn tới chiến lược bootstrap (`library_list` trả full 500 dòng một lần hay phân trang; có cache/snapshot nào giúp paint sớm trong khi WebView2/IPC handshake còn đang khởi động hay không) để đạt ngân sách 2 s tổng. Rủi ro thấp vì SQLite cục bộ vốn nhanh, nhưng đáng có một dòng quyết định tường minh vì đây là NFR đo được và ảnh hưởng cách viết command `library_list`.

**Đề xuất:** Thêm một dòng ở Deferred hoặc AD-4: xác nhận `library_list` không phân trang ở quy mô 500 dòng (đủ nhanh trong ngân sách), hoặc nêu rõ chiến lược nếu cần cache/snapshot sớm.

## Không phải gap (đối chiếu xác nhận khớp)

- 8 category lỗi UX ↔ AD-7 `quota|auth|model|network|format|permission|storage|blocked` — khớp 1-1.
- Ẩn Ad slot ở route Live ↔ AD-13 "ẩn với mọi route Live" — khớp.
- Hai pill riêng (ghi/kết nối) không gộp ↔ chủ đề chính của AD-3 + AD-10 (Recording độc lập mạng) — khớp về nguyên tắc, chỉ thiếu chi tiết ở Finding 3.
- Job không sống qua restart, huỷ sạch khi đóng app có job ↔ AD-11 "Job không sống qua restart; JobRegistry trong bộ nhớ là nguồn sự thật" — khớp.
- Consent chặn cả "Kiểm tra key" ↔ AD-6 "gemini/ từ chối mọi request khi chưa có Consent... kể cả list models" — khớp.
- Dark mode + settings do Rust sở hữu ↔ AD-8 (settings trong SQLite, không tauri-plugin-store) — cơ chế lưu theme phù hợp, không có gap kiến trúc riêng ngoài việc theme là một giá trị settings như mọi giá trị khác.
