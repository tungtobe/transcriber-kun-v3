---
title: trans-kun v3
status: final
created: 2026-09-17
updated: 2026-09-18
---

# PRD: trans-kun v3
*Tên làm việc — đã chốt (Q9). Rebuild hoàn toàn của Transcriber-kun v2.2.2.*

## 0. Mục đích tài liệu

PRD này dành cho chủ sản phẩm (tony), team build (1–2 dev + AI agent) và các workflow phía sau (UX, architecture, epics/stories). Nó mô tả **trans-kun v3 phải làm gì** ở mức năng lực người dùng thấy được; **cách làm** (crate, schema, IPC, lộ trình, spike) nằm ở `addendum.md` và bộ tài liệu gốc `docs/rebuild-v3/` (01 hiện trạng, 02 bài học, 03 điều kiện store, 04 kiến trúc, 05 kế hoạch). PRD không lặp lại các tài liệu đó; khi có mâu thuẫn, PRD này là nguồn yêu cầu, bộ tài liệu gốc là nguồn bối cảnh.

Cấu trúc: Glossary neo từ vựng; FR đánh dấu *(mới ở v3)* là năng lực v2 chưa có, còn lại là kế thừa v2 (có thể đổi cách làm); tính năng gộp theo nhóm với FR đánh số toàn cục; giả định gắn thẻ `[ASSUMPTION]` inline và gom ở §9. Mọi quyết định phạm vi đã chốt ngày 2026-09-13/14 (Q1–Q11 trong doc 05) được coi là **đầu vào cố định**, không mở lại trong PRD này.

## 1. Tầm nhìn

trans-kun là ứng dụng desktop (macOS, Windows) giúp người Việt làm việc với khách Nhật **ghi lại và hiểu buổi họp ngay lập tức**: transcribe realtime âm thanh cuộc họp online kèm bản dịch, transcribe file ghi âm/video có sẵn, rồi sinh memo/議事録 từ transcript. Người dùng tự mang Gemini API key của mình; dữ liệu họp chỉ đi từ máy người dùng tới Google Gemini, không qua server trung gian nào của chúng ta.

v3 là bản build lại từ đầu với một lời hứa mới: **cài từ store, mở lên là dùng**. Không cài Python, không cài ffmpeg, không wizard, không quyền admin, không self-update. Toàn bộ phần khiến v2 sinh ra phần lớn ticket hỗ trợ (bootstrap runtime, dò PATH, cài công cụ ngoài) biến mất vì mọi xử lý nằm trong một tiến trình Rust. Đổi lại, v3 chấp nhận thu hẹp phạm vi: chỉ Gemini (bỏ Whisper local), bỏ Copilot, bỏ Linux, bỏ kênh tải trực tiếp.

Vì sao quan trọng: phân phối qua Mac App Store và Microsoft Store mở ứng dụng ra ngoài phạm vi nội bộ Relipa, buộc sản phẩm đạt chuẩn sandbox, privacy và review của hai store. Đây vừa là ràng buộc kỹ thuật vừa là dấu ấn chất lượng mà v2 không có. Một ad slot nhỏ tự vận hành tạo đường doanh thu/cross-promotion ban đầu mà không làm hỏng trải nghiệm B2B.

## 2. Người dùng mục tiêu

### 2.1 Jobs To Be Done

- **Chức năng:** Có transcript liên tục, có timestamp, của buổi họp Nhật–Việt (online hoặc file) mà không phải tự ghi chép; có bản dịch sang ngôn ngữ mình đọc nhanh nhất; sinh 議事録/memo theo mẫu của mình trong vài giây sau họp; tìm lại phiên cũ theo tên/tag.
- **Cảm xúc:** Không sợ "mất" nội dung khi mạng chập chờn, app crash hay máy sập nguồn; không lo lộ nội dung họp cho bên thứ ba ngoài Google; không phải ngại xin quyền "ghi màn hình".
- **Xã hội:** Tự tin xác nhận lại yêu cầu với khách Nhật ngay trong họp nhờ bản dịch realtime; gửi 議事録 chuẩn mực đúng giờ.
- **Bối cảnh:** Máy công ty có thể bị hạn chế cài phần mềm, có proxy/CA nội bộ; họp qua Zoom/Teams/Google Meet; dùng tai nghe Bluetooth; đôi khi buổi họp chuyển ngôn ngữ giữa chừng.

### 2.2 Không phải người dùng (v3)

- Người cần transcribe **offline** hoàn toàn hoặc không muốn dùng Google Gemini.
- Người dùng Linux.
- Tổ chức cần quản trị tập trung (SSO, key do công ty cấp, audit) — để phiên bản sau (Q11).
- Người cần diarization/speaker label; v3 không hiển thị speaker trên UI (chỉ lưu trong dữ liệu và export `.json` khi model trả về).

### 2.3 Hành trình người dùng chính

- **UJ-1. Linh mở app lần đầu từ Mac App Store và dùng được trong 3 phút.**
  - **Persona + bối cảnh:** Linh, BrSE 3 năm kinh nghiệm ở công ty offshore, MacBook M2 chạy macOS 15, họp khách Nhật 2–3 lần/tuần qua Teams. Đã có Gemini API key do sếp gửi qua Slack.
  - **Trạng thái vào:** Vừa cài từ App Store, chưa có dữ liệu, chưa cấp quyền gì.
  - **Đường đi:** (1) App mở màn Onboarding, tự chọn ngôn ngữ UI theo hệ thống (vi), cho đổi. (2) Màn đồng ý dữ liệu: nói rõ audio/transcript sẽ gửi tới Google Gemini bằng key của Linh, link privacy policy; Linh bấm Đồng ý. (3) Dán API key; app kiểm tra bằng cách tải danh sách model và báo "Key hợp lệ, N model khả dụng". (4) Vào Home trống với hướng dẫn hai lối: kéo file vào, hoặc bắt đầu Live.
  - **Cao trào:** Linh bấm Live → chọn nguồn "Âm thanh hệ thống + mic" → macOS hỏi quyền **System Audio Recording** (không phải Screen Recording) và Micro; Linh cấp; transcript bắt đầu chạy khi Linh phát thử một video YouTube.
  - **Kết thúc:** Linh dừng; app lưu phiên rồi mở Transcript detail của nó; phiên xuất hiện trong Home với tên theo giờ. Linh đổi tên thành "Test".
  - **Edge case:** Key sai/hết hạn → app hiện lỗi có thể hiểu ("Key không được chấp nhận") ngay ở Onboarding, cho sửa tại chỗ; app không bao giờ vào Home ở trạng thái "trắng".

- **UJ-2. Linh họp Teams 60 phút với khách Nhật, có dịch realtime và không mất gì khi app bị force-quit.**
  - **Trạng thái vào:** Đã onboard; đang ở Home; Teams sắp bắt đầu; tai nghe Bluetooth.
  - **Đường đi:** (1) Bấm Live, nguồn "system + mic", ngôn ngữ transcribe `auto`, target dịch `vi` (mặc định từ Settings). (2) Transcript gốc (ja) và bản dịch (vi) chạy song song; Linh chuyển view "Cả hai". (3) Giữa họp khách chuyển sang nói tiếng Anh, bản dịch kém đi → Linh bấm "Nhận diện lại ngôn ngữ"; transcript tiếp tục, timestamp không nhảy. (4) Linh bật loa đọc bản dịch để nghe tiếng Việt trong tai nghe; volume Teams tự hạ khi app nói và trả lại khi im. (5) Linh gõ ghi chú vào panel Notes trong lúc họp. (6) Phút 40, Linh cần nói: đổi nguồn sang chỉ "mic" rồi quay lại "system + mic" mà không ngắt phiên. (7) Kết nối Gemini bị server đóng nhiều lần trong 60 phút; Linh không thấy gì ngoài một chỉ báo nhỏ; transcript liền mạch. (8) Phút 45 Wi-Fi văn phòng rớt 3 phút: chỉ báo chuyển sang "đang ghi âm, đang nối lại (2:10)", Linh vẫn gõ ghi chú; mạng có lại, transcript tự chạy tiếp, 3 phút đó hiện một dòng "mất kết nối 45:02–48:10" trong transcript.
  - **Cao trào:** Họp xong, Linh bấm Dừng. Trong vài giây phiên được lưu, có recording phát lại được, transcript gốc, ghi chú. Linh bấm "Sinh memo" với template 議事録 → memo Markdown xuất hiện, có nhúng ghi chú của Linh.
  - **Kết thúc:** Phiên nằm trong Home, badge "memo", "có audio". Linh gắn tag "KH-ABC", "sprint-12".
  - **Edge case:** Ở phút 50 app bị force-quit. Mở lại app: phiên mồ côi được tự đưa vào Home với recording tới phút 50 và transcript đã tích luỹ; Linh có thể "Transcribe lại" từ recording để có bản chất lượng cao hơn (lấp luôn 3 phút mất kết nối), hiển thị cạnh bản live.

