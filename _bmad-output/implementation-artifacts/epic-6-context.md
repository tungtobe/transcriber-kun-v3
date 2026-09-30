# Epic 6 Context: Cấu hình từ xa & Ad slot

<!-- Compiled from planning artifacts. Edit freely. Regenerate with compile-epic-context if planning docs change. -->

## Goal

Cung cấp một đường tải nội dung từ xa có xác minh để người dùng chủ động xem trước và áp dụng cấu hình đề xuất, đồng thời hiển thị house ads tuân thủ store. Nội dung remote không được ghi đè key hoặc làm ảnh hưởng phiên họp, transcript hay recording; quảng cáo không tracking và tuyệt đối vắng mặt trên route Live.

## Stories

- Story 6.1: Module `remote/` — tải, xác minh chữ ký, cache, fallback
- Story 6.2: Đồng bộ cấu hình đề xuất
- Story 6.3: Pipeline Creative — chọn, xoay, giới hạn và cờ tắt quảng cáo
- Story 6.4: Ad slot ở sidebar và tuân thủ store
- Story 6.5: Công cụ ký, xuất bản nội dung từ xa và smoke test hằng tuần

## Requirements & Constraints

- Cấu hình đề xuất là tuỳ chọn, chỉ tải theo thao tác người dùng; hiển thị diff trước khi áp dụng. Không bao giờ liệt kê hoặc ghi key hay Consent. Chỉ thay đổi thiết lập được cho phép; lỗi tải/xác minh không được làm đổi settings.
- House ads chọn theo locale UI và thời hạn, xoay theo trọng số, tối đa một impression cho mỗi creative trong 10 phút. Creative click mở trình duyệt ngoài. Nội dung phải có nhãn Sponsored, lối báo cáo và giải thích tĩnh “Vì sao tôi thấy quảng cáo này”.
- Không gửi cookie, identifier, telemetry hoặc impression/click ra ngoài; chỉ đếm cục bộ. Cờ `ads_enabled=false` hoặc `is_premium=true` ẩn slot hoàn toàn. Quyết định sản phẩm của epic: v3 không có gửi thống kê, cũng không có tuỳ chọn opt-in.
- Ads/remote lỗi phải được cô lập: không đổi trạng thái transcript, dừng Job/Live hay làm gián đoạn Recording. Offline hoặc không có creative hợp lệ thì dùng creative nhúng sẵn, trừ khi cờ tắt quảng cáo đang có hiệu lực.
- Creative không được che nội dung, phát âm thanh, tự phát hay dùng interstitial. Ảnh tối đa 100 KB; chỉ ảnh được phép, không HTML/script. Không dùng dữ liệu server làm đường dẫn file.

## Technical Decisions

- Mọi GET tới nguồn HTTPS đã cấu hình cho `ads.json`, ảnh và `recommended-settings.json` đi qua `remote/`; không cookie, định danh hay header nhận diện người dùng. Không tải qua module feature riêng.
- Xác minh envelope/chữ ký ed25519 bằng public key nhúng trong binary. Cache đã xác minh tại `cache/remote/<sha256-của-url>`, TTL 24 giờ; URL nguồn và key là cấu hình build. Nội dung lỗi chữ ký dùng cache đã xác minh hoặc fallback nhúng; payload cấu hình tải thất bại không được coi là bản mới thành công. Ràng buộc redirect, timeout, giới hạn kích thước khi stream và decode; digest ảnh phải được manifest ký bảo vệ.
- Tên file ảnh dùng ID tự sinh/hash, không dùng Creative ID hay chuỗi server. `remote/` chỉ tải/xác minh; mọi đọc/ghi settings thuộc `settings/` trong Rust/SQLite. Preview gắn digest payload và revision settings; khi stale phải tính lại. Apply chạy validator/allow-list và ghi transaction nguyên tử.
- `ads/` quản lý locale, thời hạn, weighted rotation, cap và đếm cục bộ. Impression chỉ ghi khi thực sự hiển thị; cap lưu qua điều hướng/restart. Fallback không lặp impression do remount. Không gọi chọn ads hoặc đếm impression trên `/live`.
- Lỗi phụ trợ được phân loại/redact và không panic. Công cụ ký phải tạo đúng envelope, bytes ký và digest ảnh mà client xác minh; private key nằm ngoài repo ứng dụng, có quy trình giữ và xoay khoá.

## UX & Interaction Patterns

- Ad slot nằm dưới sidebar rộng 260 px, slot rộng 236 px; chỉ hiện ở Home, Transcript detail và Settings. Layout quyết định việc ẩn slot trên mọi route Live, gồm LiveSetup, đang ghi và đang lưu.
- Slot có nền surface, viền 1 px, bo góc 10 px; ảnh + text tĩnh cỡ 300×100 co theo slot hoặc 320×50. Khi tải/offline vẫn giữ cùng kích thước bằng fallback để tránh layout nhảy. Hỗ trợ theme sáng/tối, vi/en/ja, tương phản AA và bàn phím/aria-label.
- Nhóm “Cấu hình đề xuất” trong Settings tải diff inline, không thêm route; mỗi dòng hiển thị nhãn, giá trị hiện tại và giá trị đề xuất. Có Áp dụng/Huỷ, toast sau khi áp dụng và banner lỗi tại chỗ.

## Cross-Story Dependencies

- Story 6.1 phụ thuộc 1.2; đây là nền chung cho ads và cấu hình.
- Story 6.2 phụ thuộc 6.1, settings 1.9 và Template 3.6. Story 6.3 phụ thuộc 6.1; Story 6.4 phụ thuộc 6.3 và layout/theme 1.3.
- Story 6.5 phụ thuộc 6.1, CI 1.7 và xác minh model Live 4.4. Trước xuất bản cần chốt Open Question 6: nơi host nội dung/Privacy Policy, URL cuối và người/cơ chế giữ private key.
