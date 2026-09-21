---
title: Đối chiếu 03-dieu-kien-len-store.md với PRD + Addendum
created: 2026-09-17
---

# Đối chiếu điều kiện Store (docs/rebuild-v3/03-dieu-kien-len-store.md) ↔ PRD & Addendum

Input tra cứu: `docs/rebuild-v3/03-dieu-kien-len-store.md` (2026-09-14).
Đối chiếu với: `prd.md` (đặc biệt §5 NFR, §6 Ràng buộc, §7 Nền tảng, §9 Monetization, §11–13) và `addendum.md` (đặc biệt mục I, J, K).

## Khoảng trống

- INPUT §1.1 (dòng 13, 16) — App ID / provisioning "Mac App Store Connect" / App Store Connect API key / cert **Apple Distribution** + **Mac Installer Distribution** không được liệt kê tường minh làm hạng mục cần chuẩn bị (addendum J chỉ nhắc `provisionprofile`, `productbuild ký "3rd Party Mac Developer Installer"`, thiếu tên 2 loại cert và bước tạo App ID/API key) → nên vào **addendum** (mục J, build & phân phối); mức **thấp**.
- INPUT §1.1 (dòng 17) "Category (`bundle.category`) bắt buộc" — addendum J có nhắc "category" trong overlay config nhưng PRD/addendum không nói app thuộc category nào (vd Productivity) → nên vào **addendum** (mục J); mức **thấp**.
- INPUT §1.4 (dòng 72) "mọi ngôn ngữ trong một bundle" (đóng gói localization, khác với FR-47 i18n runtime) — không xuất hiện ở đâu trong PRD/addendum như một ràng buộc đóng gói → nên vào **addendum** (mục J); mức **thấp**.
- INPUT §1.4 (dòng 74) "Không dùng API riêng tư" — không được phát biểu tường minh như một constraint trong PRD §6 dù ngụ ý qua việc dùng crate công khai (symphonia/cpal/coreaudio-sys) → nên vào **PRD §6.2** hoặc **addendum**; mức **thấp**.
- INPUT §2.1 (dòng 88) "Partner Center developer account, phí đăng ký" — hoàn toàn không nhắc ở PRD/addendum (chỉ có việc "reserve tên" ở Open Question 5 của PRD) → nên vào **addendum** (mục J hoặc K, lộ trình/setup); mức **thấp**.
- INPUT §2.3 (dòng 117) "Certification tối đa ~3 ngày làm việc" — không có trong PRD/addendum, có thể ảnh hưởng lộ trình Phase 7 (ra mắt) ở addendum K → nên vào **addendum** (mục K, lộ trình & spike); mức **thấp**.
- INPUT §2.4 (dòng 120–121) "Microsoft cho phép app non-game dùng hệ thống thanh toán riêng song song Microsoft commerce; hoa hồng khác nhau theo đường" — không được ghi nhận ở đâu, kể cả như bối cảnh cho Premium/IAP tương lai (Q11, §9 Monetization của PRD chỉ nói chung chung "Chừa sẵn... trait nguồn key") → nên vào **addendum** (bổ sung ghi chú ngữ cảnh cho Q11), vì ảnh hưởng thiết kế monetization Windows sau này; mức **trung**.
- INPUT §2.3 (dòng 118) "MSIX khai capability `microphone`; loopback WASAPI không cần capability đặc biệt" — addendum J có "capability `microphone`" nhưng câu "loopback không cần capability đặc biệt" không được xác nhận lại ở đâu (không phải mâu thuẫn, chỉ là chưa nêu) → nên vào **addendum** (mục F hoặc J); mức **thấp**.
- INPUT §5 checklist (dòng 182) "Screenshot theo kích thước từng store, mô tả 3 ngôn ngữ, **age rating (4+)**" — đây là khoảng trống rõ nhất: PRD không có bất kỳ FR/NFR/ràng buộc nào về store-listing assets (screenshot, mô tả marketing 3 ngôn ngữ, age rating), addendum cũng không nhắc. Đây là điều kiện bắt buộc để submit cả hai store và có thể ảnh hưởng SM-1 ("Lên được cả hai store") → nên vào **PRD §6.2** (Tuân thủ store) hoặc ít nhất **addendum J**; mức **cao**.
- INPUT §5 checklist (dòng 184) "Package.appxmanifest (identity, publisher, capabilities), assets icon" — addendum J chỉ nói "MSIX x64 + Arm64; Store ký; capability `microphone`", không nhắc identity/publisher/icon assets như hạng mục cần chuẩn bị → nên vào **addendum** (mục J); mức **thấp**.
- INPUT §5 checklist (dòng 185) "Kiểm thử sandbox thật (`codesign -d --entitlements`, chạy từ `/Applications`, mở file bằng dialog, relaunch xem history vẫn phát được)" — phần "relaunch vẫn phát được" đã được phản ánh gián tiếp qua FR-10 ("Mở lại app sau relaunch trong sandbox vẫn phát được mọi Phiên") và SM-1, nhưng bước kiểm tra `codesign`/chạy từ `/Applications` như một hạng mục QA/test-plan tường minh thì chưa có ở PRD/addendum → nên vào **addendum** (mục K/J, như một bước QA trước submit); mức **thấp**.
- INPUT §1.5 (dòng 80) "Đánh giá ~1–2 tuần" cho spike bridge StoreKit 2 — addendum K (spike S7) chỉ ghi "chỉ nếu chọn freemium — không ở v3" mà không giữ lại ước lượng effort này cho tham khảo tương lai → nên vào **addendum** (mục K); mức **thấp** (không cấp bách vì ngoài phạm vi v3).