- **UJ-3. Minh transcribe file recording 90 phút và gửi memo cho khách trong buổi chiều.**
  - **Persona + bối cảnh:** Minh, sales, Windows 11 laptop công ty có proxy Zscaler. Nhận file `.mp4` recording Zoom 90 phút từ khách.
  - **Trạng thái vào:** Đã onboard trên Windows (cài từ Microsoft Store).
  - **Đường đi:** (1) Kéo file vào Home → app chuyển sang màn Transcript, bắt đầu ngay. (2) Progress hiển thị theo thời lượng audio đã xử lý ("32/90 phút"), có log chi tiết nếu mở panel debug. (3) Minh rời sang Home để xem phiên khác; Home hiện "1 job đang chạy" và cho quay lại. (4) Một chunk bị quota 429 → app xoay sang key thứ hai (Minh có 2 key); chunk khác lỗi hẳn → transcript đánh dấu "thiếu 45:00–50:00".
  - **Cao trào:** Transcript hoàn tất với cảnh báo thiếu; Minh bấm "Chạy lại" chỉ cho khoảng thiếu (hoặc toàn bộ); sau khi đầy đủ, Minh chọn template "Biên bản họp sales" → memo → Copy → dán vào email.
  - **Kết thúc:** Phiên trong Home có badge memo; proxy phát lại nằm trong app nên sau này Minh xoá file gốc vẫn nghe lại và click-to-seek được.
  - **Edge case:** File `.wmv` → app báo ngay "Định dạng không hỗ trợ, hãy chuyển sang mp4/m4a/mp3" thay vì lỗi im lặng.

- **UJ-4. Linh tìm lại một câu khách nói ba tuần trước.**
  - **Đường đi:** Home → lọc tag "KH-ABC" → tìm "giao diện admin" trong tên phiên không ra → mở phiên nghi ngờ → ô tìm trong transcript → 3 kết quả, prev/next → click segment → audio nhảy tới đúng chỗ → Linh nghe lại giọng khách để chắc chắn → Export SRT gửi cho QA.
  - **Cao trào:** Nghe được đúng câu trong 30 giây thay vì tua cả recording.

- **UJ-5. Reviewer của Apple mở app lần đầu với key demo.**
  - **Bối cảnh:** Reviewer không có key Gemini; đọc App Review Notes có key demo và 3 bước.
  - **Đường đi:** Onboarding → consent → dán key demo → Home trống có hướng dẫn → kéo file mẫu → transcript chạy → mở Settings, thấy link Privacy Policy, nút "Báo cáo quảng cáo" và "Vì sao tôi thấy quảng cáo này" cạnh banner.
  - **Cao trào:** Không có màn hình trống, không crash, không xin quyền lạ (không Screen Recording), không có gợi ý tải bản ngoài store.

## 3. Glossary

- **Phiên (Session)** — Một đơn vị lưu trữ trong thư viện: kết quả của một lần Transcribe file hoặc một lần Live. Có `id`, tên, loại (file|live), ngày tạo, thời lượng, Transcript, Proxy phát lại, và tuỳ chọn Recording, Ghi chú, Memo, Tag.
- **Transcript** — Danh sách Segment có thứ tự của một Phiên. Một Phiên live có thể có thêm một Transcript thứ hai từ Transcribe lại.
- **Segment** — Một đoạn text có `start`, `end` (giây, tuyệt đối trong Phiên), text, và speaker (tuỳ chọn; chỉ trong dữ liệu/export, không hiển thị ở UI v3).
- **Bản dịch (Translation)** — Text do model Live trả về ở ngôn ngữ Target; chỉ hiển thị trong màn Live, **không lưu** vào Phiên.
- **Target** — Ngôn ngữ đích của Bản dịch (mặc định trong Settings, đổi được giữa phiên Live).
- **Nguồn audio (Source)** — `system` (âm thanh hệ thống), `mic:<tên>` hoặc `mixed:<mic>` (system + mic).
- **Recording** — File WAV ghi liên tục trong Phiên live, lưu trong Container.
- **Proxy phát lại (Playback proxy)** — Bản audio nén (FLAC 16 kHz mono, Q2) tạo trong Container cho mọi Phiên để trình phát dùng, độc lập với file nguồn.
- **Container** — Vùng lưu trữ riêng của app do sandbox/store cấp; cố định, không tuỳ chỉnh (Q7).
- **Job** — Một tác vụ dài đang chạy (transcribe file, sinh memo) có progress, cancel, lỗi có cấu trúc.
- **Chunk** — Đoạn audio N phút (mặc định 5) gửi cho Gemini trong Transcribe file.
- **Khoảng thiếu (Missing range)** — Khoảng thời gian của Chunk thất bại hẳn; Transcript có Khoảng thiếu gọi là **Partial** và không được coi là hoàn chỉnh.
- **Key pool** — Tập Gemini API key người dùng nhập (nhiều key cách dấu phẩy), xoay vòng theo chính sách quota.
- **Model** — Tên model Gemini cho từng mục đích: transcribe file, live, memo. Người dùng chọn từ danh sách tải về hoặc nhập tự do.
- **Memo** — Văn bản Markdown do Gemini sinh từ Transcript (+ Ghi chú) theo một Template memo; lưu theo Phiên.
- **Template memo** — Prompt có placeholder `{transcript}` và tuỳ chọn `{notes}`; có bộ mặc định theo ngôn ngữ UI; người dùng CRUD.
- **Ghi chú (Notes)** — Text tự do người dùng gõ trong/sau Phiên, tự lưu, được nhúng vào prompt Memo.
- **Tag** — Nhãn text gắn cho Phiên; chuẩn hoá (trim, không phân biệt hoa thường); tối đa 20 Tag/Phiên, mỗi Tag ≤ 80 ký tự.
- **Nhận diện lại (Re-detect)** — Trong Live với ngôn ngữ `auto`: mở kết nối model mới với ngữ cảnh trống để model bắt lại ngôn ngữ đang nói, giữ nguyên Phiên, Recording, đồng hồ.
- **Transcribe lại (Re-transcribe)** — Chạy Transcribe file trên Recording của Phiên live để có Transcript chất lượng cao hơn, giữ Memo/Tag/Ghi chú.
- **Đọc bản dịch (TTS)** — Phát audio giọng nói của Bản dịch do model trả về, trong tiến trình app.
- **Ducking** — Tự hạ volume hệ thống khi TTS nói và phục hồi sau.
- **Consent** — Sự đồng ý rõ ràng của người dùng, ghi nhận một lần, rằng audio/transcript sẽ được gửi tới Google Gemini bằng key của họ.
- **Ad slot** — Vùng hiển thị một Creative quảng cáo tự vận hành (house ads), có nhãn "Sponsored", nút Báo cáo và "Vì sao tôi thấy quảng cáo này".
- **Creative** — Một mẩu quảng cáo: ảnh, tiêu đề, nhà tài trợ, URL, locale, thời hạn, trọng số; tải từ endpoint của chúng ta, có chữ ký.
- **Nhật ký chẩn đoán (Diagnostics log)** — Log content-free của app; xuất được qua allow-list; không bao giờ chứa transcript, key, ghi chú.

