# Requirements — trans-kun v3

Hệ quả kiểm thử được, nhóm theo CAP của `SPEC.md`. Số FR/NFR giữ theo PRD. *(mới)* = năng lực v2 chưa có. `[ASSUMPTION]` = ngưỡng hoặc chi tiết chưa được xác nhận. **[override]** = đổi so với bản PRD 2026-09-17 theo quyết định UX/architecture 2026-09-18; đã cập nhật ngược vào PRD.

## CAP-1 Onboarding & Consent

Luồng lần đầu (hoặc khi thiếu Consent/key) gồm: ngôn ngữ UI → Consent → API key. Không có bước kiểm tra môi trường.

- **FR-1 Ngôn ngữ UI.** Chọn vi/en/ja; mặc định theo ngôn ngữ hệ thống, fallback `en`. Đổi là áp dụng ngay cho Onboarding. Giá trị được lưu và dùng cho Template mặc định, nhãn thời gian, Creative.
- **FR-2 Consent** *(mới)*. Không có request nào tới Google trước khi có Consent, kể cả kiểm tra key. Màn Consent nêu tên bên nhận (Google) và có link Privacy Policy mở trong trình duyệt ngoài. Consent lưu kèm số phiên bản văn bản (hằng compile-time); chỉ hỏi lại khi số phiên bản tăng. Từ chối → app chỉ cho xem Settings/About, có nút quay lại đồng ý. `[ASSUMPTION: một cấp Consent]`
- **FR-3 Nhập & kiểm tra key.** Nhận một hoặc nhiều key, định dạng `AIza…` và `AQ.…`. App kiểm tra bằng cách tải danh sách model. Lỗi được phân loại: key không hợp lệ / mạng / CA công ty / quota, kèm hướng dẫn. Key hợp lệ → hiện số model khả dụng. Có nút "Bỏ qua, nhập sau".
- **FR-4 Chưa có key.** Mọi màn vẫn mở được; nút cần Gemini bị vô hiệu kèm giải thích và lối tắt tới nơi nhập key. Không màn trắng, không crash, không dialog lặp. Danh sách Phiên và phát lại vẫn dùng được.

## CAP-2 Key pool & model

- **FR-5 Lưu key an toàn** *(mới)*. Key nằm trong Keychain / Credential Manager, file settings không chứa key. Xoá key trên UI thì xoá khỏi kho khoá. Không truy cập được kho khoá → hiện lỗi dễ hiểu, các setting khác không mất.
- **FR-6 Xoay vòng key.** Một chính sách chung cho mọi luồng Gemini:
  - 429 → key đó nghỉ 60 s, request thử lại bằng key kế.
  - 401/403 → loại key khỏi vòng cho tới khi người dùng sửa, thử key kế.
  - 400/404 và timeout → không xoay, báo lỗi. Timeout không bao giờ khiến cùng request được gửi sang key khác.
  - Mọi key đều đang nghỉ → Job chờ, tối đa 180 s/Chunk.
  - Hết key vì 401/403 → lỗi category Auth kèm lối tắt tới Settings.
- **FR-7 Model theo mục đích.** Mặc định `gemini-flash-lite-latest` cho transcribe/memo, `gemini-3.5-live-translate-preview` cho live. Danh sách live chỉ gồm model hỗ trợ Live. Tên ngoài danh sách vẫn được chấp nhận, kèm cảnh báo nhẹ. Google trả lỗi category Model → cảnh báo tại chỗ (không chỉ ghi log) kèm lối tắt tới Settings; Job/Phiên không tự đổi Model.

## CAP-3 Transcribe file

- **FR-8 Nhận file.** Qua dialog hệ thống hoặc kéo thả vào Home/Transcript; chuyển sang màn Transcript và bắt đầu ngay. Chỉ cần quyền file do người dùng chọn. Kéo nhiều file → mỗi file một Phiên, xử lý tuần tự. `[ASSUMPTION: tuần tự]`
- **FR-9 Định dạng.**
  - Hỗ trợ: mp3, m4a, wav, flac, ogg/vorbis, aiff, caf; video mp4/mov/mkv/webm (chỉ lấy track audio).
  - `avi/wmv/flv/ts` → từ chối trước khi tạo Phiên, thông báo nêu định dạng nên chuyển sang.
  - Opus trong webm/mkv: hỗ trợ hoặc từ chối rõ ràng tuỳ Q1, không được lỗi im lặng.
  - Danh sách định dạng hiện trong dialog chọn file và trong thông báo lỗi.
