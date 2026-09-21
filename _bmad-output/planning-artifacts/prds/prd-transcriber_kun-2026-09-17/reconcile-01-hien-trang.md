# Đối chiếu doc 01 (hiện trạng & phạm vi) với PRD v3 + Addendum

Phạm vi đối chiếu: mọi mục nhãn **GIỮ**/**ĐỔI** trong `docs/rebuild-v3/01-hien-trang-va-pham-vi.md` (§2.2–2.5 bảng tính năng, §3 luồng transcribe file, §4 luồng Live, §6 NFR). Bỏ qua §2.1 (thay bằng Onboarding — đã có ánh xạ rõ), phần BỎ, và §2.6 Copilot trừ khi PRD vô tình vẫn đưa vào.

## Khoảng trống

1. **§2.4 "Dịch realtime" + §4 bước 3 (worker setup, nhánh "Nếu tắt dịch")** → INPUT mô tả rõ một nhánh cấu hình khi dịch bị tắt hẳn (bỏ `outputAudioTranscription`, `target = source` hoặc `ja` khi auto, không `echoTargetLanguage`) — tức v2 cho phép chạy Live **không dịch** (tiết kiệm token, giảm độ trễ). PRD FR-19 chỉ mô tả view toggle "Gốc/Dịch/Cả hai" và đổi Target giữa phiên, ngầm định dịch luôn bật ở tầng API. Không có FR nào cho phép người dùng bắt đầu phiên Live với dịch tắt hoàn toàn ở mức cấu hình. Việc này cũng liên quan NFR-8 (minh bạch chi phí token) vì dịch luôn bật = luôn tốn thêm output token dù người dùng chỉ cần transcript gốc. → Nên bổ sung vào FR-16 hoặc FR-19 (hoặc FR mới) một tuỳ chọn "Không dịch" khi bắt đầu Live. Mức độ: **Cao**.

2. **§2.2 "Danh sách phiên gần đây" (cột hiển thị "engine")** → FR-27 thay cột "engine" bằng "loại (file/live)", làm mất thông tin engine/model đã dùng để tạo Transcript — có ý nghĩa hơn từ v3 vì FR-14 có 2 họ model transcribe khác hành vi (segment JSON thường vs word-timestamp có speaker). → Cân nhắc bổ sung hiển thị model/họ model trong FR-27. Mức độ: **Thấp**.

3. **§2.5 Settings/Gemini "API key(s) ... ẩn/hiện"** → FR-3/FR-5 không nhắc chi tiết UI ẩn/hiện giá trị key khi nhập hoặc xem lại (chỉ nói lưu trong keychain). → Bổ sung một dòng hệ quả nhỏ vào FR-5 hoặc FR-39. Mức độ: **Thấp**.

4. **§3 bước 6 "prompt yêu cầu segment 5–15 s" (model thường)** → FR-14 chỉ nêu ngưỡng gộp segment cho họ model `*-transcribe` (~8s, tối đa 15s), không nêu lại ngưỡng độ dài segment mong muốn cho model JSON-schema thường. → Bổ sung câu ngắn vào hệ quả FR-14 cho rõ cả hai họ model đều nhắm segment 5–15s. Mức độ: **Thấp**.

5. **§3 bước 7 "500/502/503/504 retry; lỗi khác fail ngay"** → FR-6/FR-15 nêu chính sách theo quota (429) và auth (401/403), nhưng không nói rõ ở mức PRD hành vi khi gặp lỗi server 5xx (nên retry trong cùng 4 lần) so với các lỗi khác (fail ngay không đợi hết 4 lần) — ảnh hưởng trải nghiệm khi lỗi không phải quota. Addendum D có nêu nhóm lỗi (`Quota|Auth|Model|Request|Timeout|Network|Blocked|Shape`) nhưng không map rõ 5xx→retry. → Có thể để ở tầng addendum là đủ, nhưng nên có 1 câu hệ quả trong FR-15 xác nhận hành vi này để không bị hiểu nhầm là "mọi lỗi đều dùng hết 4 lần thử". Mức độ: **Thấp**.

## Mâu thuẫn

1. **Cache-hit khi transcribe trùng file** — INPUT §3 bước 2: "cache hit → trả ngay (trừ force-rerun)" nghĩa là hành vi mặc định là **tự động trả kết quả cache**, chỉ hỏi/ép chạy lại khi người dùng chủ động yêu cầu. PRD FR-11 lại quy định: "Transcribe cùng một file (theo hash nội dung) → app **hỏi** mở Phiên có sẵn hay chạy lại" — tức luôn hiện hộp thoại hỏi thay vì tự trả ngay theo mặc định. Đây là thay đổi hành vi mặc định không được liệt kê trong bảng quyết định Q1–Q11 của addendum, nên chưa rõ là chủ đích hay sơ suất khi viết PRD. Mức độ ảnh hưởng: **Trung bình–Cao**, cần chủ sản phẩm xác nhận.

2. **Nội bộ INPUT tự mâu thuẫn về key rotation khi 401/403** — §2.6 (Copilot, dòng "Key rotation") ghi "không xoay với 401/403/400/404/timeout", trong khi §3 bước 7 (luồng transcribe file chính) ghi "401/403 → loại key". PRD FR-6 đi theo §3 (401/403 → loại key khỏi vòng), tức PRD đã chọn đúng phiên bản không-Copilot. Không phải lỗi PRD, chỉ ghi nhận để tránh nhầm khi đọc lại doc 01. Mức độ: **Thấp**.

3. **Nội bộ INPUT tự mâu thuẫn về Copilot ở Transcript detail** — §2.3 dòng "Xem lại card Copilot" gắn nhãn **GIỮ** (đơn giản hoá: 1 feed), trong khi toàn bộ Copilot (§2.6) gắn nhãn **BỎ** theo quyết định 2026-09-14 (Q1 trong addendum). PRD/addendum tuân theo quyết định Q1 mới hơn (bỏ hoàn toàn, kể cả xem lại card cũ), điều này hợp lý nhưng là một điểm nhãn lỗi thời chưa cập nhật trong doc 01 gốc, không phải khoảng trống của PRD. Mức độ: **Thấp**.

## Đã phủ tốt

- Onboarding/Consent/API key thay thế Setup wizard — khớp chặt với ý đồ §2.1.
- Home: tìm kiếm, tag (giới hạn 20/80 ký tự, chuẩn hoá, AND-filter, quick-picker, xoá tag toàn cục), đổi tên, xoá phiên, job đang chạy (SPA một cửa sổ) — ánh xạ 1-1 vào FR-27–31.
- Transcript detail: player + click-to-seek + highlight, tìm kiếm, side-by-side, export 3 định dạng, memo (template/cache/regenerate/copy/download), notes tự lưu 800ms — ánh xạ vào FR-32–38.
- Live: 3 nguồn audio + đổi giữa phiên, macOS Core Audio tap, Windows WASAPI 2 endpoint, re-detect ngôn ngữ (generation guard), TTS + ducking 30%, reconnect bền (MAX_FAILURES=5, resend chunk chưa ack), recording bền (vá header ~5s), phục hồi phiên mồ côi — ánh xạ đầy đủ vào FR-17–26, khớp cả tham số số học.
- Tham số vận hành Gemini transcribe file (max attempts 4, cooldown 60s, max wait 180s) khớp chính xác giữa INPUT §3, PRD FR-6/FR-15 và addendum D.
- NFR §6 (riêng tư, không chặn lõi, bền phiên, đa ngôn ngữ, chi phí) ánh xạ đầy đủ sang NFR-1/2/3/8 và §4.10.
- Data model file-based → SQLite: addendum §G liệt kê schema tương ứng đầy đủ các trường trong các file `.json/.meta.json` cũ.