## 4. Tính năng

### 4.1 Onboarding & Consent
**Mô tả:** Lần mở đầu tiên (hoặc khi chưa có Consent/key), app hiện luồng ba bước: ngôn ngữ UI → Consent → API key. Không có bước kiểm tra môi trường nào. App có trạng thái "chưa có key" thân thiện ở mọi màn (không crash, không màn trắng), cho phép quay lại nhập key. Thực hiện UJ-1, UJ-5.

#### FR-1: Chọn ngôn ngữ UI khi mở lần đầu
Người dùng có thể chọn ngôn ngữ UI (vi/en/ja); mặc định theo ngôn ngữ hệ thống, fallback `en`.
**Hệ quả (kiểm thử được):**
- Đổi ngôn ngữ áp dụng tức thì cho toàn bộ luồng Onboarding.
- Giá trị được lưu và dùng cho Template memo mặc định, nhãn thời gian, nội dung Creative.

#### FR-2: Màn Consent trước khi gửi bất kỳ dữ liệu nào *(mới ở v3)*
Người dùng phải đồng ý rõ ràng rằng audio và transcript sẽ gửi tới Google Gemini bằng key của họ, trước khi bất kỳ tính năng gọi Gemini nào bật.
**Hệ quả:**
- Không có request nào tới Google trước khi Consent được ghi nhận, kể cả kiểm tra key.
- Màn Consent có link Privacy Policy mở trong trình duyệt ngoài và nêu tên bên nhận dữ liệu (Google).
- Consent lưu bền cùng số phiên bản văn bản Consent; app chỉ hỏi lại khi số phiên bản tăng (mọi thay đổi nội dung đều tăng phiên bản). `[ASSUMPTION: chỉ một cấp Consent, không tách audio/transcript]`
- Từ chối Consent → app ở trạng thái chỉ xem Settings/About, có nút quay lại đồng ý.

#### FR-3: Nhập và kiểm tra API key ngay trong Onboarding
Người dùng nhập một hoặc nhiều Gemini API key; app kiểm tra bằng cách tải danh sách model và báo kết quả tại chỗ.
**Hệ quả:**
- Chấp nhận cả định dạng key `AIza…` và `AQ.…`.
- Lỗi phân loại rõ: key không hợp lệ / mạng / CA công ty (proxy) / quota, với hướng dẫn ngắn.
- Key hợp lệ → app hiện số model khả dụng và cho tiếp tục; có thể "Bỏ qua, nhập sau".

#### FR-4: Trạng thái "chưa có key" thân thiện
Khi chưa có key hợp lệ, mọi màn vẫn mở được; các nút cần Gemini bị vô hiệu với lời giải thích và lối tắt tới nơi nhập key.
**Hệ quả:**
- Không màn hình trắng; không crash; không dialog lặp.
- Danh sách Phiên và phát lại vẫn dùng được (không cần Gemini).

### 4.2 Gemini API key & model
**Mô tả:** Người dùng tự mang key (BYOK). Key được lưu an toàn, có thể nhiều key để xoay vòng khi quota; model chọn theo mục đích. Thiết kế chừa chỗ cho nguồn key khác (premium) ở phiên bản sau (Q11) mà không lộ ra UI v3.

#### FR-5: Lưu key an toàn *(mới ở v3)*
API key được lưu trong kho khoá của hệ điều hành (Keychain / Credential Manager), không nằm trong file cấu hình plaintext.
**Hệ quả:**
- File settings không chứa key.
- Xoá key trong UI xoá khỏi kho khoá.
- Nếu kho khoá không truy cập được → thông báo lỗi có thể hiểu, không mất các setting khác.

#### FR-6: Key pool xoay vòng theo quota
Khi có nhiều key, app xoay vòng theo loại lỗi: quota (429) → key đó nghỉ 60 s và request thử lại bằng key kế; key bị từ chối (401/403) → loại key khỏi vòng cho tới khi người dùng sửa, request thử lại bằng key kế nếu còn; lỗi request/model (400/404) và timeout → không xoay, báo lỗi.
**Hệ quả:**
- Một chính sách chung cho mọi luồng Gemini (transcribe, live, memo, list models).
- Khi mọi key đang nghỉ, Job chờ (tối đa 180 s/Chunk) thay vì fail ngay.
- Timeout không được khiến app gửi cùng request đó sang key khác.
- Hết key khả dụng vì 401/403 → lỗi category Auth kèm lối tắt tới Settings.

#### FR-7: Chọn model theo mục đích, có tải danh sách
Người dùng chọn Model cho transcribe file, live và memo từ danh sách tải qua API (lọc theo năng lực), hoặc nhập tên tự do.
**Hệ quả:**
- Mặc định như v2 (Q10): `gemini-flash-lite-latest` cho transcribe/memo, `gemini-3.5-live-translate-preview` cho live.
- Danh sách live chỉ hiện model hỗ trợ Live.
- Tên Model không có trong danh sách vẫn được chấp nhận, kèm cảnh báo nhẹ.
- Model đang cấu hình bị Google trả lỗi category Model (đổi tên/ngừng) → app hiện cảnh báo tại chỗ (không chỉ trong log) và lối tắt tới Settings để chọn Model khác; Job/Phiên không tự đổi Model.

### 4.3 Transcribe file
**Mô tả:** Người dùng kéo/chọn file audio/video; app tách audio, cắt Chunk, gửi Gemini, ghép Segment với timestamp tuyệt đối, tạo Proxy phát lại trong Container, lưu Phiên. Job có progress theo thời lượng thực, cancel, tiếp tục xem khi quay lại, và không bao giờ giấu Khoảng thiếu. Thực hiện UJ-3.

#### FR-8: Nhận file qua dialog hoặc kéo thả
Người dùng có thể chọn file bằng dialog hệ thống hoặc kéo thả vào Home/Transcript; app chuyển sang màn Transcript và bắt đầu ngay.
**Hệ quả:**
- Hoạt động trong sandbox: chỉ cần quyền file do người dùng chọn trong phiên chạy.
- Kéo nhiều file → xử lý tuần tự, mỗi file một Phiên. `[ASSUMPTION: hàng đợi tuần tự, không song song]`

#### FR-9: Định dạng hỗ trợ được nêu rõ, lỗi tức thì
App hỗ trợ audio mp3, m4a, wav, flac, ogg/vorbis, aiff, caf và video mp4/mov/mkv/webm (chỉ lấy track audio). Định dạng khác bị từ chối ngay với thông báo nêu định dạng nên chuyển sang.
**Hệ quả:**
- File `avi/wmv/flv/ts` → thông báo không hỗ trợ trước khi tạo Phiên.
- Track Opus trong webm/mkv: hỗ trợ hay từ chối rõ ràng theo kết quả spike (Open Question 1); không được lỗi im lặng.
- Danh sách định dạng hiện trong dialog chọn file và trong thông báo lỗi.

#### FR-10: Không phụ thuộc file nguồn sau khi tạo Phiên *(mới ở v3)*
Ngay khi Phiên được tạo, app tạo Proxy phát lại trong Container; mọi tính năng về sau (phát, seek, Transcribe lại, export) dùng Proxy phát lại, không cần file nguồn.
**Hệ quả:**
- Xoá/di chuyển file nguồn sau đó không ảnh hưởng Phiên.
- Mở lại app sau relaunch trong sandbox vẫn phát được mọi Phiên.
- Proxy phát lại hỏng/thiếu → Phiên vẫn mở được Transcript; trình phát báo "không có audio" và cho chọn lại file nguồn để tạo lại Proxy.

