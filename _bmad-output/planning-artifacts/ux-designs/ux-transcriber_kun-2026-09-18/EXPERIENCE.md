---
name: trans-kun v3
status: final
updated: 2026-09-18
sources:
  - _bmad-output/planning-artifacts/prds/prd-transcriber_kun-2026-09-17/prd.md
  - _bmad-output/planning-artifacts/prds/prd-transcriber_kun-2026-09-17/addendum.md
  - design-system/trans-kun/MASTER.md
  - design-system/trans-kun/mockup/
  - docs/rebuild-v3/06-thiet-ke-giao-dien-v3.md
---

# trans-kun v3 — Experience Spine

> Tài liệu này sở hữu **"hành xử như thế nào"**: kiến trúc thông tin, trạng thái, tương tác, khả năng tiếp cận, hành trình. Đặc tả thị giác nằm ở `DESIGN.md`; token được tham chiếu theo tên dạng `{colors.accent}`. Khi mâu thuẫn với mockup hay bất kỳ bản import nào, **spine thắng**.

## Foundation

**SPA một cửa sổ desktop.** Tauri 2 (lõi Rust, một tiến trình) + Svelte 5 + TypeScript trong WebView hệ thống (WKWebView trên macOS, WebView2 Evergreen trên Windows). macOS ≥ 14.4 universal, phân phối Mac App Store; Windows 10 1809+ / 11 x64 + Arm64, phân phối Microsoft Store dạng MSIX.

**Không kế thừa UI system bên thứ ba** — không shadcn, không MUI, không component library. Design system tự dựng; `DESIGN.md` là nguồn sự thật cho mọi token và đặc tả thị giác. Hệ quả cho spine này: mỗi component pattern bên dưới phải tự nêu hành vi của nó, không có "kế thừa mặc định của thư viện" để dựa vào.

**Ba ràng buộc nền quyết định phần lớn hành vi:**

1. **BYOK.** Người dùng mang Gemini API key của chính họ. App không có tài khoản, không backend lưu dữ liệu họp, không đồng bộ cloud. Mọi thứ tốn tiền đều tốn tiền *của người dùng* — nên giao diện phải nói trước.
2. **Sandbox + store review.** App Sandbox (macOS) và package identity (MSIX) với entitlement tối thiểu: network client, micro, system audio, file do người dùng chọn. **Không xin quyền Screen Recording.** Không tải/chạy code lúc chạy, không self-update, không nhắc kênh tải ngoài store.
3. **Không mất dữ liệu họp.** Recording và Segment đã tích luỹ phải sống qua crash, force-quit, mất mạng, server đóng kết nối. Đây là lời hứa mạnh nhất của sản phẩm (SM-4) và nó định hình toàn bộ mục *Độ bền phiên & trạng thái kết nối*.

Rời màn **không bao giờ** làm mất job hay phiên đang chạy.

## Information Architecture

```
Onboarding (3 bước — chỉ lần đầu, hoặc khi phiên bản văn bản Consent tăng)
└─ App shell  (sidebar 260px: nav · card job · ad slot)
   ├─ /home            ── /session/:id
   ├─ /live            ── LiveSetup → Live (đang ghi) → (Dừng) → /session/:id
   └─ /settings/:group
```

| Bề mặt | Route | Đến từ | Mục đích | FR |
|---|---|---|---|---|
| Onboarding — Ngôn ngữ | — | Mở app lần đầu | Chọn vi/en/ja, mặc định theo hệ thống | FR-1 |
| Onboarding — Consent | — | Bước 1 | Đồng ý gửi audio/transcript tới Google bằng key của mình | FR-2 |
| Onboarding — API key | — | Bước 2 | Nhập + kiểm tra key, hoặc "Bỏ qua, nhập sau" | FR-3, FR-5 |
| **Trang chủ** | `/home` | Sidebar · mở app · `g h` | **Chính là thư viện**: drop-zone, nút Live, danh sách phiên, tìm, lọc tag, card job | FR-4, FR-8–9, FR-11–12, FR-27–31 |
| Tag picker (popover) | — | Chip "+ N tag khác", "+ Tag", ô Tag ở LiveSetup | Tìm/tạo/chọn tag — **một popover dùng chung cho cả ba nơi** | FR-29 |
| **Transcript detail** | `/session/:id` | Dòng phiên · card job · sau khi Dừng Live | Trình phát, transcript, tìm, export, Memo, Ghi chú | FR-10, FR-13, FR-15, FR-26, FR-32–38 |
| **Live — bắt đầu** | `/live` | Nút Live · `⌘⇧L` | Chọn nguồn audio, tag, ngôn ngữ, target; kiểm tra quyền | FR-16–17, FR-19 |
| **Live — đang ghi** | `/live` | "Bắt đầu ghi" | Transcript + bản dịch realtime, đổi nguồn/target, TTS, ghi chú | FR-18–25, FR-36 |
| **Cài đặt** | `/settings/:group` | Sidebar | 9 nhóm: Chung · Gemini · Chunking · Live · **Memo** · Lưu trữ · Chẩn đoán · Cấu hình đề xuất · Giới thiệu & Quyền riêng tư. Nhóm **Memo** ("Memo — mẫu prompt") là **nơi duy nhất** thêm/sửa/xoá Template memo | FR-5–7, FR-16, FR-39–43 |

**Quyết định IA đã chốt (2026-09-18):**

- **Trang chủ chính là thư viện.** Không tách mục "Thư viện" riêng. Khi đã có phiên, vùng nhập liệu co lại thành **drop-zone mỏng** nằm trên danh sách; khi chưa có phiên, nó nở ra thành hai card ngang hàng.
- **Sau khi Dừng Live → điều hướng thẳng sang `/session/:id`** của phiên vừa tạo, qua một overlay "Đang lưu phiên…". Không ở lại màn Live read-only. Lý do: bản dịch **không được lưu** (FR-19), nên ở lại sẽ để người dùng nhìn một cột Dịch rỗng dần; còn detail có ngay Export, Transcribe lại và Memo — đúng cao trào của UJ-2.
- **Ad slot sống trong sidebar**, nên nó tự biến mất ở Live cùng sidebar ad. Cấu trúc thực thi quy tắc thay vì trông chờ lập trình viên nhớ ẩn nó.

Deep link nội bộ dùng cho card job và badge "1 job đang chạy". Modal chỉ chồng **một cấp**.

