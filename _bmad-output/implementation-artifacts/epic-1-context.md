# Epic 1 Context: Cài đặt & mở app lần đầu

<!-- Compiled from planning artifacts. Edit freely. Regenerate with compile-epic-context if planning docs change. -->

## Goal

Dựng nền tảng greenfield cho toàn sản phẩm và đưa người dùng từ cài bản beta tới vào được Home dùng được, không bao giờ gặp màn trắng. Epic này đóng băng bộ khung mọi epic sau dựa vào: cấu trúc repo + binding IPC typed, lưu trữ + lỗi có category + log content-free, theme sáng/tối, i18n ba ngôn ngữ, luồng Onboarding (ngôn ngữ → Consent → API key), kho khoá OS + key pool, cổng gọi Gemini duy nhất, khung Settings, và hai bản build store rỗng tính năng (TestFlight for Mac, Windows Store package flight) để gỡ sớm rủi ro cert/sandbox/keyring.

## Stories

- Story 1.1: Dựng repo greenfield, CI hai OS và binding IPC typed
- Story 1.2: Nền lưu trữ, lỗi có category và log content-free
- Story 1.3: Token thị giác, theme sáng/tối và app shell
- Story 1.4: i18n vi/en/ja và Onboarding chọn ngôn ngữ
- Story 1.5: Onboarding Consent
- Story 1.6: Kho khoá OS và Key pool
- Story 1.7: Cổng Gemini duy nhất và liệt kê model
- Story 1.8: Onboarding nhập key và Home khởi đầu khi chưa có key
- Story 1.9: Settings — khung, Chung và Gemini
- Story 1.10: Settings — Chẩn đoán và Giới thiệu & Quyền riêng tư
- Story 1.11: Bản beta macOS lên TestFlight for Mac (spike S5)
- Story 1.12: Bản beta Windows MSIX qua Store package flight (spike S6)

## Requirements & Constraints

- Onboarding theo đúng thứ tự Ngôn ngữ → Consent → API key, không có bước kiểm tra môi trường; ngôn ngữ mặc định theo hệ thống (fallback `en`), đổi tức thì không reload.
- Không request nào tới Google (kể cả liệt kê model) trước khi Consent phiên bản hiện hành được ghi nhận; Consent lưu bền theo version, chỉ hỏi lại khi văn bản tăng version; từ chối → chỉ xem Settings/About, có lối quay lại, không màn trắng.
- Phải luôn vào được Home dùng được kể cả bỏ qua/nhập sai key; nút cần Gemini bị vô hiệu kèm giải thích + lối tắt, không bao giờ ẩn.
- API key chỉ lưu trong kho khoá OS, không bao giờ ở file cấu hình/DB dạng plaintext; xoá trong UI phải xoá thật khỏi kho khoá; kho khoá lỗi → category `storage`, không mất setting khác.
- Key pool xoay theo loại lỗi (quota nghỉ rồi đổi key kế; auth loại khỏi vòng; lỗi client/timeout không xoay) theo một chính sách chung cho mọi luồng Gemini; hết key khả dụng thì tác vụ chờ có ngân sách thời gian rõ, không fail ngay.
- Model transcribe/live/memo chọn từ danh sách tải qua API hoặc nhập tự do; lỗi category `model` không bao giờ tự đổi model của người dùng.
- Mọi lỗi hiển thị quy về 8 category ổn định (quota, auth, model, network, format, permission, storage, blocked) kèm một hành động gợi ý, không lộ stack trace/key/URL.
- Log và bản xuất log content-free tuyệt đối (không transcript/dịch/ghi chú/memo/key); chỉ đếm cục bộ (số phiên, lỗi theo category, crash) — không thống kê gửi đi, không opt-in dưới bất kỳ hình thức nào.
- Không telemetry, không server trung gian, không phụ thuộc ngoài (không sidecar/binary ngoài, không self-update, một tiến trình), không quyền admin.
- ID sinh bằng UUIDv7; không dùng chuỗi từ người dùng/server làm thành phần đường dẫn file.
- Giao diện đạt WCAG AA đo riêng từng theme, điều hướng bàn phím cho thao tác chính; đổi theme tức thì.
- Bộ khoá i18n ba ngôn ngữ phải đồng bộ tuyệt đối; CI fail nếu thiếu key ở bất kỳ ngôn ngữ nào.

## Technical Decisions