#### FR-11: Phát hiện trùng lặp theo nội dung file
Transcribe cùng một file (theo hash nội dung) → app mở ngay Phiên có sẵn thay vì chạy lại; người dùng chủ động "Chạy lại" từ Transcript detail khi muốn.
**Hệ quả:**
- Mở Phiên có sẵn không gọi Gemini, không tốn token.
- Chạy lại giữ Tag, Ghi chú, Memo của Phiên cũ; Transcript cũ được ghi đè sau khi bản mới hoàn tất, không tạo Phiên mới.

#### FR-12: Progress theo thời lượng thực, cancel, tiếp tục xem
Job hiển thị tiến độ theo phút audio đã xử lý trên tổng, có nút huỷ, có panel log; rời màn rồi quay lại vẫn thấy Job đang chạy.
**Hệ quả:**
- Khi huỷ, việc gửi Chunk mới dừng trong ≤ 2 s và dữ liệu tạm được dọn; Phiên chưa hoàn chỉnh không được lưu.
- Home hiển thị Job đang chạy và lối quay lại (FR-27).
- Đóng app khi Job chạy → hỏi xác nhận; nếu tiếp tục đóng, Job huỷ sạch (không chạy nền).

#### FR-13: Chunk, timestamp tuyệt đối, offset
Audio được cắt thành Chunk `chunkMinutes` (mặc định 5, tối thiểu 1); Segment trả về theo `MM:SS` tương đối so với Chunk và được cộng offset tuyệt đối; người dùng có thể đặt offset timestamp toàn cục (giây) áp dụng lúc hiển thị/export.
**Hệ quả:**
- Segment không bao giờ lùi thời gian so với Segment trước.
- Chunk quá lớn để gửi inline → app tự chia nhỏ hơn (ví dụ 3 phút) và ghi log.

#### FR-14: Model Gemini cho transcribe file và Live
Transcribe file dùng model Gemini thông thường qua `generateContent`, trả JSON Segment 5–15 s. Prompt yêu cầu tự nhận diện tiếng Việt và tiếng Nhật xen kẽ, giữ nguyên lời nói và không dịch. Model trong Settings có thể là tên version, alias hoặc tên người dùng nhập; app không tự đổi model hay đoán version từ alias. Model `*-transcribe`/Interactions không dùng cho file ở v3.
**Hệ quả:**
- Live dùng model Live Translate riêng để nhận transcript đầu vào, transcript bản dịch và âm thanh bản dịch.
- Segment file có timestamp tuyệt đối sau khi cộng offset Chunk; speaker của bản file không có dữ liệu thì để NULL.

#### FR-15: Chịu lỗi từng Chunk, không giấu Khoảng thiếu
Mỗi Chunk thử tối đa 4 lần với chính sách Key pool (FR-6); Chunk thất bại hẳn thành Khoảng thiếu; Transcript Partial được hiển thị với cảnh báo mốc thời gian thiếu và **không** được lưu như hoàn chỉnh.
**Hệ quả:**
- UI liệt kê từng Khoảng thiếu và cho "Chạy lại phần thiếu" (chỉ gửi lại Chunk thiếu) hoặc "Chạy lại toàn bộ".
- Phiên Partial có trạng thái `partial` trong Home (badge) cho tới khi đầy đủ.
- Không bao giờ ghi đè Transcript live bằng Transcript Partial (Transcribe lại).
- Lỗi server tạm thời (5xx) được thử lại trong 4 lần; lỗi request/model (400/404) hoặc nội dung bị chặn → Chunk fail ngay, không thử lại.
- Response JSON vỡ/cắt cụt → parser cứu Segment hợp lệ; phần không cứu được thành Khoảng thiếu.

#### FR-16: Ngôn ngữ transcribe
Người dùng chọn ngôn ngữ transcribe (auto/ja/vi/en, mặc định auto) trong Settings; áp dụng cho Transcribe file và Live.

### 4.4 Live transcribe & dịch
**Mô tả:** Người dùng bắt đầu Phiên live từ Nguồn audio đã chọn; transcript gốc stream theo thời gian thực, kèm Bản dịch sang Target; có thể đổi Nguồn, đổi Target, Nhận diện lại, bật Đọc bản dịch giữa phiên mà không ngắt Phiên. Recording ghi liên tục và sống sót qua crash. Kết nối tới model được duy trì trong suốt, người dùng không phải làm gì khi server đóng kết nối định kỳ. Thực hiện UJ-2.

#### FR-17: Chọn Nguồn audio, đổi giữa phiên
Người dùng chọn `system`, `mic:<tên>` hoặc `mixed:<mic>` trước khi bắt đầu; có dropdown mic (preselect mặc định, nút làm mới); đổi Nguồn audio giữa phiên không ngắt Phiên, không mất timestamp.
**Hệ quả:**
- Đổi Nguồn audio có hiệu lực trong ≤ 1 s; đồng hồ Phiên và Recording liên tục.
- macOS: app chỉ xin quyền "System Audio Recording" và Micro, không bao giờ xin Screen Recording; âm thanh do chính app phát (TTS) không bị thu lại. Windows: thu được cả âm thanh của app họp native (Zoom/Teams dùng endpoint communications) lẫn họp trong trình duyệt (endpoint console). Cơ chế: addendum §F.
- Kiểm thử chấp nhận: thu được Zoom, Teams và Google Meet trong Chrome trên cả hai OS.
- Thiếu quyền → hướng dẫn cấp quyền trong app với đường dẫn tới System Settings, không crash; Phiên chưa bắt đầu cho tới khi có quyền hoặc người dùng chọn Nguồn audio khác.

#### FR-18: Transcript realtime
Text stream token-by-token, tách câu khi gặp dấu kết câu, timestamp theo đồng hồ capture cục bộ; Segment tích luỹ được lưu tạm định kỳ để phục hồi.
**Hệ quả:**
- Segment mới hiện trong ≤ 2 s sau khi câu kết thúc trong điều kiện mạng bình thường. `[ASSUMPTION: ngưỡng]`
- Danh sách Segment tự cuộn theo mới nhất, dừng tự cuộn khi người dùng cuộn lên.

#### FR-19: Dịch realtime với Target đổi được giữa phiên
Bản dịch stream song song với Transcript gốc; view chuyển **Gốc / Dịch / Cả hai**; Target mặc định từ Settings, đổi giữa phiên qua dropdown mà không ngắt Phiên; dropdown có lựa chọn **Không dịch** để tắt hẳn Bản dịch (chỉ transcript gốc).
**Hệ quả:**
- Speech đã ở Target vẫn hiện trong pane Dịch (không im lặng).
- Target được chọn thì luôn dịch, kể cả khi ngôn ngữ nguồn khai báo trùng Target.
- Bản dịch **không** lưu vào Phiên; sau khi dừng, chỉ còn transcript gốc.
- Đổi Target: Transcript gốc không gián đoạn; Bản dịch mới bắt đầu từ câu kế tiếp.
- Khi "Không dịch": không yêu cầu model sinh Bản dịch hay audio TTS (không tốn token cho dịch); view chỉ còn Gốc; nút TTS bị vô hiệu.

#### FR-20: Nhận diện lại ngôn ngữ
Khi ngôn ngữ transcribe là `auto`, người dùng có thể bấm Nhận diện lại; app mở kết nối model mới với ngữ cảnh trống, giữ Phiên, Recording, đồng hồ, Transcript đã có.
**Hệ quả:**
- Không có Segment lặp hay nhảy timestamp sau khi chuyển.
- Sự kiện từ kết nối cũ sau thời điểm chuyển bị bỏ; kết nối cũ được đóng trong ≤ 1 s sau khi kết nối mới sẵn sàng.
- Chuyển thất bại (kết nối mới không sẵn sàng) → giữ kết nối cũ, báo lỗi nhẹ, Phiên không gián đoạn.
- Nút bị ẩn/vô hiệu khi ngôn ngữ không phải `auto`.