→ Tham chiếu bố cục: [`design-system/trans-kun/mockup/project/`](../../../../design-system/trans-kun/mockup/project/) (12 artboard, bảng đối chiếu ở `DESIGN.md.Components`). **Spine thắng khi mâu thuẫn.**

## Voice and Tone

Microcopy. Tư thế thương hiệu và giọng thẩm mỹ nằm ở `DESIGN.md.Brand & Style`.

Người đọc là **BrSE và sales đang trong hoặc vừa xong một buổi họp với khách Nhật**. Họ đang vội, đang chia sự chú ý, và đôi khi đang hoảng vì tưởng mất dữ liệu. Giọng vì thế: **bình tĩnh, cụ thể, nói thẳng chuyện gì đang xảy ra và làm gì tiếp theo.** Không trấn an rỗng, không vui vẻ giả tạo.

| Do | Don't |
|---|---|
| "Đang ghi âm · Đang nối lại (2:10)" | "Đang xử lý…" |
| "Mất kết nối 45:02–48:10" | "Có lỗi xảy ra" |
| "Key không được chấp nhận. Kiểm tra lại key trong Cài đặt → Gemini." | "Authentication failed (401)" |
| "Thiếu 3 khoảng · 12 phút" + nút "Chạy lại phần thiếu" | "Transcript có thể chưa đầy đủ" |
| "Vẫn đang ghi âm — transcript sẽ tự chạy tiếp khi có mạng." | "Mất kết nối!" |
| "Định dạng không hỗ trợ. Hãy chuyển sang mp4, m4a hoặc mp3." | "Invalid file format" |
| "Tốn token Gemini" (badge, nói trước) | Im lặng rồi tính tiền người dùng |
| "Đã lưu 14:32" | "Đã lưu thành công! ✓" |
| "Chưa có phiên nào. Kéo file vào đây, hoặc bắt đầu Live." | "Không có dữ liệu để hiển thị" |

**Ba quy tắc cứng:**

1. **Không bao giờ để lộ** stack trace, API key, URL endpoint, hay nội dung transcript trong bất kỳ thông báo nào (NFR-9, FR-41).
2. **Mọi lỗi có một hành động.** Nếu không nghĩ ra hành động nào cho người dùng, đó là dấu hiệu category lỗi bị đặt sai.
3. **Số liệu cụ thể thắng tính từ.** "32 / 90 phút · 36 %" thay vì "đang xử lý"; "2:10" thay vì "một lát".

## Component Patterns

Hành vi. Đặc tả thị giác nằm ở `DESIGN.md.Components`.

