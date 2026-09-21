# Đối chiếu — `design-system/trans-kun/mockup/` → hai spine

**Ngày:** 2026-09-18 · **Phạm vi:** 17 artboard tĩnh (`project/*.dc.html`) + nguồn sinh `build.py`.

## Quyết định về vị trí

Mockup **ở nguyên tại `design-system/trans-kun/mockup/`**, không sao chép vào `imports/` hay `mockups/` của workspace này. Lý do: chúng có nguồn sinh (`build.py`) đang được bảo trì; nhân bản sẽ tạo hai bản lệch nhau. Hai spine trỏ tới chúng bằng link tương đối.

**Quan hệ:** mockup là **tham chiếu bố cục**, spine là **hợp đồng**. Spine thắng khi mâu thuẫn.

## Độ phủ artboard

| Artboard | Đã chắt vào spine | Mục |
|---|---|---|
| `OnboardingLang` | Radio card ngôn ngữ, badge "Theo hệ thống" | IA · Flow 1 |
| `OnboardingConsent` | Sơ đồ luồng dữ liệu, phiên bản văn bản Consent, hai nút | Minh bạch chi phí & quyền riêng tư · Flow 1, 5 |
| `OnboardingKey` | Input password + hiện/ẩn + Kiểm tra key, kết quả `role=status` | Component Patterns · Accessibility Floor |
| `HomeEmpty` | Hai card trạng thái trống, danh sách định dạng, banner chưa có key | State Patterns |
| `Home` | Drop-zone mỏng, hàng chip tag, card job, danh sách phiên, footer bộ lọc | IA · Component Patterns · State Patterns |
| `TagPicker` | Popover dùng chung ba nơi, ô tìm kiêm tạo mới, nhóm "Đang lọc" | Component Patterns |
| `Transcript` | Trình phát, banner partial, segmented chọn bản, grid 56px, panel 360px | Component Patterns · State Patterns |
| `LiveSetup` | 3 radio card nguồn, ô tag, hai select, ghi chú quyền | IA · Flow 1, 2 |
| `Live` | Toolbar 64px, hai pill riêng, hai cột, panel 320px, không ad slot | Độ bền phiên · Ad slot |
| `Settings` | Nav 220px, bảng 2 cột, tooltip mỗi trường | Component Patterns |
| `SettingsMemo` | Master-detail, hàng kiểm tra `{transcript}`/`{notes}` | Component Patterns |
| `Components` | Bảng màu, 4 trạng thái Live, badge, nút, **6 mẫu lỗi**, giải phẫu ad slot | → `Taxonomy lỗi` (mở rộng từ 6 lên **8 category** theo NFR-9) |

Không có artboard nào mồ côi. Không có bề mặt IA nào thiếu artboard.

## Đồng bộ với spine — ĐÃ XONG 2026-09-18

| Điểm | Trước | Sau |
|---|---|---|
| Bề rộng sidebar | 232px | **260px** (`build.py`, `sidebar(w=260)`) |
| Vùng ad slot | 208px | **236px** |
| Sau khi Dừng Live | chưa thể hiện | artboard mới **`LiveSaving`** — overlay "Đang lưu phiên…" |
| Dark mode | chỉ light | **4 artboard dark**: `HomeDark`, `TranscriptDark`, `LiveDark`, `ComponentsDark` |

Tổng: **12 → 17 artboard**. Bộ sinh dark (`DARK_MAP` trong `build.py`) ánh xạ từng token light sang đúng token dark đã thiết kế, xử lý riêng hai mã hex mang hai vai (`#0F766E` nền-vs-chữ, `#FFFFFF` surface-vs-chữ).

## Ý trong mockup không lên spine (có chủ ý)

- **`canvas.json`** (toạ độ artboard trên canvas) — thuần công cụ, không phải quyết định thiết kế.
- **`support.js`** của canvas (tương tác Play, link giữa màn) — thuộc môi trường xem mockup, không phải hành vi sản phẩm.
- **Nội dung mẫu** (tên phiên, chuỗi transcript ja/vi cụ thể) — là dữ liệu minh hoạ. Riêng **giọng microcopy** trong đó đã được chắt vào `EXPERIENCE.md.Voice and Tone`.