- **FR-10 Không phụ thuộc file nguồn** *(mới)*. Tạo Proxy phát lại ngay khi có Phiên. Phát, seek, Transcribe lại và export đều dùng Proxy. Xoá hay di chuyển file nguồn không ảnh hưởng. Mở lại app trong sandbox vẫn phát được. Proxy hỏng/thiếu → vẫn mở được Transcript; trình phát báo "không có audio" và cho chọn lại file nguồn để tạo lại Proxy.
- **FR-11 Trùng lặp.** Cùng hash SHA-256 của nội dung → mở ngay Phiên có sẵn, không gọi Gemini. "Chạy lại" là thao tác chủ động trong Transcript detail: giữ Tag/Ghi chú/Memo, chỉ ghi đè Transcript sau khi bản mới hoàn tất, không tạo Phiên mới.
- **FR-12 Job.** Progress tính theo phút audio đã xử lý trên tổng; có nút huỷ và panel log. Rời màn rồi quay lại vẫn thấy Job; Home hiện Job đang chạy. Huỷ → ngừng gửi Chunk mới trong ≤ 2 s, dọn dữ liệu tạm, không lưu Phiên dở. Đóng app khi Job đang chạy → hỏi xác nhận; nếu đóng thì Job bị huỷ sạch, không chạy nền.
- **FR-13 Chunk & timestamp.** Chunk dài `chunkMinutes` (mặc định 5, tối thiểu 1). Segment `MM:SS` tương đối trong Chunk được cộng offset tuyệt đối. Segment sau không bao giờ lùi thời gian so với Segment trước. Chunk quá lớn để gửi inline → tự chia nhỏ hơn (ví dụ 3 phút) và ghi log. Có offset timestamp toàn cục (giây), áp dụng khi hiển thị/export.
- **FR-14 Hai họ model.** Model tổng quát: prompt yêu cầu Segment 5–15 s. Model `*-transcribe`: word gộp thành Segment ≈ 8 s (tối đa 15 s), cắt khi đổi speaker. App tự chọn cách gọi theo tên model. **[override]** speaker chỉ lưu trong data và export `.json`, không hiện ở UI.
- **FR-15 Khoảng thiếu.**
  - Mỗi Chunk thử tối đa 4 lần theo FR-6; 5xx được thử lại; 400/404 hoặc nội dung bị chặn → Chunk fail ngay.
  - JSON vỡ → parser cứu các Segment hợp lệ, phần còn lại thành Khoảng thiếu.
  - UI liệt kê từng Khoảng thiếu, cho "Chạy lại phần thiếu" hoặc "Chạy lại toàn bộ".
  - Phiên có badge `partial` cho tới khi đầy đủ. Partial không bao giờ được lưu như hoàn chỉnh.
- **FR-16 Ngôn ngữ transcribe.** auto/ja/vi/en, mặc định auto, đặt trong Settings. Áp dụng cho cả file và Live.

## CAP-4 Live transcript & dịch

- **FR-17 Nguồn audio.**
  - Chọn `system`, `mic:<tên>` hoặc `mixed:<mic>`; dropdown mic có sẵn mặc định và nút làm mới.
  - Đổi Nguồn giữa phiên có hiệu lực ≤ 1 s; đồng hồ và Recording liên tục.
  - macOS: chỉ xin System Audio Recording + Micro, không bao giờ xin Screen Recording, không thu lại TTS của chính app.
  - Windows: thu được cả app họp native (endpoint communications) lẫn họp trong trình duyệt (endpoint console).
  - Kiểm thử chấp nhận: Zoom, Teams, Meet-trong-Chrome trên cả hai OS.
  - Thiếu quyền → hướng dẫn trong app, kèm đường dẫn tới System Settings; Phiên chưa bắt đầu cho tới khi có quyền hoặc người dùng đổi Nguồn.