| Component | Dùng ở | Quy tắc hành vi |
|---|---|---|
| **Dòng phiên** | Trang chủ | Click mở `/session/:id`. Đổi tên **inline** (Enter lưu, Esc huỷ, tên rỗng bị từ chối, ≤ 200 ký tự). Menu ⋯: Đổi tên · Gắn tag · Tải recording · Xoá. Badge "Thiếu N khoảng" và "Phục hồi" đứng **ngay sau tên** vì chúng đòi hành động; badge Memo/Audio chỉ hiện ở detail. Virtual list — 500 dòng render ≤ 1 s. |
| **Card job** | Trang chủ (đầy đủ) + sidebar (thu gọn) | Tiến độ **theo phút audio thực**, không theo số chunk: `32 / 90 phút · 36 %`. Dòng chi tiết: chunk hiện tại, key đang dùng, số lần thử lại. Nút "Mở" và "Huỷ". Huỷ dừng gửi chunk mới trong ≤ 2 s, dọn dữ liệu tạm, **không lưu phiên dở**. Nhìn thấy từ mọi màn. |
| **Drop-zone** | Trang chủ, Transcript detail | Kéo file vào **bất kỳ đâu trên màn** cũng nhận, không chỉ vùng drop-zone. Nhiều file → xếp hàng **tuần tự**, mỗi file một phiên, số file chờ hiện trên card job. File trùng nội dung (theo hash) → **mở phiên có sẵn**, không gọi Gemini. |
| **Tag picker** (popover) | Trang chủ · Transcript detail · LiveSetup | Một component dùng chung ba nơi. Ô tìm kiếm **kiêm tạo mới**; nhóm "Đang lọc" lên đầu; danh sách toàn bộ tag sắp theo số phiên; link "Quản lý tag". Chuẩn hoá: trim, bỏ trùng không phân biệt hoa thường, ≤ 20 tag/phiên, ≤ 80 ký tự/tag. |
| **Hàng chip tag** | Trang chủ | Một hàng, **không wrap**: tag đang lọc (có x) → 3–5 tag dùng nhiều nhất → chip "+ N tag khác" mở popover. Chip "Chưa gắn tag" tách bằng vạch dọc. Nhiều tag = **AND**. |
| **Ô tìm** | Trang chủ (theo tên) · Transcript detail (trong transcript) | Cập nhật khi gõ (≤ 200 ms với 500 phiên; ≤ 100 ms với ~700 segment). Không phân biệt hoa thường, bỏ khoảng trắng thừa. Trong transcript: đếm `n/N`, prev/next **vòng tròn**, Enter = next, Shift+Enter = prev, highlight `{colors.mark}` + cuộn tới match. Query + lọc tag là AND; xoá query **giữ nguyên** lọc tag. |
| **Trình phát** | Transcript detail | Dùng **Proxy phát lại** trong Container, không phụ thuộc file nguồn. Click segment → seek tới `start`. Segment đang phát nền `{colors.accent-soft}`. Thanh seek `role=slider`, phím ←/→ nhảy 5 s. Proxy hỏng → "Không có audio · Chọn lại file nguồn" thay vì im lặng. |
| **Danh sách segment** | Transcript detail, Live | Timestamp `HH:MM:SS` (bỏ giờ nếu < 1 h), đã cộng offset toàn cục. **Speaker ẩn ở v3** (giữ trường trong data model). Khoảng thiếu và mất kết nối là **dòng nằm trong luồng**, đúng vị trí thời gian — không phải banner tách rời. |
| **Chỉ báo Live** | Live | **Hai pill riêng, không bao giờ gộp**: (a) "Đang ghi" + đồng hồ + dot pulse; (b) kết nối, ba trạng thái — Đang transcribe / Đang nối lại (+ thời gian đã chờ) / Đã dừng transcript. |
| **Tự cuộn** | Live, Transcript detail | Cuộn theo dòng mới nhất; **dừng tự cuộn ngay khi người dùng cuộn lên**, hiện nút "Xuống dòng mới nhất" để quay lại. |
| **Panel Memo** | Transcript detail | Select template + nút "Sinh / Sinh lại" kèm badge "Tốn token Gemini" và dòng ghi rõ "Sinh từ bản X + ghi chú · giờ". Markdown **sanitize** trước khi render; link mở trình duyệt ngoài. Lỗi memo hiện **inline trong panel**, không đụng transcript. Memo cũ sau khi transcript chạy lại giữ nguyên, kèm nhãn "Memo sinh từ bản trước". |
| **Panel Ghi chú** | Transcript detail (tab) · Live (mở mặc định) | Textarea tự lưu debounce ~800 ms, chỉ báo "Đã lưu hh:mm". Phải lưu **trước khi phiên finalize** để không mất khi force-quit. |
| **Editor template memo** | Cài đặt → Memo | Master-detail (thay dialog của v2). Hàng kiểm tra tại chỗ `{transcript}` (bắt buộc) và `{notes}` (tuỳ chọn) cập nhật khi gõ; thiếu `{transcript}` → nút Lưu vô hiệu + dòng kiểm tra chuyển đỏ. "Khôi phục mẫu mặc định" **chỉ ghi lại mẫu mặc định**, không xoá mẫu của người dùng. |
| **Đồng bộ cấu hình đề xuất** | Cài đặt → Cấu hình đề xuất | Nút "Tải cấu hình đề xuất" → tải bộ cấu hình đã ký, xác minh chữ ký, rồi **hiện diff cho người dùng xem trước khi áp dụng** (FR-42). Diff render **inline ngay trong nhóm setting này**, không phải route mới: mỗi dòng là một thiết lập sẽ đổi, dạng `nhãn · giá trị hiện tại → giá trị đề xuất`; thiết lập không đổi thì không liệt kê. Hai nút: **Áp dụng** (primary) và **Huỷ**. **Không bao giờ liệt kê hay ghi đè key.** Áp dụng xong → toast "Đã áp dụng N thiết lập". Chữ ký sai hoặc mạng lỗi → banner category tại chỗ, **không đổi gì cả**. `[ASSUMPTION: preview diff — giữ nguyên theo PRD §4.8 FR-42 và chỉ mục giả định §14.]` |
| **Tải recording** | Trang chủ — dòng phiên live | Chỉ là **một mục trong menu ⋯** của dòng phiên (không phải màn riêng). Mở dialog lưu hệ thống. Chỉ hiện với phiên `live` có Recording; phiên `file` không có mục này. Định dạng theo nền tảng (WAV/FLAC, +M4A nếu có encoder). |
| **Lưu trữ & Chẩn đoán** | Cài đặt → Lưu trữ · Chẩn đoán | **Chỉ hiển thị thông tin + tải log** — không có luồng nhiều bước. Lưu trữ: thanh dung lượng Media/DB, số phiên, "Mở thư mục", "Xoá toàn bộ dữ liệu…" (danger-soft, **xác nhận hai bước** — ngoại lệ duy nhất cần dialog ở hai nhóm này). Chẩn đoán: "Xuất gói nhật ký" (allow-list, mở dialog lưu), "Xoá nhật ký", checkbox "Gửi thống kê ẩn danh" mặc định tắt. |
| **Trường Settings** | Cài đặt | Bảng 2 cột (`220px` nhãn · control). **Mọi trường có icon (?) tooltip** và helper text bền. Giá trị không hợp lệ **bị chặn tại chỗ**, không đợi tới lúc Lưu. |
| **Banner** | Trang chủ · LiveSetup · Live · Transcript detail | Khuôn hiển thị lỗi/cảnh báo **inline, gần nơi xảy ra**. Luôn ba phần: tiêu đề = tên category · một câu nguyên nhân + cách sửa · một nút hành động. Không tự tắt — người dùng giải quyết hoặc rời màn. Nhiều banner cùng lúc trên một màn → gộp theo mức nghiêm trọng, **danger trên warning trên info**, tối đa hai banner hiển thị cùng lúc. |
| **Toast** | Toàn cục | Chỉ cho **việc nền đã xong** (memo xong, export xong). Tự tắt 4 s, `aria-live=polite`. **Không dùng toast cho lỗi cần hành động** — lỗi đi inline, gần nơi xảy ra. |
| **Dialog** | 5 trường hợp | Xoá phiên · Xoá toàn bộ dữ liệu (2 bước) · Đóng app khi đang ghi · Đóng app khi có job · Xoá tag toàn cục. Không dùng cho gì khác. |

## State Patterns

