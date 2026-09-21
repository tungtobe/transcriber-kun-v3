---
title: Review — đối chiếu ARCHITECTURE-SPINE với PRD/addendum trans-kun v3
reviewer: architecture-review (subagent)
date: 2026-09-18
target: ../ARCHITECTURE-SPINE.md
sources:
  - ../../../prds/prd-transcriber_kun-2026-09-17/prd.md
  - ../../../prds/prd-transcriber_kun-2026-09-17/addendum.md
---

# Review: đối chiếu ARCHITECTURE-SPINE với PRD/addendum

## Verdict

Spine bám khá sát PRD/addendum ở phần lớn NFR trọng yếu (bền phiên, mồ côi, an toàn đường dẫn media, Consent gate, log content-free), nhưng có **một mâu thuẫn thật** (bộ category lỗi hiển thị AD-7 lệch với NFR-6/NFR-9) và **năm khoảng trống đáng kể** — chủ yếu là các bất biến xuyên-feature (đóng app, ducking crash-recovery, cô lập lỗi Proxy/Memo khỏi lõi, ranh giới an toàn đường dẫn cho Creative, và ràng buộc "không bao giờ xin Screen Recording") — mà một epic độc lập hoàn toàn có thể làm khác nếu không được ghi thành luật ở spine.

## Danh sách phát hiện

### 1. [HIGH] Bộ category lỗi hiển thị của AD-7 mâu thuẫn với NFR-6/NFR-9 — thiếu category "CA"

**Nguồn:** NFR-6, NFR-9, addendum §D (phân loại `code`) vs. ARCHITECTURE-SPINE.md AD-7

**Vấn đề:** NFR-9 liệt kê rõ 8 category hiển thị: *"quota/auth/model/mạng/**CA**/định dạng/quyền hệ thống/lưu trữ"*. NFR-6 củng cố thêm: *"lỗi TLS/CA có thông báo riêng, hướng dẫn được"* — tức CA phải là một nhánh phân biệt được với "mạng" chung chung, đúng với bối cảnh proxy/CA nội bộ nêu ở §2.1 (Zscaler).

AD-7 lại định nghĩa 8 category khác: `quota | auth | model | network | format | permission | storage | blocked` — **không có `ca`**, thay vào đó có `blocked`. `blocked` không nằm trong danh sách NFR-9 nêu, còn `ca` bị AD-7 bỏ hẳn dù NFR-6 yêu cầu tường minh.

Vấn đề sâu hơn: lỗi CA/TLS xảy ra ở tầng bắt tay TLS (reqwest/rustls), **trước khi** có response từ Gemini — nó không nằm trong 8 `code` API-level của addendum §D (`Quota|Auth|Model|Request|Timeout|Network|Blocked|Shape`). Vì vậy AD-7's rule "ánh xạ code → category" không có chỗ để sinh ra category CA — cần một đường phân loại riêng ở tầng transport, không đi qua bộ phân loại lỗi API của `gemini/`.

**Vì sao quan trọng cho kiến trúc:** AD-7 tự nhận là "category ổn định" — một hợp đồng UI/i18n mà mọi feature dựa vào (FR-47 khoá i18n theo category). Nếu hai epic (một làm `gemini/` error mapping, một làm networking/CA detection) suy luận khác nhau về việc CA có tồn tại như category riêng hay bị gộp vào `network`, UI sẽ vi phạm NFR-6 mà không ai phát hiện tới lúc review.

**Đề xuất sửa:** Sửa rule AD-7: thêm `ca` vào enum (9 category, hoặc thay `blocked` bằng cách gộp `blocked` vào `format`/`model` nếu hợp lý về UX rồi dành chỗ cho `ca`); nói rõ lỗi CA/TLS được phân loại **ở tầng transport** (`reqwest`/rustls, trước khi vào bộ phân loại API của `gemini/`), không qua 8 `code` addendum §D. Cần chốt lại đúng 1 bộ category duy nhất và làm NFR-9 khớp nó (hoặc annotate NFR-9 đã lỗi thời nếu chủ sản phẩm đồng ý bỏ CA riêng — nhưng khi đó phải sửa luôn NFR-6/NFR-9, không để yên bản PRD mâu thuẫn).

---