- **FR-18 Realtime.** Text stream theo token, tách câu ở dấu kết câu, timestamp theo đồng hồ capture cục bộ. Segment hiện ≤ 2 s sau khi câu kết thúc `[ASSUMPTION]`. Tự cuộn theo dòng mới, dừng tự cuộn khi người dùng cuộn lên. Segment được lưu tạm định kỳ để phục hồi.
- **FR-19 Dịch.**
  - View Gốc / Dịch / Cả hai.
  - Target đổi giữa phiên không ngắt Transcript gốc; Bản dịch mới bắt đầu từ câu kế tiếp.
  - Speech đã ở ngôn ngữ Target vẫn hiện trong pane Dịch. Target đã chọn thì luôn dịch, kể cả khi trùng ngôn ngữ nguồn khai báo.
  - Bản dịch không được lưu.
  - "Không dịch" → không yêu cầu Bản dịch hay audio TTS (không tốn token dịch); view chỉ còn Gốc; nút TTS bị vô hiệu.
- **FR-20 Nhận diện lại.** Chỉ bật khi ngôn ngữ là `auto` (ngoài ra ẩn hoặc vô hiệu). Không có Segment lặp hay nhảy timestamp. Sự kiện từ kết nối cũ sau thời điểm chuyển bị bỏ; kết nối cũ đóng ≤ 1 s sau khi kết nối mới sẵn sàng. Kết nối mới lỗi → giữ kết nối cũ, báo lỗi nhẹ.

## CAP-5 TTS & Ducking

- **FR-21.** Bật/tắt đọc Bản dịch. Audio phát trong tiến trình app, trên thiết bị output mặc định, không đi qua UI. Có chỉ báo "đang đọc". Ducking hạ volume hệ thống còn 30 %, phục hồi sau. Người dùng tự chỉnh volume lúc đang Ducking → bỏ mức gốc đã lưu, giữ mức người dùng vừa chỉnh. Crash giữa lúc Ducking → lần mở sau phục hồi volume. Đổi thiết bị output → phát tiếp ≤ 2 s; Bluetooth tắt đột ngột → phát hiện và chuyển ≤ 3 s.

## CAP-6 Độ bền Phiên live

- **FR-22 Kết nối trong suốt.**
  - App duy trì kết nối qua giới hạn phía server (resumption, nén ngữ cảnh, reconnect khi nhận `goAway`) và qua mọi lần mất mạng, mạng chập chờn, đổi Wi-Fi hay máy ngủ.
  - Backoff 1 → 2 → 4 … tối đa 30 s, jitter ±20 %, không giới hạn số lần khi Phiên còn chạy. Mạng có lại → Transcript tiếp tục trong ≤ 30 s + thời gian thiết lập kết nối.
  - Audio chưa được ack sẽ gửi lại, giữ tối đa 60 s. Phần vượt quá thành Khoảng thiếu "mất kết nối mm:ss–mm:ss"; audio đó vẫn nằm trong Recording.
  - Ngưỡng 5 lần liên tiếp chỉ áp dụng cho lỗi server từ chối setup (payload sai, key bị từ chối, model không tồn tại) → báo lỗi, cho chọn "tiếp tục chỉ ghi âm" hoặc dừng.
  - Phiên 60 phút có ≥ 6 lần reconnect không mất Segment nào.
  - Chỉ báo kết nối có ba trạng thái (đang kết nối / đang nối lại + thời gian chờ / đã dừng transcript), tách riêng khỏi chỉ báo "đang ghi âm". UI không bao giờ báo "đang transcribe" khi không còn kết nối sống. Lý do lỗi hiển thị đã qua redaction.
- **FR-23 Recording bền.**
  - WAV ghi liên tục, vá header ~5 s một lần; force-quit → phát được tới mốc ≤ 5 s trước khi chết.
  - Mạng, model, key hay lỗi Gemini đều không dừng Recording và không tạo khoảng trống trong file; chỉ bấm Dừng hoặc lỗi thiết bị audio mới dừng.
  - **[override]** Phiên + Recording được tạo khi mở thiết bị capture thành công (Live bắt đầu được khi offline). Lỗi trước bước đó → dọn sạch, không để lại file chỉ có header.
  - Dừng → finalize và tạo Proxy phát lại.