| Trạng thái | Bề mặt | Cách xử lý |
|---|---|---|
| **Chưa có key** | Mọi màn | Banner warning ở Trang chủ: "Chưa có API key hợp lệ — các tính năng cần Gemini đang tắt" + link "Nhập key". Nút cần Gemini **vô hiệu + tooltip + lối tắt**, không ẩn. Danh sách phiên và phát lại **vẫn dùng được**. Không màn trắng, không dialog lặp. (FR-4) |
| **Từ chối Consent** | Toàn app | App vào chế độ chỉ xem Cài đặt/About, có banner quay lại đồng ý. Không request nào tới Google, **kể cả kiểm tra key**. (FR-2) |
| **Trang chủ trống** | `/home` | Hai card ngang hàng: "Kéo file vào đây / Chọn file" và "Bắt đầu Live", kèm danh sách định dạng hỗ trợ. (UJ-1, UJ-5) |
| **Tìm/lọc không ra kết quả** | Trang chủ | Trạng thái rỗng + nút "Xoá bộ lọc". Footer luôn hiện `6 / 38 phiên · lọc…`. |
| **Job đang chạy** | Trang chủ + sidebar | Card job ở đầu danh sách + bản thu gọn ở sidebar. Rời màn rồi quay lại vẫn thấy. Xoá phiên đang có job → **bị chặn** cho tới khi job xong/huỷ. |
| **Transcript partial** | Trang chủ + Transcript detail | Badge "Thiếu N khoảng" ở dòng phiên; banner warning ở detail liệt kê từng khoảng + nút "Chạy lại phần thiếu" / "Chạy lại toàn bộ"; dòng warning trong luồng segment kèm "Chạy lại khoảng này". **Không bao giờ ẩn, không bao giờ lưu như hoàn chỉnh.** (FR-15) |
| **Phiên phục hồi** | Trang chủ | Badge "Phục hồi" + gợi ý Transcribe lại. Quét và đưa vào thư viện **chạy nền, không hỏi, không chặn Trang chủ**. (FR-24) |
| **Proxy phát lại hỏng** | Transcript detail | Transcript vẫn mở được; thanh phát hiện "Không có audio · Chọn lại file nguồn" để tạo lại proxy. (FR-10) |
| **Thiếu quyền hệ thống** | LiveSetup | Banner category "Thiếu quyền hệ thống" + nút mở System Settings. Nút "Bắt đầu ghi" vô hiệu cho tới khi có quyền **hoặc người dùng đổi sang nguồn khác**. Không crash. (FR-17) |
| **Model bị Google từ chối** | Live, job, Cài đặt | Cảnh báo **tại chỗ** (không chỉ trong log) + lối tắt tới Cài đặt → Gemini. App **không tự đổi model** thay người dùng. (FR-7) |
| **Hết key khả dụng (401/403)** | Mọi luồng Gemini | Lỗi category "Key bị từ chối" + lối tắt Cài đặt. Key bị loại khỏi vòng cho tới khi người dùng sửa. (FR-6) |
| **Mọi key đang nghỉ (429)** | Job | Job **chờ** tối đa 180 s/chunk thay vì fail ngay; card job hiện "đang chờ quota". (FR-6) |
| **Đang đọc bản dịch (TTS)** | Live | Pill "Đang đọc" nhỏ ở đầu cột Dịch, bật/tắt theo lúc TTS thực sự phát. (FR-21) |
| **Không dịch** | Live | View chỉ còn cột Gốc; toggle TTS **vô hiệu**; không gửi yêu cầu sinh bản dịch hay audio → không tốn token cho dịch. (FR-19) |
| **Cold load** | Trang chủ | App mở tới Trang chủ ≤ 2 s (NFR-5). Trong lúc chờ: khung sidebar + header render ngay, vùng danh sách hiện skeleton 6 dòng khớp bố cục thật. `[ASSUMPTION: nguồn không nêu cách xử lý cold load; skeleton là đề xuất, cần duyệt.]` |
| **Đang tải danh sách model** | Cài đặt → Gemini | Nút "Tải danh sách" chuyển sang trạng thái đang chạy, select vẫn dùng được với giá trị hiện tại. Lỗi → banner category tại chỗ, **không** xoá giá trị đang cấu hình. `[ASSUMPTION]` |
| **Live vừa bắt đầu, chưa có lời nào** | Live | Hai cột rỗng với một dòng help mờ ("Đang nghe…"); pill "Đang ghi" và pill kết nối đã hoạt động. Không hiện spinner — im lặng là trạng thái hợp lệ ở đây. `[ASSUMPTION]` |
| **Chưa có tag nào** | Tag picker | Ô tìm kiếm kiêm tạo mới + dòng gợi ý "Gõ để tạo tag đầu tiên". Không hiện danh sách rỗng trống trơn. `[ASSUMPTION]` |
| **Ad slot không có creative** | Trang chủ, detail, Cài đặt | Offline hoặc chữ ký `ads.json` sai → creative nhúng sẵn (giới thiệu trans-kun/Relipa). Slot **không bao giờ để trống hay co lại** làm layout nhảy. |
| **Đang lưu phiên** | Live → detail | Overlay nhỏ "Đang lưu phiên… finalize recording, tạo proxy" rồi điều hướng sang `/session/:id`. |

## Taxonomy lỗi

*(Mục riêng của sản phẩm — thực thi NFR-9.)*

Mọi lỗi người dùng nhìn thấy phải thuộc **đúng một trong tám category** dưới đây, và mỗi lỗi hiển thị theo cùng một khuôn: **tiêu đề = tên category · một câu nêu nguyên nhân + cách sửa · một nút hành động.**

| Category | Khi nào | Hành động đi kèm |
|---|---|---|
| **Quota** | Gemini trả 429 | "Thử lại" hoặc chờ tự động; gợi ý thêm key thứ hai |
| **Key bị từ chối** | 401/403 | → Cài đặt → Gemini |
| **Model** | 400/404 do tên model, model ngừng phục vụ | → Cài đặt → Gemini, chọn model khác |
| **Mạng / CA** | Timeout, TLS, proxy doanh nghiệp (Zscaler) | Hướng dẫn kiểm tra proxy/CA; app dùng CA store hệ thống |
| **Định dạng** | File `avi/wmv/flv/ts`, track không giải mã được | Nêu **định dạng nên chuyển sang** (mp4/m4a/mp3) |
| **Quyền hệ thống** | Thiếu System Audio Recording hoặc Micro | Nút mở System Settings đúng trang |
| **Lưu trữ** | Container đầy, ghi file thất bại, keychain không truy cập được | → Cài đặt → Lưu trữ |
| **Nội dung bị chặn** | Gemini từ chối nội dung | Nêu chunk/khoảng bị ảnh hưởng; không thử lại |

**Quy tắc chung:** lỗi hiện **inline, gần nơi xảy ra** — không phải toast. Category phải **ổn định** để lớp i18n dịch được, và phải bao trùm cả lỗi **ngoài** Gemini. Lỗi ở một luồng **không được làm hỏng luồng khác**: memo lỗi không đụng transcript, ad slot lỗi không đụng gì cả, proxy lỗi không chặn transcript (NFR-2).

## Độ bền phiên & trạng thái kết nối

*(Mục riêng của sản phẩm — thực thi FR-22–25, NFR-3, SM-4. Đây là lời hứa trung tâm; nếu phải hy sinh thứ gì, không hy sinh mục này.)*

**Nguyên tắc nền: Recording và transcript là hai thứ độc lập.** Recording ghi liên tục vào Container, header vá ~5 s để file luôn phát được. **Mạng chết, key hết, model lỗi đều không dừng Recording** — chỉ người dùng bấm Dừng hoặc lỗi thiết bị audio mới dừng. Giao diện phải phản ánh đúng sự tách biệt này, và đó là lý do hai pill trạng thái không bao giờ được gộp.

**Kết nối lại là việc của app, không phải của người dùng.**

