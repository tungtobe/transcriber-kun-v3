# Mockup trans-kun v3

Bản tĩnh của canvas thiết kế — **17 artboard**, sinh lại được bằng `python3 build.py` (chỉ dùng thư viện chuẩn).

- `build.py` — nguồn sinh: token CSS, icon (Lucide), shell sidebar, từng màn, **và bộ sinh dark**. Sửa ở đây rồi chạy lại để sinh `project/`.
- `project/*.dc.html` — mỗi file một artboard, tự chứa CSS. Mở trực tiếp bằng trình duyệt vẫn xem được (thiếu `support.js` của canvas chỉ mất tương tác Play/link giữa màn).
- `project/canvas.json` — vị trí artboard trên canvas.

## Artboard

| Nhóm | File |
|---|---|
| Onboarding | `OnboardingLang` · `OnboardingConsent` · `OnboardingKey` |
| Home | `HomeEmpty` · `Home` · `TagPicker` |
| Detail & Cài đặt | `Transcript` · `Settings` · `SettingsMemo` |
| Live | `LiveSetup` · `Live` · `LiveSaving` |
| Dùng chung | `Components` |
| **Dark** | `HomeDark` · `TranscriptDark` · `LiveDark` · `ComponentsDark` |

## Dark mode

Dark **nằm trong bản submit store đầu tiên**. `build.py` sinh bản dark bằng `DARK_MAP` — ánh xạ **từng token light sang đúng token dark đã thiết kế** trong `DESIGN.md`, không phải phép đảo màu. Hai ngoại lệ được xử lý riêng vì cùng một mã hex mang hai vai:

- `#0F766E` **giữ nguyên** khi là nền nút primary, chuyển `#2DD4BF` khi là chữ/icon/viền.
- `#FFFFFF` **giữ nguyên** khi là chữ/icon trên nền đặc, chuyển `#1F2024` khi là surface.

Muốn thêm màn dark: thêm tên file vào `DARK_SOURCES` trong `build.py`.

## Nguồn sự thật

Chuỗi ưu tiên: **`DESIGN.md` + `EXPERIENCE.md`** (trong `_bmad-output/planning-artifacts/ux-designs/ux-transcriber_kun-2026-09-18/`) → `../MASTER.md` → mockup này.

Mockup là **tham chiếu bố cục, không phải hợp đồng**. Khi lệch nhau, sửa mockup theo spine.

Đây không phải code frontend v3; frontend thật dựng bằng Svelte 5 + TS theo `docs/rebuild-v3/04-kien-truc-moi-rust-only.md`.