- Repo mới theo cấu trúc chuẩn (không starter template): frontend chia store theo domain bọc binding sinh tự động; backend Rust chia module theo feature (ipc, core, settings, library, transcribe, live, memo, ads, gemini, media, audio, db, secrets, remote). Phụ thuộc một chiều ipc → feature → hạ tầng → core; feature không import lẫn nhau, điều phối chéo feature chỉ ở `ipc/`.
- Binding IPC sinh tự động, không viết tay; CI fail nếu binding lệch bản sinh; mọi command trả kết quả có kiểu lỗi thống nhất.
- Cấm dùng nhóm plugin framework chính thức cho shell/fs/updater/process/http/store và mọi sidecar/binary ngoài; thêm loại dependency này là thay đổi kiến trúc, phải cập nhật tài liệu nguồn.
- Trạng thái đồng thời phức tạp (đặc biệt key pool) chỉ dùng mô hình actor (task sở hữu state, giao tiếp qua kênh lệnh/phản hồi), không bọc bằng khoá chia sẻ trực tiếp.
- Một kết nối SQLite (WAL) duy nhất, truy cập tuần tự; SQL chỉ nằm trong tầng repository theo entity; migration chỉ tiến. Migration đầu chỉ tạo bảng settings.
- Settings chỉ qua service typed có giá trị mặc định, phát sự kiện khi đổi; không dùng cơ chế store key-value chung của framework.
- Lỗi qua IPC là kiểu có cấu trúc `{category, code, chi tiết đã ẩn}`; ánh xạ mã kỹ thuật → category định nghĩa một nơi duy nhất, có test phủ mọi mã.
- Dữ liệu nhạy cảm (transcript, dịch, ghi chú, memo, key, prompt) bọc kiểu wrapper luôn hiển thị dạng đã ẩn; log có lớp redaction tự động; test tự động grep log sau phiên mẫu.
- Mọi gọi Gemini (REST + WebSocket) chỉ qua một cổng gateway duy nhất, REST thuần không SDK; cổng tự chặn khi chưa Consent, cấp key theo pool có ưu tiên giữa các luồng; không gọi nền/định kỳ.
- Tham số vận hành (cooldown, số lần thử, timeout) định nghĩa tập trung, test bằng port giả + đồng hồ giả.
- Version app một nguồn duy nhất; dependency nhạy cảm (đặc biệt bản pre-release của công cụ sinh binding) pin chính xác.
- Hai bản build store dùng overlay cấu hình riêng: macOS sandbox với entitlement tối thiểu (mạng, mic, file người dùng chọn, keychain nếu cần), không updater; Windows MSIX với capability microphone. Kiểm thử trên kênh phân phối thật, không chỉ máy dev.

## UX & Interaction Patterns

- App shell cố định: sidebar 260px, header 64px; cửa sổ hẹp thì panel phụ đóng trước, sidebar giữ nguyên; cửa sổ tối thiểu 1024×680.
- Hệ token thị giác (màu, spacing, radius, elevation phẳng, typography bundle sẵn, icon bộ duy nhất, focus ring nhất quán) áp dụng ngay từ epic này cho mọi màn về sau, không dồn tới cuối dự án; bộ token dark còn phần cần chủ sản phẩm/UX duyệt trước khi code.
- Onboarding: card giữa màn, stepper 3 bước không có kiểm tra môi trường; bước ngôn ngữ dùng radio card có badge "theo hệ thống"; bước Consent có sơ đồ luồng dữ liệu tới Google; bước key có kiểm tra tại chỗ (kết quả trong vùng trạng thái đọc được bằng trình đọc màn hình) và luôn có lối bỏ qua.
- Mẫu dùng chung toàn app: banner lỗi/cảnh báo ba phần (category, nguyên nhân + cách sửa, một hành động) theo mức nghiêm trọng tối đa hai banner cùng lúc; nút vô hiệu luôn hiện kèm tooltip lý do, tới được bằng bàn phím.
- Settings dùng layout nav-trái + bảng hai cột, mỗi trường có tooltip và helper text bền; nhóm tính năng chưa hoàn thành ở epic hiện tại bị ẩn; ô key ẩn ký tự mặc định kèm nút hiện/ẩn.
- Router chỉ đăng ký route của epic đã có màn thật; đổi theme/ngôn ngữ có hiệu lực tức thì.

## Cross-Story Dependencies

Chuỗi trong epic: 1.1 → 1.2 → 1.3 → 1.4 → 1.5; 1.6 phụ thuộc 1.2 (chạy song song nhánh Onboarding); 1.7 phụ thuộc 1.5 và 1.6; 1.9 phụ thuộc 1.3, 1.4, 1.7; 1.8 phụ thuộc 1.7 và 1.9; 1.10 phụ thuộc 1.5 và 1.9; 1.11/1.12 (hai spike build store) phụ thuộc 1.1, 1.2, 1.6, có thể chạy song song với nhánh Onboarding/Settings.

Mọi epic sau xây trên nền epic này: cổng Gemini + key pool được tái dùng nguyên vẹn cho transcribe file, live, memo, cấu hình từ xa; kiểu lỗi có category và redaction dùng xuyên suốt; khung Settings được các epic sau chỉ bổ sung nhóm mới chứ không viết lại; app shell/router/theme/i18n là hạ tầng bắt buộc cho mọi màn mới. Hai spike build store là điều kiện để Epic 7 nâng thành bản submit đầy đủ. Việc duyệt token dark (trước 1.3) và các quyết định chưa chốt về tên store/tài khoản, nơi host Privacy Policy (ảnh hưởng bản beta công khai ở 1.5) là gate ngoài epic có thể chặn tiến độ nếu chưa xong.