- Mất kết nối vì bất kỳ lý do gì → tự nối lại, backoff 1 s → 2 s → 4 s → … tối đa 30 s, có jitter, **không giới hạn số lần** khi phiên còn chạy.
- Người dùng chỉ thấy **một chỉ báo nhỏ**, không thấy lỗi. Phiên 60 phút với ≥ 6 lần reconnect phải **không mất segment nào**.
- Audio thu trong lúc mất kết nối được giữ **tối đa 60 s** để gửi lại. Vượt quá → transcript live ghi một dòng "Mất kết nối mm:ss–mm:ss" **ngay trong luồng segment**. Toàn bộ audio đó vẫn nằm trong Recording, nên Transcribe lại lấp được sau.
- **Ngoại lệ duy nhất:** server từ chối *setup* 5 lần liên tiếp (payload sai, key bị từ chối, model không tồn tại) — đây là lỗi không tự hết. App dừng thử, hiện banner và cho chọn **"Tiếp tục chỉ ghi âm"** hay **"Dừng"**. Phiên vẫn lưu được và Transcribe lại sau.

**Sống sót qua crash.** Force-quit → Recording phát được tới mốc ≤ 5 s trước khi chết; Ghi chú đã lưu; lần mở app sau, phiên mồ côi **tự vào thư viện** với badge "Phục hồi", chạy nền, không hỏi. Volume hệ thống bị ducking dở cũng được phục hồi ở lần mở sau.

**Chặn thoát khi đang ghi.** Đóng app/cửa sổ lúc phiên live đang chạy → dialog "Phiên đang ghi — dừng và lưu trước khi thoát?". Nếu tiếp tục, app giữ tiến trình đủ lâu để finalize Recording và lưu phiên. Đóng app khi có **job** chạy → xác nhận tương tự, nhưng job bị **huỷ sạch** (không chạy nền).

**Đổi thứ gì cũng không ngắt phiên.** Đổi nguồn audio (≤ 1 s), đổi target dịch, bật/tắt TTS, "Nhận diện lại ngôn ngữ" — tất cả giữ nguyên phiên, Recording và đồng hồ. Timestamp **không bao giờ nhảy**; segment không bao giờ lùi thời gian so với segment trước.

## Minh bạch chi phí & quyền riêng tư

*(Mục riêng của sản phẩm — thực thi FR-2, §6.1, §6.3, NFR-1, NFR-8.)*

**Chi phí là tiền của người dùng, nên nói trước — không nói sau.** Đúng **ba luồng** tốn token: transcribe file, live, sinh memo. Mọi nút kích hoạt một trong ba luồng đó mang badge **"Tốn token Gemini"**: Sinh memo, Sinh lại, Transcribe lại, Chạy lại (toàn bộ / phần thiếu). Mở lại một phiên có sẵn (trùng hash file) **không gọi Gemini** và giao diện nói rõ điều đó.

Không có gọi nền định kỳ. Không upload trung gian.

**Consent đi trước mọi byte.** Không request nào tới Google trước khi Consent được ghi nhận — kể cả thao tác "Kiểm tra key". Màn Consent phải: vẽ sơ đồ **"Máy của bạn → (bằng key của bạn) → Google Gemini"** nhấn mạnh không có server trung gian nào của chúng ta; nêu đích danh bên nhận dữ liệu (Google); có link Privacy Policy mở **trình duyệt ngoài**; hiển thị "Văn bản đồng ý phiên bản N". Consent lưu bền kèm số phiên bản; **chỉ hỏi lại khi số phiên bản tăng**.

**Privacy mặc định.** Không tài khoản, không telemetry mặc định, không tracking, không identifier. Thống kê tổng hợp ẩn danh **chỉ khi người dùng bật opt-in** (mặc định tắt, có giải thích ngay cạnh checkbox). Nhật ký chẩn đoán **content-free**: không bao giờ chứa transcript, bản dịch, ghi chú, memo hay key; mọi chuỗi lỗi đi qua bộ lọc redaction. Xuất nhật ký chỉ lấy file trong allow-list.

Key lưu trong **kho khoá hệ điều hành** (Keychain / Credential Manager), không nằm trong file cấu hình. Xoá key trong UI xoá khỏi kho khoá thật.

## Ad slot & tuân thủ store

*(Mục riêng của sản phẩm — thực thi FR-44–46, §6.2, SM-5, SM-C1.)*

Ad slot là **house ads tự vận hành**, không phải mạng quảng cáo bên thứ ba. Nó tồn tại để tạo đường doanh thu/cross-promotion ban đầu **mà không làm hỏng trải nghiệm B2B** — và counter-metric SM-C1 nói rõ: không được tăng impression bằng cách thêm vị trí, tăng tần suất hay che nội dung.

**Vị trí:** đáy sidebar 260px, rộng 236px. Hiện ở Trang chủ, Transcript detail, Cài đặt.

**Tuyệt đối không hiện ở route Live** — kể cả sau khi đã bấm Dừng, khi người dùng vẫn còn ở route đó. Vì slot nằm trong sidebar và sidebar ad ẩn theo route, quy tắc này được cấu trúc layout thực thi.

**Mỗi creative phải kèm ba thứ** để qua review store (Apple 2.5.18, Microsoft 10.x): nhãn **"Sponsored"**, nút **"Báo cáo quảng cáo"** (mở URL/mailto có sẵn `id` creative), và link **"Vì sao tôi thấy quảng cáo này"** (text tĩnh theo ngôn ngữ UI — *không* theo dữ liệu cá nhân, vì không có dữ liệu cá nhân nào).

**Hành vi:** một creative tại một thời điểm, xoay theo trọng số, mỗi creative tối đa **một lần hiển thị / 10 phút**. Click mở **trình duyệt ngoài**. Cache 24 h; offline hoặc chữ ký `ads.json` sai → dùng **creative nhúng sẵn** (giới thiệu trans-kun/Relipa). Ảnh ≤ 100 KB, cache trong Container, **không tải script**. Đếm impression/click **chỉ tại chỗ**; chỉ gửi tổng hợp ẩn danh nếu người dùng đã bật opt-in.

Cờ `ads_enabled` từ server hoặc `is_premium` cục bộ → ẩn toàn bộ slot (chưa có UI premium ở v3).

**Không bao giờ:** che nội dung, có âm thanh, interstitial, tự động phát, hay xuất hiện trong lúc người dùng đang họp.

## Đa ngôn ngữ

*(Mục riêng của sản phẩm — thực thi FR-1, FR-47, §6.2.)*

UI có **ba ngôn ngữ ngang hàng: vi / en / ja**, cùng một tập key; thiếu key ở bất kỳ ngôn ngữ nào **bị chặn ở CI**. Đổi ngôn ngữ áp dụng **tức thì**, không cần khởi động lại — kể cả giữa luồng Onboarding.

