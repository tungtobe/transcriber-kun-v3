---
id: SPEC-transcriber_kun
companions:
  - glossary.md
  - requirements.md
  - store-compliance.md
  - ../../planning-artifacts/prds/prd-transcriber_kun-2026-09-17/addendum.md
  - ../../planning-artifacts/ux-designs/ux-transcriber_kun-2026-09-18/DESIGN.md
  - ../../planning-artifacts/ux-designs/ux-transcriber_kun-2026-09-18/EXPERIENCE.md
  - ../../planning-artifacts/architecture/architecture-transcriber_kun-2026-09-18/ARCHITECTURE-SPINE.md
sources:
  - ../../planning-artifacts/prds/prd-transcriber_kun-2026-09-17/prd.md
---

> **Canonical contract.** This SPEC and the files in `companions:` are the complete, preservation-validated contract for what to build, test, and validate. Source documents listed in frontmatter are for traceability — consult them only if you need narrative rationale or prose color this contract intentionally omits.

# trans-kun v3

## Why

**Tầm nhìn** kết hợp với **nghĩa vụ phải đáp ứng**. Người Việt làm việc với khách Nhật (BrSE, sales) cần ghi lại và hiểu buổi họp ngay lập tức: transcript realtime có bản dịch, transcribe file ghi âm, và sinh 議事録/memo. Transcriber-kun v2.2.2 làm được việc này. Tuy vậy, phần lớn ticket hỗ trợ của v2 đến từ môi trường: bootstrap Python, ffmpeg, PATH, quyền admin, quyền Screen Recording. v2 cũng không lên được store.

v3 build lại từ đầu với lời hứa **"cài từ store, mở lên là dùng"**: một tiến trình Rust, không phụ thuộc ngoài, người dùng tự mang Gemini key (BYOK), dữ liệu họp chỉ đi từ máy người dùng tới Google. Việc phát hành trên Mac App Store và Microsoft Store đưa app ra khỏi phạm vi nội bộ Relipa, nhưng cũng bắt app đạt chuẩn sandbox, privacy và review của store. Đổi lại, phạm vi hẹp hơn v2: chỉ dùng Gemini, bỏ Whisper, Copilot, Linux và kênh tải trực tiếp. Một ad slot house-ads nhỏ mở đường doanh thu/cross-promotion mà không làm xấu trải nghiệm B2B.

## Capabilities

Hệ quả kiểm thử được theo từng FR nằm ở `requirements.md`. Nhãn FR-n giữ nguyên số của PRD (FR-1–48).

- **CAP-1** — Onboarding & Consent (FR-1–4)
  - **intent:** Người dùng mới chọn ngôn ngữ UI, đồng ý gửi dữ liệu tới Google, nhập và kiểm tra key ngay tại chỗ. Khi chưa có key, mọi màn vẫn dùng được.
  - **success:** Không có request nào tới Google trước khi có Consent (kể cả kiểm tra key). Key sai hiện lỗi đã phân loại ngay ở Onboarding. Không màn trắng hay crash khi chưa có key.

- **CAP-2** — Key pool & model (FR-5–7)
  - **intent:** Người dùng lưu một hoặc nhiều key an toàn. Khi hết quota hoặc key bị từ chối, app tự xoay key. Model chọn riêng cho từng mục đích.
  - **success:** File settings không chứa key. 429 → key đó nghỉ 60 s và request chuyển sang key kế. 401/403 → loại key khỏi vòng. Timeout và 400/404 không làm request chuyển sang key khác. Model lỗi hiện cảnh báo tại chỗ và không tự đổi model.

- **CAP-3** — Transcribe file (FR-8–16)
  - **intent:** Người dùng kéo/chọn file audio/video và nhận Transcript có timestamp tuyệt đối. Phiên không phụ thuộc file nguồn. App không bao giờ giấu Khoảng thiếu.
  - **success:** File 90 phút cho ra Transcript liền mạch. Chunk lỗi hẳn thành Khoảng thiếu, người dùng chạy lại được riêng phần đó. Định dạng không hỗ trợ bị từ chối trước khi tạo Phiên. Xoá file nguồn xong vẫn phát và seek được.