#### FR-21: Đọc bản dịch (TTS) trong tiến trình app, có Ducking
Người dùng bật/tắt loa đọc Bản dịch; audio phát trong tiến trình app, theo thiết bị output mặc định (kể cả khi đổi tai nghe giữa chừng); volume hệ thống hạ còn 30 % khi model nói và phục hồi sau.
**Hệ quả:**
- Âm thanh TTS không bị chính app thu lại (không vòng lặp dịch).
- Màn Live có chỉ báo "đang đọc" bật/tắt theo lúc TTS phát; audio TTS không đi qua lớp UI.
- Người dùng tự chỉnh volume trong lúc Ducking → app không "đánh nhau" với thao tác đó: bỏ mức volume gốc đã lưu để phục hồi, giữ mức người dùng vừa chỉnh.
- Crash khi đang Ducking → lần mở sau phục hồi volume.
- Đổi thiết bị output → phát tiếp trên thiết bị mới trong ≤ 2 s; thiết bị Bluetooth tắt đột ngột → phát hiện và chuyển trong ≤ 3 s.

#### FR-22: Kết nối bền, trong suốt với người dùng
Kết nối tới model được duy trì qua giới hạn thời gian phía server (resumption, nén ngữ cảnh, reconnect khi server báo sắp đóng) và qua mọi lần mất kết nối phía máy người dùng (mất mạng, mạng chập chờn, đổi Wi-Fi, ngủ máy): app tự nối lại với thời gian chờ tăng dần và không cần người dùng thao tác; audio chưa được xác nhận (chưa có ack) sẽ được gửi lại sau khi reconnect.
**Hệ quả:**
- Phiên 60 phút với ≥ 6 lần reconnect không mất Segment nào; UI chỉ hiện chỉ báo trạng thái nhỏ, không lỗi.
- Mất kết nối vì bất kỳ lý do nào (timeout, lỗi mạng, server đóng) → app nối lại với backoff luỹ tiến 1 s → 2 s → 4 s → … tối đa 30 s giữa hai lần, có jitter; **không giới hạn số lần** trong khi Phiên còn chạy. Khi mạng có lại, Transcript tiếp tục trong ≤ 30 s + thời gian thiết lập kết nối, người dùng không phải bấm gì.
- Ngưỡng 5 lần liên tiếp chỉ áp dụng cho lỗi server từ chối setup (payload sai, key bị từ chối, model không tồn tại): đây là lỗi không tự hết, app báo lỗi và cho người dùng chọn tiếp tục ghi âm không transcript (chỉ Recording) hay dừng; Phiên vẫn lưu được và Transcribe lại sau.
- Audio thu trong lúc mất kết nối được giữ tối đa 60 s để gửi lại; phần vượt quá bị bỏ khỏi luồng gửi model và Transcript live ghi một Khoảng thiếu "mất kết nối mm:ss–mm:ss". Toàn bộ audio đó vẫn nằm trong Recording (FR-23) nên Transcribe lại lấp được.
- Chỉ báo kết nối có ba trạng thái phân biệt được: đang kết nối / đang nối lại (kèm thời gian đã chờ) / đã dừng transcript; luôn hiển thị cạnh chỉ báo "đang ghi âm" riêng.
- Lý do lỗi hiển thị đã được lọc (không lộ key, URL, transcript).
- UI không bao giờ hiển thị trạng thái "đang transcribe" khi không còn kết nối model nào sống; trạng thái Recording và trạng thái kết nối là hai chỉ báo riêng.

#### FR-23: Recording bền, sống sót qua crash
Recording WAV ghi liên tục vào Container, header vá định kỳ (~5 s) để file luôn phát được; khi dừng, finalize và tạo Proxy phát lại.
**Hệ quả:**
- Recording và đồng hồ Phiên không phụ thuộc trạng thái mạng: mất mạng, mất kết nối model, hết key hay lỗi Gemini đều không dừng ghi và không tạo khoảng trống trong file; chỉ người dùng bấm Dừng hoặc lỗi thiết bị audio mới dừng Recording.
- Force-quit giữa chừng → file phát được tới mốc ≤ 5 s trước khi chết.
- Phiên và file Recording được tạo khi mở thiết bị capture thành công, kể cả khi offline (Live bắt đầu được khi chưa có mạng; kết nối model nối sau theo FR-22). Lỗi trước khi capture mở → dọn sạch, không để dòng Phiên hay file chỉ có header.

#### FR-24: Phục hồi Phiên mồ côi khi mở app
Khi khởi động, app quét Phiên live chưa finalize, đưa vào thư viện với Recording và Segment đã tích luỹ, tạo Proxy phát lại nếu thiếu.
**Hệ quả:**
- Phiên phục hồi có badge "phục hồi" và cho Transcribe lại.
- Không hỏi người dùng; chạy nền, không chặn Home.

#### FR-25: Chặn thoát khi đang ghi
Đóng app/cửa sổ khi Phiên live đang chạy → hỏi xác nhận; nếu tiếp tục, app giữ tiến trình đủ lâu để finalize Recording và lưu Phiên.
**Hệ quả:**
- Không mất Phiên khi người dùng đóng bình thường.

#### FR-26: Lưu Phiên live vào thư viện, Transcribe lại
Khi dừng, app hiện overlay "Đang lưu phiên…" (finalize Recording, tạo Proxy) rồi điều hướng sang Transcript detail của Phiên vừa lưu; Phiên live lưu cùng schema với Phiên file, tên mặc định là nhãn thời gian cục bộ. Người dùng có thể Transcribe lại từ Recording; kết quả hiển thị cạnh bản live (side-by-side), giữ Memo/Tag/Ghi chú.
**Hệ quả:**
- Transcribe lại là một Job (FR-12) và tuân FR-15.
- Bản live không bị xoá khi Transcribe lại Partial.

### 4.5 Home — thư viện Phiên
**Mô tả:** Màn đầu sau Onboarding: drop-zone, nút Live, danh sách Phiên mới nhất trước, tìm kiếm, lọc theo Tag, Job đang chạy. Thực hiện UJ-4.

#### FR-27: Danh sách Phiên và Job đang chạy
Mỗi dòng: tên, ngày (múi giờ cục bộ), loại (file/live), Model đã dùng, số Segment, thời lượng, badge memo / có audio / partial / phục hồi. Job đang chạy hiện ở đầu với tiến độ và lối quay lại.
**Hệ quả:**
- Danh sách 500 Phiên hiển thị trong ≤ 1 s. `[ASSUMPTION: ngưỡng]`
- Xoá Phiên đang có Job → bị chặn cho tới khi Job xong/huỷ.

#### FR-28: Tìm kiếm theo tên
Ô tìm lọc theo tên Phiên, có nút xoá query; kết hợp với lọc Tag.
**Hệ quả:**
- Khớp không phân biệt hoa thường và dấu cách thừa; kết quả cập nhật khi gõ (≤ 200 ms với 500 Phiên).
- Query + bộ lọc Tag là AND; xoá query giữ nguyên bộ lọc Tag.
- Không có kết quả → trạng thái rỗng có nút xoá bộ lọc.

#### FR-29: Tag
Người dùng gắn nhiều Tag/Phiên (≤ 20, mỗi Tag ≤ 80 ký tự), chuẩn hoá trim + bỏ trùng không phân biệt hoa thường; lọc theo nhiều Tag (AND); lọc "Chưa gắn tag"; quick-picker Tag có sẵn; chế độ quản lý Tag để xoá một Tag khỏi mọi Phiên.
**Hệ quả:**
- Tag tồn tại qua đổi tên, Transcribe lại, chạy lại.
- Xoá Tag toàn cục là một thao tác, có xác nhận.

#### FR-30: Đổi tên, xoá Phiên
Đổi tên inline; xoá Phiên xoá toàn bộ dữ liệu liên quan (Transcript, Proxy, Recording, Memo, Ghi chú, liên kết Tag) khỏi Container sau xác nhận.
**Hệ quả:**
- Tên rỗng bị từ chối; tên ≤ 200 ký tự; Esc huỷ, Enter lưu.
- Sau xoá, dung lượng Container giảm tương ứng và không còn file mồ côi; Tag không còn Phiên nào vẫn tồn tại cho tới khi người dùng xoá Tag.