Ngôn ngữ UI kéo theo: template memo mặc định, nhãn thời gian, nội dung văn bản Consent, text trong ad slot, và header nhúng ghi chú vào prompt memo.

**Hệ quả lên layout:** nút và badge dùng **`min-width` thay vì `width` cố định** — chuỗi tiếng Nhật và tiếng Việt dài hơn tiếng Anh và sẽ bị cắt. Timestamp và số **luôn mono tabular**, không đổi theo ngôn ngữ.

Lưu ý phân biệt: **ngôn ngữ UI** (vi/en/ja) khác **ngôn ngữ transcribe** (auto/ja/vi/en) khác **target dịch**. Transcript và bản dịch có thể trộn ja/vi/en trong cùng một phiên; giao diện không được giả định một phiên chỉ có một ngôn ngữ.

## Interaction Primitives

**Bàn phím phủ mọi thao tác chính** (NFR-11). Đây là app dùng trong lúc họp — người dùng thường đang gõ ghi chú và không muốn rời tay khỏi bàn phím.

- `⌘⇧L` / `Ctrl+⇧+L` — Bắt đầu / Dừng Live (toàn cục)
- `⌘F` / `Ctrl+F` — Tìm trong màn hiện tại
- `Enter` / `Shift+Enter` — Kết quả tìm kế tiếp / trước đó (vòng tròn)
- `Space` — Play/pause khi focus đang ở transcript
- `←` / `→` — Seek ±5 s khi focus ở thanh phát
- `Esc` — Đóng panel, popover, dialog; huỷ đổi tên inline
- `Enter` — Lưu đổi tên inline

**Chuột:** click dòng phiên để mở; click segment để seek; kéo thả file vào bất kỳ đâu trên màn.

**Cấm ở mọi nơi:**

- **Infinite scroll** — dùng virtual list cho danh sách dài, nhưng không phân trang lười.
- **Hover-only affordance** — mọi hành động phải tới được bằng bàn phím.
- **Modal chồng quá một cấp.**
- **Icon-only button không `aria-label`.**
- **Toast cho lỗi cần hành động** — lỗi đi inline.
- **Tự động phát audio** khi người dùng chưa yêu cầu.
- **Auto-scroll đè lên thao tác cuộn của người dùng** — người dùng cuộn lên là dừng tự cuộn, không thương lượng.

**`prefers-reduced-motion`:** tắt dot pulse, caret tĩnh, bỏ animation panel. Hai chuyển động chạy liên tục (pulse và caret) là hai thứ đầu tiên phải tắt.

## Accessibility Floor

Hành vi. Tương phản thị giác nằm ở `DESIGN.md`.

**Mức cam kết: WCAG 2.2 AA cho text**, trên cả light và dark, **đo riêng từng theme**. `[ASSUMPTION: NFR-11 đặt đây là mức tối thiểu và không yêu cầu hỗ trợ trình đọc màn hình đầy đủ; spine này giữ nguyên mức đó, nhưng các điểm bên dưới nên coi là sàn chứ không phải trần.]`

- **Focus ring nhìn thấy trên mọi control**; `Tab` order khớp thứ tự thị giác trên từng màn.
- **Trạng thái không bao giờ chỉ dựa vào màu** — luôn có icon + chữ đi kèm. Người dùng mù màu phải phân biệt được "đang transcribe" với "đang nối lại".
- **Kết quả kiểm tra key** render trong vùng `role=status` để trình đọc màn hình đọc được mà không cần người dùng đi tìm.
- **Toast** dùng `aria-live=polite` — thông báo việc nền không được cắt ngang.
- **Thanh seek** là `role=slider` với phím ←/→, không chỉ kéo chuột.
- **Toggle TTS** dùng `aria-pressed` phản ánh đúng trạng thái bật/tắt.
- **Nút vô hiệu** phải giải thích *vì sao* qua tooltip có thể tới được bằng bàn phím — không phải tooltip chỉ hiện khi hover.
- **Icon-only button** luôn có `aria-label` mô tả hành động, không mô tả icon.
- **Segment đang phát** được đánh dấu bằng cả nền accent-soft lẫn thuộc tính ngữ nghĩa, để không phụ thuộc thị giác.

## Responsive & Platform

Đây là **cửa sổ desktop resize được**, không phải trang web responsive. Không có breakpoint theo nghĩa web.

| Bề rộng cửa sổ | Hành vi |
|---|---|
| ≥ 1280px (mặc định) | Sidebar 260px + main + panel phụ (360px detail / 320px Live) cùng mở |
| 1024–1279px | Panel phụ đóng trước; sidebar **giữ nguyên** 260px; transcript nhận phần dư |
| < 1024px | Không hỗ trợ — cửa sổ tối thiểu 1024×680 |

**Khác biệt theo nền tảng:**

| Chủ đề | macOS | Windows |
|---|---|---|
| Quyền audio | **System Audio Recording** + Micro. **Không bao giờ** xin Screen Recording. | Thu được cả app họp native (endpoint communications) lẫn họp trong trình duyệt (endpoint console) |
| Hướng dẫn khi thiếu quyền | Link mở đúng trang System Settings | Link mở đúng trang Settings |
| WebView | WKWebView | WebView2 Evergreen — app kiểm tra runtime lúc chạy và **hướng dẫn cài nếu thiếu** |
| Tải recording | WAV, và M4A nếu có encoder | WAV/FLAC; không có encoder AAC → **chỉ đưa lựa chọn WAV/FLAC**, không lỗi im lặng |
| Phím tắt | `⌘` | `Ctrl` |

**Kiểm thử chấp nhận:** thu được Zoom, Teams và Google Meet trong Chrome trên **cả hai OS**.

`[ASSUMPTION: Windows Arm64 chỉ vào bản submit đầu nếu có máy test thật trước Phase 6 — Open Question 7 của PRD. Spine này không thay đổi theo kiến trúc CPU.]`

## Inspiration & Anti-patterns

**Kế thừa từ v2 (có chủ ý):** accent teal `#0F766E` và font IBM Plex — người dùng v2 phải nhận ra sản phẩm. Mặc định model (`gemini-flash-lite-latest` cho transcribe/memo). Bộ năng lực cốt lõi: transcribe file, live + dịch, memo, tag/tìm (SM-3).

**Bỏ hẳn so với v2 — mỗi thứ vì một lý do cụ thể:**