## Mâu thuẫn

- **Tên app không nhất quán giữa các tài liệu**: INPUT §1.4 (dòng 75) vẫn dùng tên "Transcriber-kun" khi nói về kiểm tra trùng tên trên App Store Connect, trong khi PRD (tiêu đề, §1 Tầm nhìn, Q9 trong addendum: "Tên app trans-kun, bundle ID `com.transkun.app`") và Open Question 5 của PRD đều dùng tên mới "trans-kun". Đây nhiều khả năng là do doc 03 được viết trước khi quyết định đổi tên (Q9, chốt cùng ngày 2026-09-13/14) chưa được cập nhật ngược lại. Không phải mâu thuẫn về yêu cầu nghiệp vụ, nhưng nên cập nhật doc 03 (hoặc ghi chú) để tránh nhầm khi ai đó dùng lại "Transcriber-kun" lúc reserve tên/App ID trên App Store Connect hoặc Partner Center.
- Không phát hiện mâu thuẫn nội dung khác (bundle ID `com.transcriberkun.app` ở INPUT §1.1 vs `com.transkun.app` ở addendum Q9 là cùng loại vấn đề trên — hệ quả của đổi tên, không phải xung đột yêu cầu).

## Đã phủ tốt

- App Sandbox 2.4.5/2.5.2, cấm tải/chạy code, cấm self-update, không sidecar → NFR-4, NFR-7, §6.2, addendum B.
- Quyền hệ thống: Core Audio tap macOS ≥14.4 (không Screen Recording), WASAPI loopback Windows → FR-17, NFR-7, PRD §7, addendum F, khớp đúng quyết định Q3.
- Privacy: consent trước khi gửi Google (5.1.2), privacy policy 5.1.1, privacy label, BYOK + key demo reviewer, `ITSAppUsesNonExemptEncryption=false` → FR-2, FR-3, §6.1, §6.2, UJ-5, Open Question 4.
- Universal binary macOS, Windows x64+Arm64, WebView2 Evergreen kiểm tra runtime → PRD §7 khớp sát INPUT §1.4/§2.2.
- IAP/StoreKit: đúng hướng "chưa dùng ở v3, chỉ chừa hook" → §6.2, §9, Q11, non-goals, addendum K (spike S7).
- Quảng cáo house-ads: schema Creative, kích thước, cap tần suất, nhãn Sponsored + nút Báo cáo/Vì sao, không tracking/ATT, cờ `ads_enabled`/`is_premium`, chữ ký `ads.json` → FR-44/45/46, §6.1, §6.4, addendum I, khớp gần như 1:1 với phần "Đã chốt" của INPUT §3.
- Ma trận §4 của INPUT (audio, mic, TTS, export, auto-update cấm, cache dir bỏ) → phản ánh đầy đủ qua FR-10, FR-17, FR-21, FR-35, FR-40, NFR-4, non-goals.
- Cache dir tuỳ chỉnh bị bỏ (Q7), không import v2 (Q8), không Linux (Q6), không kênh tải trực tiếp (Q5), không auto-update → non-goals PRD, khớp đúng.
- WACK pass được nêu tường minh trong §6.2 PRD.