- **FR-24 Phiên mồ côi.** Lúc khởi động, Phiên live chưa finalize được đưa vào thư viện cùng Recording và các Segment đã tích luỹ; tạo Proxy nếu thiếu. Có badge "phục hồi" và cho Transcribe lại. Chạy nền, không hỏi người dùng, không chặn Home.
- **FR-25 Chặn thoát.** Đóng app khi đang Live → hỏi xác nhận; nếu tiếp tục, app giữ tiến trình đủ lâu để finalize và lưu Phiên.

## CAP-7 Lưu Phiên live & Transcribe lại

- **FR-26.** Phiên live dùng cùng schema với Phiên file; tên mặc định là nhãn thời gian cục bộ. **[override, UX]** Dừng → overlay "Đang lưu phiên…" → điều hướng sang Transcript detail. Transcribe lại là một Job (FR-12, FR-15); kết quả hiện cạnh bản live, giữ Memo/Tag/Ghi chú. Bản live không bị xoá hay thay bởi một kết quả Partial.

## CAP-8 Home

- **FR-27 Danh sách.** Mỗi dòng gồm: tên, ngày (múi giờ cục bộ), loại, Model, số Segment, thời lượng, badge memo / có audio / partial / phục hồi. Job đang chạy nằm ở đầu, có tiến độ và lối quay lại. 500 Phiên hiển thị ≤ 1 s `[ASSUMPTION]`. Không xoá được Phiên đang có Job.
- **FR-28 Tìm theo tên.** Không phân biệt hoa thường, bỏ qua dấu cách thừa; cập nhật khi gõ, ≤ 200 ms với 500 Phiên. Query AND với bộ lọc Tag; xoá query vẫn giữ bộ lọc Tag. Không có kết quả → trạng thái rỗng có nút xoá bộ lọc.
- **FR-29 Tag.** ≤ 20 Tag/Phiên, mỗi Tag ≤ 80 ký tự; trim, bỏ trùng không phân biệt hoa thường. Lọc nhiều Tag theo AND; có lọc "Chưa gắn tag"; quick-picker. Tag giữ nguyên qua đổi tên, Transcribe lại, Chạy lại. Xoá một Tag khỏi mọi Phiên là một thao tác, có xác nhận.
- **FR-30 Đổi tên, xoá.** Đổi tên inline: không nhận tên rỗng, tối đa 200 ký tự, Esc huỷ, Enter lưu. Xoá (có xác nhận) → xoá Transcript, Proxy, Recording, Memo, Ghi chú, liên kết Tag khỏi Container; dung lượng giảm tương ứng, không còn file mồ côi. Tag không còn gắn Phiên nào vẫn tồn tại.
- **FR-31 Tải Recording.** Chỉ cho Phiên live, là mục trong menu ⋯ của dòng. Lưu qua dialog. **[override]** định dạng WAV/FLAC (không M4A).

## CAP-9 Transcript detail

- **FR-32 Trình phát.** Phát từ Proxy; click Segment → seek tới `start`; highlight Segment đang phát. Seek trong file 90 phút ≤ 500 ms `[ASSUMPTION: phụ thuộc S8/Q3]`.
- **FR-33 Tìm trong Transcript.** Không phân biệt hoa thường; bộ đếm "n/N"; prev/next vòng tròn; Enter = next; highlight và cuộn tới match. ~700 Segment → ≤ 100 ms.
- **FR-34 Hiển thị.** Timestamp `HH:MM:SS` (bỏ phần giờ nếu < 1 h), đã cộng offset. Phiên có hai Transcript → xem cạnh nhau hoặc chọn một. Click ở cột nào cũng seek được. Export/copy lấy Transcript đang chọn. **[override]** không hiện speaker.
- **FR-35 Export.** `.txt`, `.srt`, `.json` qua dialog lưu; copy toàn bộ vào clipboard. Áp offset. `.json` giữ speaker. Phiên Partial → `.txt` kèm ghi chú Khoảng thiếu `[ASSUMPTION]`.