- **Setup wizard, banner ffmpeg, dò PATH, quyền admin** — đây là nguồn gốc phần lớn ticket hỗ trợ của v2. v3 là một tiến trình Rust, cài từ store là dùng được (SM-2).
- **Emoji làm icon trên toolbar** — thay bằng Lucide một bộ, vì emoji render khác nhau giữa macOS và Windows và không có `aria-label`.
- **5 trang HTML rời** — thay bằng SPA một cửa sổ, một shell.
- **Modal update / self-update** — store không cho phép (Apple 2.5.2).
- **Copilot panel (Radar/Sniper/profile/skill)** — bỏ hoàn toàn (Q1).
- **Dialog quản lý template memo** — thay bằng master-detail trong Cài đặt, vì dialog không đủ chỗ cho editor prompt.
- **Cột Model / Segment / Trạng thái trong danh sách phiên** — bỏ (chốt 2026-09-18). Chúng làm dòng phiên dày lên mà không đổi được hành động nào; badge "Thiếu N khoảng" và "Phục hồi" giữ lại **vì chúng đòi hành động**.

**Từ chối có chủ ý:**

- **Hiển thị speaker** — chốt 2026-09-18 không hiển thị ở v3, vì model hiện không phân biệt đủ tin cậy. Hiện một nhãn speaker sai còn tệ hơn không hiện. Trường `speaker` vẫn giữ trong data model.
- **Chế độ offline / Whisper local** — v3 chỉ Gemini. Đổi lại được kích thước bundle ≤ 60 MB và không phụ thuộc ngoài.
- **Lưu bản dịch vào phiên** — bản dịch là công cụ *trong lúc họp*, không phải sản phẩm lưu trữ. Muốn bản dịch bền thì Transcribe lại rồi dịch bản đó.
- **Tự động dọn phiên cũ** — người dùng tự xoá. App không tự ý xoá dữ liệu họp. `[ASSUMPTION: FR-40]`
- **Thông báo thành tích, streak, animation ăn mừng** — đây là công cụ làm việc, không phải app thói quen.
- **Gợi ý bằng AI về việc nên làm gì tiếp** — app trình bày trạng thái, người dùng quyết định.

## Key Flows

### Flow 1 — Linh mở app lần đầu và dùng được trong 3 phút (UJ-1)

*Linh, BrSE 3 năm ở công ty offshore, MacBook M2 chạy macOS 15, họp khách Nhật 2–3 lần/tuần qua Teams. Vừa cài từ Mac App Store, chưa có dữ liệu, chưa cấp quyền gì. Key Gemini do sếp gửi qua Slack.*

1. App mở màn Onboarding, tự chọn ngôn ngữ UI theo hệ thống (vi); card "vi" có badge "Theo hệ thống". Linh giữ nguyên, bấm Tiếp tục.
2. Màn Consent: sơ đồ "Máy của bạn → (bằng key của bạn) → Google Gemini", ba gạch đầu dòng, link Privacy Policy, dòng "Văn bản đồng ý phiên bản 1". Linh bấm "Đồng ý và tiếp tục".
3. Linh dán key. App gọi danh sách model — **đây là request đầu tiên tới Google trong suốt vòng đời app** — và báo trong vùng `role=status`: "Key hợp lệ, 14 model khả dụng".
4. Vào Trang chủ trống: hai card "Kéo file vào đây / Chọn file" và "Bắt đầu Live", kèm danh sách định dạng hỗ trợ.
5. Linh bấm Live → chọn "Mic + Hệ thống [Khuyên dùng]" → macOS hỏi quyền **System Audio Recording** và Micro (không phải Screen Recording, và ghi chú trên màn đã nói trước điều đó).
6. **Cao trào:** Linh cấp quyền, bấm "Bắt đầu ghi", mở thử một video YouTube — và transcript bắt đầu chạy. Chưa tới ba phút kể từ lúc mở app, không cài gì thêm, không wizard, không quyền admin. Đây chính là lời hứa "cài từ store, mở lên là dùng" được chứng minh trong một lần.
7. Linh bấm Dừng; overlay "Đang lưu phiên…" rồi vào Transcript detail. Phiên có tên theo nhãn thời gian; Linh đổi tên inline thành "Test", Enter.

**Thất bại:** key sai hoặc hết hạn → lỗi category "Key bị từ chối" ngay tại Onboarding với một câu hướng dẫn, sửa được **tại chỗ**. Linh cũng có thể bấm "Bỏ qua, nhập sau" và vào app ở trạng thái "chưa có key" — nút cần Gemini vô hiệu có giải thích, **không bao giờ là màn trắng**.

### Flow 2 — Linh họp Teams 60 phút và không mất gì khi app bị force-quit (UJ-2)

*Đã onboard, đang ở Trang chủ, tai nghe Bluetooth, Teams sắp bắt đầu.*

1. Bấm Live → nguồn "Mic + Hệ thống", ngôn ngữ transcribe `auto`, target `vi` (mặc định từ Cài đặt) → "Bắt đầu ghi".
2. Transcript gốc (ja) và bản dịch (vi) chạy song song hai cột; Linh chuyển segmented sang "Cả hai".
3. Giữa họp khách chuyển sang tiếng Anh, bản dịch kém đi → Linh bấm **"Nhận diện lại"**. App mở kết nối mới với ngữ cảnh trống; phiên, Recording và đồng hồ giữ nguyên, timestamp không nhảy, không có segment lặp.
4. Linh bật toggle loa đọc bản dịch. Pill "Đang đọc" hiện ở đầu cột Dịch; volume Teams tự hạ còn 30 % khi app nói và trả lại khi im. Audio TTS không bị chính app thu lại.
5. Linh gõ ghi chú vào panel Ghi chú (mở mặc định, tự lưu, "Đã lưu 14:32").
6. Phút 40 Linh cần phát biểu: đổi select nguồn sang "Chỉ mic" rồi quay lại "Mic + Hệ thống". Có hiệu lực trong ≤ 1 s, **phiên không ngắt**.
7. Trong 60 phút, server Gemini đóng kết nối 6 lần. Linh **không thấy gì ngoài pill kết nối nhấp nháy trong chốc lát**; transcript liền mạch, không mất segment nào.
8. Phút 45 Wi-Fi văn phòng rớt 3 phút. Pill ghi âm vẫn đỏ "Đang ghi 45:02"; pill kết nối chuyển "Đang nối lại (2:10)". Banner: "Vẫn đang ghi âm — transcript sẽ tự chạy tiếp khi có mạng." Linh vẫn gõ ghi chú. Mạng có lại, transcript tự chạy tiếp; 3 phút đó để lại một dòng "Mất kết nối 45:02–48:10" đúng vị trí trong luồng segment.
9. **Cao trào:** Họp xong, Linh bấm Dừng. Overlay "Đang lưu phiên…", rồi Transcript detail mở ra với recording phát lại được, transcript gốc và ghi chú của Linh — đủ cả. Linh chọn template 議事録, bấm "Sinh memo" (badge "Tốn token Gemini" đứng cạnh, nên Linh biết trước), và memo Markdown xuất hiện với ghi chú của Linh đã nhúng sẵn. Buổi họp 60 phút, ba lần mạng trục trặc, sáu lần server đóng kết nối — và **không có gì bị mất**.
10. Linh gắn tag "KH-ABC" và "sprint-12" qua tag picker.