- **CAP-4** — Live transcript & dịch (FR-16–20)
  - **intent:** Người dùng xem transcript gốc và Bản dịch realtime từ âm thanh hệ thống, mic hoặc cả hai. Đổi Nguồn, đổi Target, chọn "Không dịch" hoặc Nhận diện lại đều không ngắt Phiên.
  - **success:** Thu được Zoom, Teams và Meet-trong-Chrome trên cả hai OS mà không xin Screen Recording. Segment hiện ≤ 2 s sau khi câu kết thúc. Đổi Nguồn có hiệu lực ≤ 1 s. Timestamp không nhảy và không có Segment lặp.

- **CAP-5** — Đọc bản dịch (TTS) có Ducking (FR-21)
  - **intent:** Người dùng nghe Bản dịch qua thiết bị output mặc định, volume hệ thống tự hạ khi TTS nói.
  - **success:** App không tự thu lại âm TTS. Volume hạ còn 30 % rồi phục hồi, không "đánh nhau" với thao tác chỉnh tay. Đổi thiết bị → phát tiếp ≤ 2 s (Bluetooth tắt đột ngột ≤ 3 s). Crash giữa lúc Ducking → lần mở sau phục hồi volume.

- **CAP-6** — Độ bền Phiên live (FR-22–25)
  - **intent:** Recording và Segment đã tích luỹ sống qua mất mạng, server đóng kết nối, crash, force-quit và đóng app. Kết nối model tự nối lại, người dùng không phải thao tác.
  - **success:** Phiên 60 phút có ≥ 6 lần reconnect không mất Segment. Rớt mạng 3 phút → Recording liên tục, Transcript ghi một Khoảng thiếu "mất kết nối". Force-quit → Recording phát được tới ≤ 5 s trước khi chết, và Phiên mồ côi tự vào thư viện ở lần mở sau.

- **CAP-7** — Lưu Phiên live & Transcribe lại (FR-26)
  - **intent:** Phiên live lưu cùng schema với Phiên file. Người dùng có thể Transcribe lại từ Recording để có bản chất lượng cao hơn, đặt cạnh bản live.
  - **success:** Transcribe lại là một Job lấp được các Khoảng thiếu, giữ Memo/Tag/Ghi chú. Bản Partial không bao giờ thay bản live.

- **CAP-8** — Home: thư viện Phiên (FR-27–31)
  - **intent:** Người dùng thấy các Phiên và Job đang chạy, tìm theo tên, lọc theo Tag, đổi tên, xoá, và tải Recording ra ngoài.
  - **success:** 500 Phiên hiển thị ≤ 1 s. Tìm kiếm cập nhật ≤ 200 ms. Xoá Phiên không để lại file mồ côi. Không xoá được Phiên đang có Job.

- **CAP-9** — Transcript detail (FR-32–35)
  - **intent:** Người dùng nghe lại, click Segment để seek, tìm trong transcript, so sánh bản live với bản Transcribe lại, export và copy.
  - **success:** Seek trong file 90 phút ≤ 500 ms. Tìm trên ~700 Segment ≤ 100 ms. Export txt/srt/json áp dụng offset timestamp.

- **CAP-10** — Ghi chú & Memo (FR-36–38)
  - **intent:** Người dùng ghi chú tự lưu trong và sau họp, rồi sinh Memo Markdown từ Transcript + Ghi chú theo Template tự quản lý.
  - **success:** Force-quit trong Live không mất Ghi chú. Template thiếu `{transcript}` bị từ chối. Lỗi Memo không ảnh hưởng Transcript.

- **CAP-11** — Settings, lưu trữ, chẩn đoán, About (FR-39–43)
  - **intent:** Người dùng chỉnh một bộ setting tinh giản, xem và xoá dữ liệu trong Container, xuất log chẩn đoán không chứa nội dung, và áp dụng cấu hình đề xuất đã ký.
  - **success:** Test tự động quét log sau một Phiên mẫu không tìm thấy nội dung hay key. Cấu hình đề xuất hiện diff trước khi áp dụng và không bao giờ ghi đè key.