### 2. [HIGH] Không có bất biến cấm dùng ScreenCaptureKit/Screen Recording cho audio capture

**Nguồn:** Q3 (addendum §A), UJ-1, FR-17, NFR-7 vs. ARCHITECTURE-SPINE.md AD-10

**Vấn đề:** PRD nhắc đi nhắc lại ràng buộc này ở nhiều chỗ độc lập — UJ-1: *"macOS hỏi quyền System Audio Recording (**không phải** Screen Recording)"*; FR-17: *"app chỉ xin quyền System Audio Recording và Micro, **không bao giờ** xin Screen Recording"*; NFR-7: *"Không xin quyền Screen Recording"*; Q3 (quyết định chốt): *"chỉ Core Audio process tap, không ScreenCaptureKit"*. Đây là một trong các tiêu chí thành công định danh rõ (SM-1, SM-2) và là rủi ro bị Apple từ chối nếu sai.

AD-10 (Topology Live) mô tả fan-out capture → WAV/WS nhưng **không nói API nào được dùng để capture** hay cấm ScreenCaptureKit. Đây là quyết định dễ bị một dev/agent viết `audio/` chọn nhầm (ScreenCaptureKit mới hơn, tài liệu Apple nhiều hơn Core Audio process tap), và không có AD nào chặn.

**Đề xuất sửa:** Thêm một câu vào AD-10 (hoặc AD riêng cho `audio/`): capture hệ thống trên macOS **chỉ** qua Core Audio process tap (không ScreenCaptureKit, không entitlement Screen Recording); Windows chỉ qua WASAPI loopback. Ràng buộc này nên nằm cùng chỗ với binds `audio/` vì nó quyết định entitlement khai trong `tauri.appstore.conf.json`.

---

### 3. [MEDIUM-HIGH] NFR-2 ("không chặn lõi") chỉ được cụ thể hoá cho `remote/` (AD-14), chưa cho Memo/Proxy

**Nguồn:** NFR-2, FR-10 (hệ quả), FR-38 (hệ quả) vs. ARCHITECTURE-SPINE.md AD-14

**Vấn đề:** NFR-2 nói rõ: *"Lỗi Memo, Proxy phát lại, Ad slot, đồng bộ cấu hình không được làm hỏng Transcript hay Recording."* — bốn nguồn lỗi ngang hàng. Nhưng AD-14 chỉ khoá bất biến này cho `remote/` (*"Lỗi remote/ không bao giờ lan sang transcript/recording (NFR-2)"*). Không có AD nào buộc `memo/` hay việc tạo Proxy phát lại (nằm trong luồng `transcribe/`/`live/`, xem FR-10: Proxy được tạo "ngay khi Phiên được tạo") phải cô lập lỗi khỏi việc lưu Transcript/Recording.

Đây là khác biệt về mức rủi ro: lỗi `remote/` (ads/config đồng bộ) vốn dĩ độc lập vật lý với transcript vì nó không nằm trên luồng ghi dữ liệu Phiên. Nhưng tạo Proxy phát lại **chạy trong cùng luồng lưu Phiên** (transcribe file, live) — một exception không được bắt ở đây có thể thực sự chặn việc lưu Transcript/Recording nếu không có rule kiến trúc buộc nó chạy tách biệt/fire-and-forget.

**Đề xuất sửa:** Mở rộng AD-14 thành một rule chung hơn (hoặc thêm bullet vào AD-4, vì AD-4 đã sở hữu "vòng đời Phiên"): lỗi tạo Proxy phát lại và lỗi sinh Memo không bao giờ được phép chặn hoặc làm hỏng việc lưu `transcripts`/`segments`/Recording — chạy sau khi Transcript đã persist, lỗi chỉ set cờ thiếu (`proxy_path IS NULL`) chứ không rollback hay panic luồng chính.

---

### 4. [MEDIUM] Không có bất biến điều phối hành vi đóng app (FR-12 vs FR-25 khác nhau, không rule cho trường hợp đồng thời)

**Nguồn:** FR-12, FR-25 vs. ARCHITECTURE-SPINE.md (không AD nào đề cập)