#### FR-31: Tải Recording
Với Phiên live, người dùng có thể lưu Recording ra ngoài dạng WAV hoặc FLAC qua dialog lưu (mục trong menu ⋯ của dòng Phiên ở Home).
**Hệ quả:**
- Không có lựa chọn M4A/AAC (v3 không dùng encoder AAC).

### 4.6 Transcript detail
**Mô tả:** Màn xem một Phiên: trình phát dùng Proxy phát lại, Transcript click-to-seek, tìm trong transcript, export, Memo, Ghi chú. Thực hiện UJ-3, UJ-4.

#### FR-32: Trình phát và click-to-seek
Trình phát audio dùng Proxy phát lại; click Segment → nhảy tới `start`; Segment đang phát được highlight; seek mượt trên cả macOS và Windows.
**Hệ quả:**
- Seek tới bất kỳ điểm nào trong file 90 phút có phản hồi ≤ 500 ms. `[ASSUMPTION: ngưỡng; phụ thuộc spike S8]`

#### FR-33: Tìm trong Transcript
Ô tìm với đếm số match, prev/next, highlight và cuộn tới match.
**Hệ quả:**
- Khớp không phân biệt hoa thường; đếm "n/N"; prev/next vòng tròn; Enter = next.
- Tìm trên Transcript 90 phút (~700 Segment) phản hồi ≤ 100 ms.

#### FR-34: Hiển thị Transcript, side-by-side
Segment có timestamp (đã cộng offset FR-13); speaker không hiển thị ở v3 (xem FR-14); Phiên có hai Transcript (live + Transcribe lại) xem được cạnh nhau hoặc chọn một.
**Hệ quả:**
- Timestamp hiển thị `HH:MM:SS` (bỏ giờ nếu < 1 h).
- Ở chế độ cạnh nhau, click Segment ở cột nào cũng seek trình phát; export/copy lấy Transcript đang được chọn.

#### FR-35: Export và copy
Export `.txt`, `.srt`, `.json` qua dialog lưu hệ thống; copy toàn bộ Transcript vào clipboard.
**Hệ quả:**
- Export áp dụng offset timestamp; `.json` giữ speaker.
- Phiên Partial export kèm ghi chú Khoảng thiếu trong `.txt`. `[ASSUMPTION]`

### 4.7 Ghi chú & Memo
**Mô tả:** Panel Ghi chú tự lưu, mở mặc định ở Live; Memo sinh từ Transcript + Ghi chú theo Template, cache theo Phiên, render Markdown. Thực hiện UJ-2, UJ-3.

#### FR-36: Ghi chú tự lưu
Textarea tự lưu (debounce ~800 ms) theo Phiên; có sẵn trong Live và Transcript detail.
**Hệ quả:**
- Không mất Ghi chú khi force-quit trong Live (lưu trước khi Phiên finalize).

#### FR-37: Template memo
Người dùng thêm/sửa/xoá/khôi phục mặc định Template memo; mỗi Template có tên và prompt chứa `{transcript}` bắt buộc, `{notes}` tuỳ chọn; bộ mặc định theo ngôn ngữ UI.
**Hệ quả:**
- Lưu Template thiếu `{transcript}` bị từ chối với lời giải thích.
- Khôi phục mặc định không xoá Template do người dùng tạo.

#### FR-38: Sinh, cache, sinh lại Memo
Người dùng chọn Template và sinh Memo; Memo lưu theo Phiên và Template; có thể sinh lại; hiển thị Markdown đã sanitize; copy và tải `.md`.
**Hệ quả:**
- Ghi chú được nhúng vào prompt với header theo ngôn ngữ UI khi Template có `{notes}`.
- Lỗi Memo không ảnh hưởng Transcript; lỗi phân loại (quota/auth/mạng) hiển thị rõ.
- Memo cũ vẫn hiện khi Transcript được chạy lại, kèm nhãn "Memo sinh từ bản trước". `[ASSUMPTION]`

### 4.8 Settings, lưu trữ, chẩn đoán, About
**Mô tả:** Settings tinh giản so với v2: chỉ còn nhóm Chung, Gemini, Chunking, Live, Memo, Lưu trữ, Đồng bộ cấu hình đề xuất, About/Privacy. Mỗi setting có tooltip trợ giúp.

#### FR-39: Các nhóm setting
Người dùng chỉnh: ngôn ngữ UI, theme (Theo hệ thống / Sáng / Tối); key (FR-5), model (FR-7), ngôn ngữ transcribe (FR-16); chunk phút, offset timestamp (FR-13); Target mặc định; Template memo (FR-37).
**Hệ quả:**
- Mọi trường có tooltip; giá trị không hợp lệ bị chặn tại chỗ.
- Ô key mặc định ẩn ký tự, có nút hiện/ẩn và nút "Kiểm tra key" (FR-3).
- Không còn nhóm engine, Whisper, Copilot, Hệ thống/wizard, Updates, thư mục cache.

#### FR-40: Lưu trữ trong Container *(mới ở v3)*
Settings hiển thị dung lượng đang dùng (media, DB), nút mở thư mục (nếu OS cho phép) và "Xoá toàn bộ dữ liệu" có xác nhận hai bước.
**Hệ quả:**
- Không có tuỳ chọn thư mục cache (Q7).
- `[ASSUMPTION: không tự động dọn Phiên cũ; người dùng tự xoá]`

#### FR-41: Nhật ký chẩn đoán content-free, xuất qua allow-list
App ghi Nhật ký chẩn đoán xoay vòng; người dùng có thể xuất gói Nhật ký chẩn đoán (chỉ file trong allow-list) và xoá Nhật ký chẩn đoán.
**Hệ quả:**
- Nhật ký chẩn đoán không bao giờ chứa Transcript, Bản dịch, Ghi chú, Memo, key; mọi chuỗi lỗi qua bộ lọc redaction (`AIza…`, `AQ.…`, URL, `authorization`).
- Test tự động quét Nhật ký chẩn đoán sau khi chạy Phiên mẫu phải không thấy nội dung.
- App đếm cục bộ số Phiên, số lỗi theo category và crash (không nội dung), hiển thị trong mục Chẩn đoán; v3 **không gửi thống kê đi đâu** và không có tuỳ chọn opt-in.

#### FR-42: Đồng bộ cấu hình đề xuất (tuỳ chọn)
Người dùng có thể tải và áp dụng bộ cấu hình đề xuất (model, chunk, template) từ endpoint của chúng ta; nội dung được ký và xác minh.
**Hệ quả:**
- Không bao giờ ghi đè key; hiển thị diff trước khi áp dụng. `[ASSUMPTION: có preview diff]`

#### FR-43: About & Privacy
Màn About hiện version, tác giả, liên hệ hỗ trợ, link Privacy Policy và link mở lại màn Consent để xem lại.

### 4.9 Quảng cáo tự vận hành (Ad slot)
**Mô tả:** Một Ad slot nhỏ ở góc dưới panel trên Home, Transcript detail và Settings; **không hiển thị ở màn Live**. Creative tải từ endpoint của chúng ta, xoay theo trọng số, cache 24 h, fallback creative nhúng sẵn khi offline. Không tracking, không identifier. Thực hiện UJ-5.

#### FR-44: Hiển thị Creative *(mới ở v3)*
Ad slot hiển thị một Creative (300×100 hoặc 320×50) theo locale UI và thời hạn; click mở trình duyệt ngoài; xoay Creative theo trọng số; mỗi Creative tối đa một lần hiển thị/10 phút.
**Hệ quả:**
- Không hiển thị ở màn Live (ẩn theo route Live; sau khi dừng app đã điều hướng sang Transcript detail — FR-26).
- Vị trí: đáy sidebar (sidebar rộng 260 px), theo `DESIGN.md`.
- Không có Creative hợp lệ và offline → dùng creative mặc định nhúng sẵn (giới thiệu trans-kun/Relipa).
- Creative không bao giờ che nội dung, không có âm thanh, không interstitial.