- **CAP-12** — Ad slot house ads (FR-44–46)
  - **intent:** App hiển thị một Creative tự vận hành ở mức kín đáo, không tracking, tuân thủ quy định quảng cáo của hai store.
  - **success:** Không hiện ở màn Live. Có nhãn Sponsored, nút Báo cáo và "Vì sao tôi thấy quảng cáo này". Chữ ký `ads.json` sai → dùng cache/fallback. Cờ `ads_enabled` tắt được toàn bộ.

- **CAP-13** — Đa ngôn ngữ & theme (FR-47–48)
  - **intent:** UI chạy ba ngôn ngữ vi/en/ja và hai theme sáng/tối, đổi tức thì.
  - **success:** CI chặn khi bộ key i18n lệch giữa ba ngôn ngữ. Mọi cặp màu mang nội dung đạt WCAG AA ở cả hai theme.

## Constraints

- **Chỉ phân phối qua store:** Mac App Store (macOS ≥ 14.4, universal) và Microsoft Store (MSIX, Windows 10 1809+/11, x64; Arm64 có điều kiện). Chi tiết ở `store-compliance.md`.
- **Một tiến trình Rust:** không Python, ffmpeg, sidecar hay binary ngoài; không tải/chạy code lúc chạy; không cần quyền admin; không self-update.
- **Sandbox tối thiểu:** chỉ network client, mic, system audio, file do người dùng chọn. **Không bao giờ xin Screen Recording.** System audio lấy qua Core Audio process tap (macOS) và WASAPI loopback (Windows).
- **BYOK, chỉ dùng Gemini:** dữ liệu họp chỉ đi tới Google bằng key của người dùng. Không có server trung gian, không tài khoản. Hạ tầng của chúng ta chỉ gồm trang tĩnh: Privacy Policy, `ads.json`, `recommended-settings.json`.
- **Consent đi trước mọi request tới Google.** Consent có số phiên bản cố định trong binary.
- **Không telemetry ở v3:** chỉ đếm cục bộ (Phiên, lỗi theo category, crash), không gửi đi đâu.
- **Bí mật và log:** key nằm trong kho khoá của OS. Log chẩn đoán content-free; mọi chuỗi lỗi đi qua bộ lọc redaction.
- **Recording không phụ thuộc mạng:** chỉ dừng khi người dùng bấm Dừng hoặc thiết bị audio lỗi. Live bắt đầu được cả khi offline (Phiên tạo lúc mở capture thành công).
- **Transcript Partial không bao giờ được coi là hoàn chỉnh,** và không bao giờ ghi đè bản live.
- **Cô lập lỗi:** lỗi ở Memo, Proxy, Ad slot hay cấu hình đề xuất không được làm hỏng Transcript hay Recording.
- **Token chỉ tốn ở ba luồng** do người dùng khởi động: transcribe file, live, memo. Không có gọi nền định kỳ. Chunk gửi inline, không upload trung gian.
- **Lỗi hiển thị:** mọi lỗi thuộc một category ổn định kèm hành động gợi ý; không hiện stack trace, key, URL hay transcript.
- **Mạng doanh nghiệp:** dùng CA store của hệ thống; lỗi TLS/CA có thông báo riêng.
- **Đường dẫn file:** chuỗi do người dùng hoặc server cung cấp không bao giờ thành một phần đường dẫn; file trong Container đặt tên theo ID tự sinh.
- **Thứ tự ưu tiên tài liệu:** SPEC + `requirements.md` quyết định *làm gì*. `ARCHITECTURE-SPINE.md` quyết định *làm thế nào* và thắng `addendum.md` khi mâu thuẫn. `DESIGN.md`/`EXPERIENCE.md` sở hữu thị giác và hành vi UI. Các quyết định Q1–Q11 là đầu vào cố định.

## Non-goals