**Vấn đề:** FR-12 (Job file) và FR-25 (Live) mô tả hai hành vi **trái ngược nhau** khi người dùng đóng app:
- FR-12: đóng khi Job file đang chạy → xác nhận → nếu đóng tiếp, **huỷ sạch, không chạy nền**.
- FR-25: đóng khi Live đang chạy → xác nhận → nếu đóng tiếp, **giữ tiến trình sống thêm** để finalize Recording + lưu Phiên.

Đây là hai chính sách khác nhau cho cùng một cơ chế hệ thống (window-close/before-quit event của Tauri), và không AD nào trong spine sở hữu việc intercept sự kiện đóng cửa sổ hay hoà giải giữa hai domain. Trường hợp cả Job file **và** Live đang chạy cùng lúc (được phép theo AD-11: "Tối đa một LiveSession... chạy song song được với Job") khi người dùng đóng app hoàn toàn chưa có luật.

**Vì sao quan trọng cho kiến trúc:** Đây là một hook toàn app (`ipc/` hoặc `core/`), không thuộc riêng `transcribe/` hay `live/`; nếu để mỗi feature tự đăng ký handler riêng cho window-close, dễ dẫn tới hai dialog xác nhận chồng nhau hoặc app thoát nửa chừng.

**Đề xuất sửa:** Thêm một AD (hoặc mở rộng AD-2) quy định: có đúng một nơi (`ipc/` hoặc `core/app_lifecycle`) chặn sự kiện đóng cửa sổ, hỏi người dùng một lần duy nhất tổng hợp cả hai trạng thái (Job + Live), rồi gọi tuần tự: huỷ Job trước (FR-12), chờ Live finalize xong (FR-25) mới cho thoát thật.

---

### 5. [MEDIUM] AD-5 (an toàn đường dẫn, NFR-12) không bind tới `remote/`/`ads/`, nơi ảnh Creative được cache

**Nguồn:** NFR-12, FR-46 vs. ARCHITECTURE-SPINE.md AD-5, AD-14, bảng Capability → Architecture Map

**Vấn đề:** NFR-12 liệt kê tường minh **"id Creative"** là một trong các chuỗi cấm dùng làm thành phần đường dẫn file. FR-46 xác nhận ảnh Creative được *"tải và cache trong Container"*. Nhưng AD-5 (`Binds: db/, media/, library/, NFR-12, FR-11`) chỉ nói về tên file `media/<session-id>/<role>.<ext>`, không đả động gì tới cache ảnh Creative của `ads/`/`remote/`.

Bảng "Capability → Architecture Map" cuối spine lại khẳng định `NFR-1/NFR-2/NFR-9/NFR-12 | xuyên suốt | AD-1, AD-5, AD-7, AD-15` — ngụ ý AD-5 quản NFR-12 **toàn app**, nhưng rule thật của AD-5 lại không có phạm vi tới `ads/`/`remote/`. Đây là một mâu thuẫn nội bộ giữa bảng tổng hợp và rule chi tiết.

**Đề xuất sửa:** Mở rộng `Binds` và rule của AD-5 để bao gồm `remote/`, `ads/`: file cache Creative đặt tên theo hash nội dung hoặc ID nội bộ tự sinh, không bao giờ dùng trường `id` do server cấp trong `ads.json` làm tên file.

---

### 6. [MEDIUM] Ducking crash-recovery (FR-21, addendum §F) không có chỗ đứng kiến trúc

**Nguồn:** FR-21 (hệ quả "Crash khi đang Ducking → lần mở sau phục hồi volume"), addendum §F ("ducking 30% với marker file phục hồi sau crash") vs. ARCHITECTURE-SPINE.md AD-4, AD-9, AD-10

**Vấn đề:** Đây là cơ chế "phục hồi khi khởi động" thứ hai trong hệ thống (cơ chế đầu là orphan Live session ở AD-4), nhưng không được nhắc tới ở đâu trong spine. Không rõ: marker file này nằm ở đâu trong Container, ai đọc nó lúc khởi động (`audio::playback`? một `startup::reconcile` chung?), và liệu nó có dùng chung cơ chế "quét lúc khởi động" với AD-4's orphan recovery hay là một luồng riêng lẻ tự phát.