**Thất bại:** phút 50 app bị force-quit. Mở lại app: phiên mồ côi **đã nằm sẵn trong thư viện** với badge "Phục hồi", recording tới phút 50 và transcript đã tích luỹ — không hỏi han, không chặn. Linh bấm "Transcribe lại" từ recording để có bản chất lượng cao hơn, lấp luôn 3 phút mất kết nối; bản mới hiện cạnh bản live (side-by-side), memo và tag giữ nguyên.

### Flow 3 — Minh transcribe file 90 phút và gửi memo trong buổi chiều (UJ-3)

*Minh, sales, Windows 11 laptop công ty có proxy Zscaler. Nhận file `.mp4` recording Zoom 90 phút từ khách. Đã onboard qua Microsoft Store. Có 2 key Gemini.*

1. Minh kéo file thả vào cửa sổ — **bất kỳ chỗ nào trên màn**, không cần nhắm vào drop-zone. App chuyển sang Transcript detail và bắt đầu ngay.
2. Card job hiện tiến độ **theo thời lượng audio thực**: "32 / 90 phút · 36 %", kèm dòng chi tiết chunk và key đang dùng.
3. Minh rời sang Trang chủ xem phiên khác. Card job vẫn hiện ở đầu danh sách **và** thu gọn ở sidebar, có nút "Mở" để quay lại.
4. Một chunk bị quota 429 → app cho key đó nghỉ 60 s và xoay sang key thứ hai; người dùng không phải làm gì. Một chunk khác lỗi hẳn sau 4 lần thử → transcript đánh dấu "Thiếu 45:00–50:00".
5. **Cao trào:** Transcript hoàn tất với banner warning liệt kê khoảng thiếu. Minh bấm **"Chạy lại phần thiếu"** — chỉ 5 phút đó được gửi lại, không phải cả 90 phút, và badge "Tốn token Gemini" đã nói trước chi phí. Sau khi đầy đủ, Minh chọn template "Biên bản họp sales" → Sinh memo → Copy → dán thẳng vào email cho khách. Từ file thô tới memo gửi được, trong một buổi chiều, trên máy công ty có proxy.
6. Phiên nằm trong thư viện với badge memo. Proxy phát lại nằm trong Container nên sau này Minh xoá file `.mp4` gốc vẫn nghe lại và click-to-seek được.

**Thất bại:** khách gửi file `.wmv` → app báo ngay lỗi category "Định dạng", nêu rõ "hãy chuyển sang mp4, m4a hoặc mp3", **trước khi tạo phiên** — không lỗi im lặng, không phiên rác. Nếu proxy Zscaler chặn TLS → lỗi category "Mạng / CA" với hướng dẫn riêng, không phải "Có lỗi xảy ra".

### Flow 4 — Linh tìm lại một câu khách nói ba tuần trước (UJ-4)

1. Trang chủ → bấm chip tag "KH-ABC" ở hàng chip. Footer đổi thành "6 / 38 phiên · lọc…".
2. Linh gõ "giao diện admin" vào ô tìm theo tên — không ra kết quả. Trạng thái rỗng có nút "Xoá bộ lọc", nhưng Linh chỉ xoá query; **lọc tag giữ nguyên**.
3. Linh mở phiên nghi ngờ nhất → `⌘F` → gõ "giao diện admin" → đếm "1/3", Enter để nhảy tiếp, kết quả highlight vàng và transcript tự cuộn tới.
4. **Cao trào:** Linh click vào segment → trình phát nhảy đúng tới `start` và phát giọng khách. Nghe được đúng câu trong **30 giây**, thay vì tua cả recording 90 phút. Linh nghe lại lần nữa để chắc chắn mình không hiểu nhầm yêu cầu.
5. Export `.srt` qua dialog lưu hệ thống, gửi cho QA.

**Thất bại:** proxy phát lại hỏng → transcript vẫn mở và tìm được; thanh phát hiện "Không có audio · Chọn lại file nguồn" để tạo lại proxy. Linh vẫn đọc được câu đó, chỉ là chưa nghe được.

### Flow 5 — Reviewer của Apple mở app lần đầu với key demo (UJ-5)

*Reviewer không có key Gemini riêng; đọc App Review Notes có key demo và 3 bước.*

1. Onboarding → chọn ngôn ngữ (reviewer chọn `en`) → Consent: nêu đích danh Google là bên nhận dữ liệu, link Privacy Policy mở trình duyệt ngoài.
2. Dán key demo → "Key hợp lệ, N model khả dụng".
3. Trang chủ trống **có hướng dẫn rõ ràng** — không phải màn trắng.
4. Kéo file mẫu vào → transcript chạy.
5. Mở Cài đặt → thấy link Privacy Policy, thấy "Xem lại văn bản đồng ý", và cạnh ad slot có nhãn "Sponsored", nút "Báo cáo quảng cáo", link "Vì sao tôi thấy quảng cáo này".
6. **Cao trào:** Reviewer đi hết một vòng sản phẩm mà **không gặp màn hình trắng, không crash, không lời xin quyền lạ** (đặc biệt: không Screen Recording), **không gợi ý tải bản ngoài store**, và không có quảng cáo nào chen vào lúc đang transcribe. Mọi thứ cần để đánh giá 2.4.5, 2.5.2, 2.5.18, 5.1.1 và 5.1.2 đều nhìn thấy được mà không phải hỏi.

**Thất bại:** key demo hết quota trong lúc review → lỗi category "Quota" nói rõ nguyên nhân và gợi ý chờ/thêm key, thay vì crash hay màn trắng. App vẫn mở, vẫn xem được phiên cũ, vẫn phát lại được.