- Không Whisper/WhisperX hay engine local nào; không chế độ offline transcribe.
- Không Copilot dưới bất kỳ hình thức nào.
- Không Linux; không DMG/MSI tải trực tiếp; không updater; không thư mục cache tuỳ chỉnh.
- Không import dữ liệu v2. trans-kun là app mới, bundle ID `com.transkun.app`.
- Không tài khoản, đồng bộ cloud, hay backend lưu dữ liệu họp.
- Không telemetry hay analytics (kể cả opt-in); không tracking quảng cáo; không mạng quảng cáo bên thứ ba; không sponsor ngoài ở v3.
- Không Premium, IAP hay đăng nhập Google. Chỉ chừa trait `KeyProvider` và cờ `is_premium`, không có UI.
- Không hiển thị speaker label trên UI.
- Không hỗ trợ `avi/wmv/flv/ts`; chưa cam kết Opus.
- Không fallback ScreenCaptureKit cho macOS < 14.4; không proxy AAC trừ khi spike S8 thất bại.
- Không quản trị tập trung (SSO, key do công ty cấp, audit).
- Không tự làm license/bản quyền riêng.

## Success signal

- **Lên được cả hai store:** bản submit đầu (sandbox/MSIX) qua review Apple và Microsoft mà không phải sửa kiến trúc. Reviewer đi hết UJ-5 bằng key demo, không gặp màn trắng, không bị xin quyền lạ.
- **Diễn lại UJ-2 trên máy thật không mất gì:** họp Teams 60 phút, ≥ 6 lần server đóng kết nối, rớt Wi-Fi 3 phút, force-quit ở phút 50. Kết quả: Recording tới phút 50, Segment đã tích luỹ và Ghi chú còn nguyên; Transcribe lại lấp được Khoảng thiếu.
- **Hết ticket môi trường:** không còn ticket về Python, ffmpeg, PATH, quyền admin hay Screen Recording.
- **Người dùng v2 chuyển sang không mất năng lực cốt lõi:** transcribe file, live + dịch, memo, tag/tìm.
- **Ràng buộc đối trọng:** không tăng impression bằng cách tăng tần suất, thêm vị trí (nhất là Live) hay che nội dung. Không tăng tốc transcribe bằng cách cache Partial hay bỏ retry.

## Assumptions

- Các ngưỡng số chưa đo thực tế nên được giữ thẻ `[ASSUMPTION]` trong `requirements.md`: độ trễ Live 2 s, 500 Phiên/1 s, seek 500 ms, app mở ≤ 2 s, bundle ≤ 60 MB, RAM ≤ 300 MB, decode ≥ 20× realtime, backoff tối đa 30 s, buffer gửi lại 60 s (hai giá trị cuối đã được chủ sản phẩm duyệt).
- Consent chỉ có một cấp; kéo nhiều file thì xử lý tuần tự; không tự dọn Phiên cũ; `.txt` của Phiên Partial kèm ghi chú Khoảng thiếu; Memo cũ được giữ kèm nhãn khi chạy lại.

## Open Questions

- **Q1:** Opus trong webm/mkv sẽ được hỗ trợ (thêm libopus, không còn thuần Rust) hay bị từ chối rõ ràng? Chốt sau spike S1.
- **Q2:** Model `*-transcribe` có nhận audio inline không, hay cần Files API? Chốt sau spike S2; ảnh hưởng FR-14.
- **Q3:** FLAC có seek mượt trong WKWebView/WebView2 không? Chốt sau spike S8. Nếu không, player chuyển sang AAC native hoặc player trong Rust.
- **Q4:** Ai sở hữu key demo cho reviewer, quota bao nhiêu, xoay thế nào khi hết hạn? Đây là rủi ro trực tiếp cho tiêu chí lên store.
- **Q5:** Tên "trans-kun" trên App Store Connect / Partner Center còn trống không?
- **Q6:** Privacy Policy, `ads.json`, `recommended-settings.json` đặt ở LP repo hiện tại hay một LP mới?
- **Q7:** Có máy test Windows Arm64 thật trước Phase 6 không? Nếu không, bản submit đầu chỉ có x64.
- **Q8:** Template memo mặc định cho ba ngôn ngữ lấy nguyên từ v2 hay viết lại?