## CAP-10 Ghi chú & Memo

- **FR-36 Ghi chú.** Tự lưu (debounce ~800 ms) theo Phiên, có ở cả Live (mở mặc định) và Transcript detail. Force-quit trong Live không mất Ghi chú.
- **FR-37 Template.** CRUD và khôi phục mặc định, quản lý ở Cài đặt → Memo. Mỗi Template có tên và prompt; thiếu `{transcript}` thì không lưu được, kèm giải thích. Khôi phục mặc định không xoá Template người dùng tạo. Bộ mặc định theo ngôn ngữ UI (nội dung: Q8).
- **FR-38 Memo.** Sinh theo Template, cache theo Phiên + Template, sinh lại được. Markdown được sanitize; link mở trình duyệt ngoài. Copy và tải `.md`. Template có `{notes}` → nhúng Ghi chú với header theo ngôn ngữ UI. Lỗi Memo không ảnh hưởng Transcript và được phân loại (quota/auth/mạng). Chạy lại Transcript → Memo cũ vẫn hiện, gắn nhãn "Memo sinh từ bản trước" `[ASSUMPTION]`. Nút Sinh memo / Transcribe lại có badge/tooltip báo sẽ tốn token.

## CAP-11 Settings, lưu trữ, chẩn đoán, About

- **FR-39 Settings.** Các nhóm: Chung (ngôn ngữ UI, theme), Gemini (key, model, ngôn ngữ transcribe), Chunking (phút, offset), Live (Target mặc định), Memo (Template), Lưu trữ, Cấu hình đề xuất, About/Privacy. Mọi trường có tooltip; giá trị sai bị chặn tại chỗ. Ô key ẩn ký tự, có nút hiện/ẩn và "Kiểm tra key". Không có nhóm engine/Whisper/Copilot/wizard/Updates/thư mục cache.
- **FR-40 Lưu trữ** *(mới)*. Hiện dung lượng (media, DB), nút mở thư mục (nếu OS cho phép), "Xoá toàn bộ dữ liệu" có xác nhận hai bước. Không tự dọn Phiên cũ `[ASSUMPTION]`.
- **FR-41 Chẩn đoán.** Log xoay vòng, xuất gói theo allow-list, xoá được. Không bao giờ chứa Transcript, Bản dịch, Ghi chú, Memo hay key; redaction áp cho `AIza…`, `AQ.…`, URL, `authorization`. Test tự động grep log sau một Phiên mẫu. **[override]** Chỉ đếm cục bộ (Phiên, lỗi theo category, crash) và hiện trong Chẩn đoán; không gửi đi đâu, không có tuỳ chọn opt-in.
- **FR-42 Cấu hình đề xuất.** Tải bộ model/chunk/template đã ký từ endpoint của chúng ta. Chữ ký sai → bỏ qua. Diff hiện inline trong nhóm setting (dòng dạng "nhãn · hiện tại → đề xuất", chỉ những mục thực sự đổi), có Áp dụng/Huỷ. Không bao giờ liệt kê hay ghi đè key hoặc Consent. `[ASSUMPTION: có diff]`
- **FR-43 About.** Hiện version, tác giả, liên hệ hỗ trợ, link Privacy Policy, link mở lại màn Consent.

## CAP-12 Ad slot