**Vì sao quan trọng cho kiến trúc:** Nếu không có một điểm sở hữu duy nhất cho "việc cần làm lúc khởi động", hai epic (live/orphan-recovery và audio/ducking) rất dễ mỗi bên tự viết code chạy ở App setup riêng, gây thứ tự chạy không xác định hoặc trùng lặp logic quét Container.

**Đề xuất sửa:** Thêm một câu ngắn (mở rộng AD-4 hoặc AD mới nhỏ) rằng có một entry-point khởi động duy nhất (`core`/`ipc::setup`) tuần tự chạy: (1) mồ côi Live session recovery, (2) audio output-volume/ducking recovery, theo cùng quy ước "im lặng, chạy nền, không chặn Home" như AD-24 mô tả cho FR-24. Nếu chủ sản phẩm chưa quan tâm, ít nhất nên đưa vào **Deferred** để không bị quên.

---

### 7. [LOW] Kiểm tra & hướng dẫn cài WebView2 Evergreen (Windows) không xuất hiện trong spine

**Nguồn:** PRD §7 ("WebView2 Evergreen — app kiểm tra runtime lúc chạy và hướng dẫn cài nếu thiếu") vs. ARCHITECTURE-SPINE.md (bảng Stack chỉ liệt kê nền tảng, không có rule)

**Vấn đề:** Đây là ngoại lệ duy nhất so với NFR-4 ("Không phụ thuộc ngoài... một tiến trình") — WebView2 Evergreen là runtime ngoài có thể thiếu trên máy Windows cũ, và PRD yêu cầu app tự phát hiện + hướng dẫn cài. Không có dòng nào trong spine ghi nhận luồng bootstrap/detection này (nó không thuộc bốn tầng module Rust mô tả ở Design Paradigm, mà là logic khởi động trước khi WebView tồn tại).

**Đề xuất sửa:** Thêm một dòng vào Deferred hoặc Structural Seed ghi nhận: kiểm tra WebView2 runtime là bước bootstrap Windows-only trước khi tạo cửa sổ Tauri, thuộc `main.rs`/installer, không thuộc feature nào — tránh bị bỏ quên khi lên epic Windows.

---

## Không flag (đã có cơ chế đủ tốt hoặc đã tự nhận là lệch có chủ đích)

- **Settings trong SQLite thay vì `tauri-plugin-store`** — AD-8 tự ghi nhận lệch addendum §B ("cần cập nhật ngược"). Không phải lỗi.
- **UUIDv7 ids** — AD-5 nói rõ, nhất quán toàn spine.
- **Không có bảng `jobs`** — AD-11 tự đánh dấu `[ASSUMPTION: bỏ bảng jobs của addendum §G]`, và nhất quán với FR-12 (đóng app khi Job chạy → huỷ sạch, không cần phục hồi Job sau crash).
- **Consent versioning** (FR-2) — AD-6 và AD-8 gate đủ chặt (consent version là settings, `gemini/` từ chối mọi request kể cả list models khi chưa có consent hiện hành).
- **Orphan Live session recovery** (FR-24) — AD-4 mô tả đầy đủ, rõ ràng, đúng tinh thần "không hỏi người dùng, chạy nền".
- **Partial transcript không bao giờ cache như hoàn chỉnh** (FR-15, addendum M/C3) — AD-4's `status ∈ {recording, finalizing, complete, partial}` + cờ `recovered`, cộng với ER diagram tách `retranscribed_from` thành transcript riêng, đã đủ để hai luồng (Chạy lại cùng Phiên vs Transcribe lại từ Live) không đụng nhau.
- **Key pool ưu tiên Live > Job > Memo** (AD-6) — không có trong PRD/addendum bằng câu chữ tường minh nhưng là hệ luận hợp lý của FR-6 + kịch bản UJ-3 nêu trong chính AD-6's "Prevents"; không mâu thuẫn, không cần flag.
- **Ngưỡng hiệu năng/kích thước (NFR-5, NFR-10, NFR-11)** — đúng loại chi tiết spine cố tình để cho code/test, không phải bất biến kiến trúc.

## File đích

`/Users/nguyenthanhtung/code/ai_lab/transcriber-kun/_bmad-output/planning-artifacts/architecture/architecture-transcriber_kun-2026-09-18/reviews/review-reconcile-prd.md`