#### FR-45: Tuân thủ store cho quảng cáo *(mới ở v3)*
Ad slot có nhãn "Sponsored", nút "Báo cáo quảng cáo" (mở URL/mailto có sẵn `id` Creative) và "Vì sao tôi thấy quảng cáo này" (text tĩnh: theo ngôn ngữ UI, không theo dữ liệu cá nhân).
**Hệ quả:**
- Không gửi identifier, không cookie, không đo lường cá nhân; impression/click chỉ đếm cục bộ, không gửi đi.
- Cờ `ads_enabled` từ server tắt được toàn bộ Ad slot; cờ `is_premium` cục bộ (chưa có UI ở v3) cũng ẩn Ad slot.

#### FR-46: Tính toàn vẹn Creative *(mới ở v3)*
`ads.json` được ký; chữ ký sai → bỏ qua, dùng cache/fallback; ảnh Creative ≤ 100 KB, tải và cache trong Container, không tải script.

### 4.10 Đa ngôn ngữ UI
**Mô tả:** UI vi/en/ja đồng bộ; Template memo mặc định, nhãn thời gian, nội dung Consent, text Ad slot theo ngôn ngữ UI. Transcript và Bản dịch có thể trộn ja/vi/en.

#### FR-47: Bộ key i18n đồng bộ
Ba ngôn ngữ có cùng tập key; thiếu key ở bất kỳ ngôn ngữ nào bị chặn ở CI.
**Hệ quả:**
- Đổi ngôn ngữ UI áp dụng tức thì, không cần khởi động lại.

#### FR-48: Theme sáng/tối *(mới ở v3)*
UI có theme sáng và tối ngay trong bản submit đầu; mặc định theo hệ thống, người dùng ép được trong Settings → Chung (FR-39). Token màu ở `DESIGN.md`.
**Hệ quả:**
- Đổi theme áp dụng tức thì; theo hệ thống thì đổi theo khi OS đổi.
- Mọi cặp màu mang nội dung đạt WCAG AA ở cả hai theme (NFR-11).

## 5. Cross-cutting NFR

- **NFR-1 Riêng tư:** Dữ liệu họp chỉ đi tới Google Gemini bằng key người dùng. Không telemetry dưới bất kỳ hình thức nào (kể cả opt-in); chỉ đếm cục bộ (FR-41). Không server trung gian. Nhật ký chẩn đoán content-free (FR-41).
- **NFR-2 Không chặn lõi:** Lỗi Memo, Proxy phát lại, Ad slot, đồng bộ cấu hình không được làm hỏng Transcript hay Recording. Mọi tác vụ dài có cancel, progress, lỗi có category ổn định để UI dịch được.
- **NFR-3 Bền phiên:** Crash, force-quit, mất mạng, mạng chập chờn, server đóng kết nối không làm mất Recording và Segment đã tích luỹ; Recording không bao giờ dừng vì lý do mạng (FR-22–24).
- **NFR-4 Không phụ thuộc ngoài:** Không binary ngoài, không tải/chạy code lúc chạy, không quyền admin, không self-update. Một tiến trình.
- **NFR-5 Hiệu năng:** Decode + cắt Chunk ≥ 20× realtime trên máy 4 nhân; app mở tới Home ≤ 2 s; Live: độ trễ Transcript ≤ 2 s sau kết câu trong mạng bình thường. `[ASSUMPTION: các ngưỡng]`
- **NFR-6 Mạng doanh nghiệp:** Dùng CA store hệ thống (proxy/Zscaler); lỗi TLS/CA có thông báo riêng, hướng dẫn được.
- **NFR-7 Sandbox & store:** Chạy trong App Sandbox (macOS) và package identity (MSIX) với entitlement/capability tối thiểu: network client, mic, file do người dùng chọn. Không xin quyền Screen Recording.
- **NFR-8 Chi phí Gemini:** Chỉ ba luồng tốn token: transcribe file, live, memo. Không có gọi nền định kỳ. Chunk gửi inline, không upload trung gian.
- **NFR-9 Khả năng hiểu lỗi:** Mọi lỗi người dùng thấy thuộc một category hiển thị (quota/auth/model/mạng/CA/định dạng/quyền hệ thống/lưu trữ) với hành động gợi ý; category hiển thị bao trùm cả lỗi ngoài Gemini và ánh xạ từ phân loại lỗi API ở addendum §D; không hiện stack trace, không lộ key/URL.
- **NFR-10 Kích thước & tài nguyên:** App bundle ≤ 60 MB mỗi nền tảng; RAM khi Live ≤ 300 MB. `[ASSUMPTION: ngưỡng]`
- **NFR-11 Tiếp cận cơ bản:** Điều hướng bàn phím cho các thao tác chính (bắt đầu/dừng Live, tìm kiếm, export); tương phản đạt WCAG AA cho text ở cả theme sáng và tối. `[ASSUMPTION: mức tối thiểu, không có yêu cầu trình đọc màn hình đầy đủ]`
- **NFR-12 An toàn đường dẫn:** Không dùng chuỗi do người dùng hoặc server cung cấp (tên Phiên, Tag, tên Template, id Creative) làm thành phần đường dẫn file; mọi file trong Container đặt tên theo ID tự sinh.

## 6. Ràng buộc & guardrail

### 6.1 Privacy
- Consent trước mọi request tới Google (FR-2); Privacy Policy vi/en/ja công khai; privacy label: "Audio Data / User Content gửi tới Google để cung cấp tính năng, không liên kết danh tính, không tracking; Advertising Data: none".
- Key trong kho khoá OS (FR-5). Redaction mọi chuỗi chẩn đoán (FR-41).
- Ad slot không identifier, không tracking → không cần ATT.
- Lập trường GDPR/CCPA: chúng ta không thu thập hay lưu dữ liệu cá nhân nào (không tài khoản, không telemetry); dữ liệu họp do người dùng gửi tới Google theo điều khoản Gemini API của chính họ. Privacy Policy nêu rõ điều này và cách xoá dữ liệu cục bộ (FR-40).

### 6.2 Tuân thủ store
- **Apple:** App Sandbox (2.4.5); không tải/chạy code (2.5.2); không self-update; IAP cho mọi mở khoá trả phí (3.1.1, chưa dùng ở v3); quảng cáo có report/disclosure (2.5.18); Privacy Policy trong app và App Store Connect (5.1.1); consent bên thứ ba AI (5.1.2); `ITSAppUsesNonExemptEncryption=false`; key demo + hướng dẫn trong App Review Notes.
- **Microsoft:** MSIX qua Partner Center, Store ký; chính sách quảng cáo 10.x; Privacy Policy; key demo trong Notes for certification; WACK pass.
- Nội dung app không nhắc tới kênh tải ngoài store hay license riêng.
- Hồ sơ submit cho cả hai store (tên app, category, age rating, screenshot và mô tả vi/en/ja, Privacy Policy/support URL, key demo cho reviewer): danh sách đầy đủ và chứng chỉ/tài khoản ở addendum §J.

### 6.3 Chi phí
*(xem thêm NFR-8)*
- BYOK: chi phí Gemini do người dùng chịu; app phải minh bạch khi nào tốn token (badge/tooltip ở nút Sinh memo, Transcribe lại).
- Hạ tầng của chúng ta chỉ gồm trang tĩnh: Privacy Policy, `ads.json`, `recommended-settings.json`.

### 6.4 An toàn nội dung
- Markdown (Memo) được sanitize trước khi render; link mở trình duyệt ngoài.
- Creative chỉ là ảnh + text tĩnh; không HTML/script từ server.

## 7. Nền tảng