- **FR-44 Hiển thị** *(mới)*. Vị trí ở đáy sidebar (sidebar rộng 260 px) trên Home, Transcript detail và Settings; không bao giờ ở route Live. Creative 300×100 hoặc 320×50, lọc theo locale UI và thời hạn, xoay theo trọng số, mỗi Creative tối đa một lần/10 phút. Click → mở trình duyệt ngoài. `ads.json` cache 24 h; offline và không có Creative hợp lệ → dùng Creative nhúng sẵn (giới thiệu trans-kun/Relipa). Không che nội dung, không âm thanh, không interstitial.
- **FR-45 Tuân thủ** *(mới)*. Có nhãn "Sponsored", nút "Báo cáo quảng cáo" (URL/mailto kèm `id` Creative) và "Vì sao tôi thấy quảng cáo này" (text tĩnh, dựa trên ngôn ngữ UI). Không identifier, không cookie, không đo lường cá nhân. **[override]** Impression/click chỉ đếm cục bộ, không gửi đi. Cờ server `ads_enabled` tắt toàn bộ; cờ cục bộ `is_premium` (chưa có UI) ẩn Ad slot.
- **FR-46 Toàn vẹn** *(mới)*. `ads.json` ký ed25519, public key nhúng trong binary; chữ ký sai → dùng cache/fallback. Ảnh ≤ 100 KB, cache trong Container. Không tải script hay HTML.

## CAP-13 Đa ngôn ngữ & theme

- **FR-47.** vi/en/ja dùng cùng một tập key; thiếu key → CI fail. Đổi ngôn ngữ áp dụng ngay, không cần khởi động lại. Template mặc định, nhãn thời gian, Consent và text Ad slot đi theo ngôn ngữ UI. Transcript/Bản dịch có thể trộn ja/vi/en.
- **FR-48 Theme** *(mới)*. Sáng/tối có trong bản submit đầu; mặc định theo hệ thống, ép được ở Cài đặt → Chung (Theo hệ thống/Sáng/Tối); đổi áp dụng tức thì. Mọi cặp màu mang nội dung đạt WCAG AA ở cả hai theme. Token: `DESIGN.md`.

## NFR xuyên suốt

- **NFR-1 Riêng tư.** Dữ liệu họp chỉ đi tới Google bằng key của người dùng. Không server trung gian. **[override]** Không có telemetry dưới bất kỳ hình thức nào.
- **NFR-2 Không chặn lõi.** Lỗi ở Memo, Proxy, Ad slot hay cấu hình đề xuất không làm hỏng Transcript/Recording. Mọi tác vụ dài có cancel, progress, và lỗi mang category ổn định.
- **NFR-3 Bền phiên.** Crash, force-quit, mất mạng hay server đóng kết nối không làm mất Recording và Segment đã tích luỹ.
- **NFR-4 Không phụ thuộc ngoài.** Không binary ngoài, không tải/chạy code, không cần quyền admin, không self-update; một tiến trình.
- **NFR-5 Hiệu năng** `[ASSUMPTION]`. Decode + cắt Chunk ≥ 20× realtime trên máy 4 nhân; mở app tới Home ≤ 2 s; độ trễ Live ≤ 2 s sau kết câu.
- **NFR-6 Mạng doanh nghiệp.** Dùng CA store của hệ thống (proxy/Zscaler). Lỗi TLS/CA có câu chữ riêng, gộp vào category Mạng/CA.
- **NFR-7 Sandbox.** Entitlement/capability tối thiểu (xem `store-compliance.md`). Không Screen Recording.
- **NFR-8 Chi phí.** Chỉ ba luồng tốn token; không có gọi nền định kỳ; Chunk gửi inline.
- **NFR-9 Lỗi dễ hiểu.** Category hiển thị: Quota, Key bị từ chối, Model, Mạng/CA, Định dạng, Quyền hệ thống, Lưu trữ, Nội dung bị chặn. Mỗi category có hành động gợi ý. Không hiện stack trace, key hay URL.
- **NFR-10 Kích thước** `[ASSUMPTION]`. Bundle ≤ 60 MB mỗi nền tảng; RAM khi Live ≤ 300 MB.
- **NFR-11 Tiếp cận** `[ASSUMPTION: mức tối thiểu]`. Điều hướng bàn phím cho thao tác chính (bắt đầu/dừng Live, tìm, export), phím tắt chỉ trong app. Tương phản text đạt WCAG AA ở cả hai theme. Không yêu cầu hỗ trợ đầy đủ screen reader.
- **NFR-12 An toàn đường dẫn.** Tên Phiên, Tag, tên Template, id Creative không bao giờ thành một phần đường dẫn; file đặt tên theo ID tự sinh.