- **macOS:** ≥ 14.4 (Q3), universal (Apple Silicon + Intel), phân phối Mac App Store; system audio qua Core Audio process tap.
- **Windows:** Windows 10 1809+ và Windows 11, x64 + Arm64, phân phối Microsoft Store (MSIX); WebView2 Evergreen — app kiểm tra runtime lúc chạy và hướng dẫn cài nếu thiếu; system audio thu được cả app họp native lẫn họp trong trình duyệt. `[NOTE FOR PM: Arm64 chỉ được đưa vào bản submit đầu nếu có máy test thật trước Phase 6 (Câu hỏi mở 7); nếu không, submit x64 trước và bổ sung Arm64 sau.]`
- **Không:** Linux (Q6), kênh tải trực tiếp (Q5), tự cập nhật.
- **Beta nội bộ:** TestFlight for Mac, Microsoft Store package flight.

## 8. Kiến trúc thông tin (màn hình)

Onboarding (ngôn ngữ → Consent → key) → **Home** (drop-zone, nút Live, danh sách + tìm + Tag, Job đang chạy, Ad slot) → **Transcript detail** (trình phát, Transcript/side-by-side, tìm, export, Memo, Ghi chú, Ad slot) → **Live** (Nguồn, trạng thái kết nối, view Gốc/Dịch/Cả hai, Target, Nhận diện lại, TTS, Ghi chú; **không** Ad slot; Dừng → overlay "Đang lưu phiên…" → Transcript detail, nơi có export và Transcribe lại) → **Settings** (nhóm ở FR-39–42, Ad slot) → **About/Privacy**. SPA một cửa sổ; rời màn không mất Job/Phiên đang chạy.

## 9. Monetization

- v3: chỉ Ad slot house ads (Q4). Không mạng quảng cáo bên thứ ba (AdSense/AdMob không hỗ trợ desktop).
- Chừa sẵn: cờ `is_premium` ẩn Ad slot và trait nguồn key (KeyProvider) để phiên bản sau thêm Premium + đăng nhập Google (Q11). Không có UI, không có IAP ở v3.

## 10. Non-goals (rõ ràng)

- Không Whisper/WhisperX hay bất kỳ engine local nào; không chế độ offline.
- Không Copilot (Radar/Sniper/profile/skill) dưới bất kỳ hình thức nào (Q1).
- Không Linux; không DMG/MSI tải trực tiếp; không updater; không thư mục cache tuỳ chỉnh.
- Không import dữ liệu v2 (Q8); trans-kun là app mới độc lập, bundle ID mới.
- Không tài khoản người dùng, không đồng bộ cloud, không backend lưu dữ liệu họp.
- Không telemetry/analytics dưới bất kỳ hình thức nào (kể cả opt-in ẩn danh); không tracking quảng cáo.
- Không hiển thị speaker label trên UI.
- Không Premium/IAP/đăng nhập Google ở v3 (xem §9).
- Không hỗ trợ `avi/wmv/flv/ts`; không cam kết Opus cho tới khi Open Question 1 chốt.
- Không tự làm license/bản quyền riêng.

## 11. Phạm vi MVP

### 11.1 Trong phạm vi (bản submit store đầu tiên)
Toàn bộ §4 (FR-1–48), §5, §6, §7. Cụ thể: Onboarding + Consent; key pool + keychain; Transcribe file (model generateContent, Khoảng thiếu, Proxy trong Container); Live (system/mic/mixed, dịch, đổi Target, Nhận diện lại, TTS + Ducking, reconnect trong suốt, Recording bền, phục hồi mồ côi, Transcribe lại); Home (tìm, Tag, rename, delete, tải Recording, Job đang chạy); Transcript detail (player, seek, tìm, side-by-side, export); Ghi chú + Memo + Template; Settings tinh giản + chẩn đoán + đồng bộ cấu hình; Ad slot house ads; i18n vi/en/ja; theme sáng/tối; build store-mac + store-win.

### 11.2 Ngoài phạm vi MVP
- Premium, đăng nhập Google, IAP — phiên bản sau (xem §9). `[NOTE FOR PM: nếu review Apple phản đối BYOK, phương án key do Relipa cấp (addendum §K) phải được kéo lên sớm]`
- Sponsor bên ngoài cho Ad slot — sau khi có người dùng; v3 chỉ house creative.
- Proxy AAC/M4A (encoder OS) — chỉ nếu spike FLAC (S8) thất bại; mặc định FLAC (Q2).
- Fallback ScreenCaptureKit cho macOS < 14.4 — không làm.
- Opus trong webm/mkv — theo Open Question 1.

## 12. Tiêu chí thành công (định tính)

Chủ sản phẩm chọn không đặt chỉ số định lượng cho v3. Thành công được đánh giá bằng:

- **SM-1 Lên được cả hai store** với build sandbox/MSIX, không phải sửa kiến trúc để qua review. Xác nhận FR-2, FR-5, FR-17, FR-45, §6.2.
- **SM-2 Không còn ticket hỗ trợ về môi trường** (Python, ffmpeg, PATH, quyền admin, Screen Recording). Xác nhận NFR-4, NFR-7, FR-17.
- **SM-3 Người dùng v2 chuyển sang v3 mà không mất năng lực cốt lõi** đã đánh dấu GIỮ trong doc 01 (transcribe file, live + dịch, memo, tag/tìm). Xác nhận §4.
- **SM-4 Không mất dữ liệu họp** trong các kịch bản crash/force-quit/mất mạng của UJ-2. Xác nhận FR-22–24.
- **SM-5 Quảng cáo không làm xấu trải nghiệm B2B:** không che nội dung, không xuất hiện trong Live, có thể báo cáo. Xác nhận FR-44–45.

**Counter-metric (không tối ưu):**
- **SM-C1 Số impression quảng cáo** — không tăng bằng cách nâng tần suất, thêm vị trí (nhất là Live) hay che nội dung. Đối trọng SM-5.
- **SM-C2 Tốc độ transcribe** — không tăng bằng cách cache Transcript Partial hay bỏ retry. Đối trọng SM-4/FR-15.

## 13. Câu hỏi mở

1. **Opus** trong webm/mkv: hỗ trợ (thêm decoder) hay từ chối rõ? Chốt sau spike S1.
2. **Model generateContent được chọn** có nhận FLAC inline kèm JSON schema trong giới hạn payload không? Kiểm chứng S2; không dùng Files API cho luồng file.
3. **Phát FLAC trong WebView** (WKWebView/WebView2) có seek mượt không? Chốt sau spike S8; nếu không, FR-32 chuyển sang AAC native hoặc player trong Rust.
4. **Key demo cho reviewer**: ai sở hữu, quota bao nhiêu, xoay thế nào khi hết hạn?
5. **Tên "trans-kun" trên App Store Connect / Partner Center** còn trống không? Reserve sớm.
6. **Privacy Policy & endpoint** (`ads.json`, `recommended-settings.json`) đặt ở LP repo hiện tại hay LP mới cho trans-kun?
7. **Máy test Windows Arm64** có sẵn không?
8. **Nội dung Template memo mặc định** theo ba ngôn ngữ: lấy nguyên từ v2 hay viết lại?

## 14. Chỉ mục giả định

- FR-2 — chỉ một cấp Consent, không tách audio/transcript.
- FR-8 — kéo nhiều file xử lý tuần tự.
- FR-18 — ngưỡng độ trễ 2 s.
- FR-22 — backoff tối đa 30 s và buffer gửi lại 60 s là giá trị chủ sản phẩm duyệt theo đề xuất, chưa đo thực tế.
- FR-27 — ngưỡng 500 Phiên/1 s.
- FR-32 — ngưỡng seek 500 ms.
- FR-35 — `.txt` Partial kèm ghi chú Khoảng thiếu.
- FR-38 — Memo cũ giữ với nhãn khi Transcript chạy lại.
- FR-40 — không tự dọn Phiên cũ.
- FR-42 — có preview diff trước khi áp dụng cấu hình đề xuất.
- NFR-5, NFR-10, NFR-11 — các ngưỡng hiệu năng, kích thước, tiếp cận.
