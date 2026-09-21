---
name: trans-kun v3
description: Ứng dụng desktop transcribe/dịch họp Nhật–Việt (Tauri 2 + Svelte 5, macOS + Windows, phân phối store). Hướng "quiet utility" — flat, không đổ bóng, nền ấm, một accent teal kế thừa v2, transcript là nhân vật chính.
status: final
updated: 2026-09-18
sources:
  - design-system/trans-kun/MASTER.md
  - design-system/trans-kun/mockup/
  - _bmad-output/planning-artifacts/prds/prd-transcriber_kun-2026-09-17/prd.md
  - docs/rebuild-v3/06-thiet-ke-giao-dien-v3.md
colors:
  # --- Light (mặc định) ---
  bg: '#F6F6F2'
  bg-sidebar: '#F0F0EB'
  surface: '#FFFFFF'
  surface-sunken: '#ECEDE8'
  border: '#E1E4E8'
  border-strong: '#CFD4DA'
  text: '#171A1F'
  text-secondary: '#3C4551'
  text-muted: '#5B6470'
  accent: '#0F766E'
  accent-hover: '#0B5D57'
  accent-soft: '#E6F3F1'
  accent-border: '#B7DDD8'
  danger: '#B42318'
  danger-strong: '#A11E1E'
  danger-soft: '#FDE8E8'
  danger-border: '#F0B8B3'
  warning: '#8A4B0A'
  warning-soft: '#FDF0DC'
  warning-border: '#F5D9A8'
  info: '#1E4E9B'
  info-soft: '#E8EEF7'
  recover: '#5B2FA3'
  recover-soft: '#F0EAFB'
  memo: '#0B5D57'
  memo-soft: '#E0F2F1'
  token-soft: '#FFF7E6'
  mark: '#FDE68A'
  overlay: 'rgba(23,26,31,.45)'
  # --- Dark (song song, không đảo màu) ---
  bg-dark: '#141517'
  bg-sidebar-dark: '#1A1B1E'
  surface-dark: '#1F2024'
  surface-sunken-dark: '#2A2B30'
  border-dark: '#2E3036'
  border-strong-dark: '#3C3F47'          # [ASSUMPTION] MASTER.md chưa nêu
  text-dark: '#ECEDEF'
  text-secondary-dark: '#C2C7CF'         # [ASSUMPTION] MASTER.md chưa nêu
  text-muted-dark: '#A0A6B0'
  accent-dark: '#2DD4BF'                 # chữ/icon; nút primary vẫn nền #0F766E chữ trắng
  accent-hover-dark: '#5EEAD4'           # [ASSUMPTION] MASTER.md chưa nêu
  accent-soft-dark: 'rgba(45,212,191,.14)'
  accent-border-dark: 'rgba(45,212,191,.38)'  # [ASSUMPTION] MASTER.md chưa nêu
  danger-dark: '#F87171'
  danger-strong-dark: '#FCA5A5'          # [ASSUMPTION] chữ trên nền danger-soft-dark
  danger-border-dark: 'rgba(248,113,113,.38)'  # [ASSUMPTION]
  danger-soft-dark: 'rgba(248,113,113,.14)'   # [ASSUMPTION]
  warning-dark: '#F5B75C'
  warning-border-dark: 'rgba(245,183,92,.38)'   # [ASSUMPTION]
  warning-soft-dark: 'rgba(245,183,92,.14)'   # [ASSUMPTION]
  info-dark: '#7AA7F0'
  info-soft-dark: 'rgba(122,167,240,.14)'     # [ASSUMPTION]
  recover-dark: '#C4A6F5'                     # [ASSUMPTION]
  recover-soft-dark: 'rgba(196,166,245,.14)'  # [ASSUMPTION]
  memo-dark: '#5EEAD4'                        # [ASSUMPTION]
  memo-soft-dark: 'rgba(94,234,212,.12)'      # [ASSUMPTION]
  token-soft-dark: 'rgba(245,183,92,.10)'     # [ASSUMPTION]
  mark-dark: '#7A6019'                        # highlight tìm kiếm trên nền tối; đo được 5.10:1 với text-dark
  overlay-dark: 'rgba(0,0,0,.60)'             # [ASSUMPTION]
typography:
  h1:
    fontFamily: 'IBM Plex Sans'
    fontSize: 22px
    fontWeight: '600'
    lineHeight: '1.3'
  screen-title:
    fontFamily: 'IBM Plex Sans'
    fontSize: 18px
    fontWeight: '600'
    lineHeight: '1.35'
  h2:
    fontFamily: 'IBM Plex Sans'
    fontSize: 16px
    fontWeight: '600'
    lineHeight: '1.4'
  option-title:
    fontFamily: 'IBM Plex Sans'
    fontSize: 15px
    fontWeight: '600'
    lineHeight: '1.45'
  body:
    fontFamily: 'IBM Plex Sans'
    fontSize: 14px
    fontWeight: '400'
    lineHeight: '1.5'
  segment:
    fontFamily: 'IBM Plex Sans'
    fontSize: 14px
    fontWeight: '400'
    lineHeight: '1.55'
  button:
    fontFamily: 'IBM Plex Sans'
    fontSize: 14px
    fontWeight: '500'
  label:
    fontFamily: 'IBM Plex Sans'
    fontSize: 13px
    fontWeight: '600'
  meta:
    fontFamily: 'IBM Plex Sans'
    fontSize: 13px
    fontWeight: '400'
  help:
    fontFamily: 'IBM Plex Sans'
    fontSize: 12px
    fontWeight: '400'
    lineHeight: '1.5'
  badge:
    fontFamily: 'IBM Plex Sans'
    fontSize: 11px
    fontWeight: '600'
  timestamp:
    fontFamily: 'IBM Plex Mono'
    fontSize: 12px
    fontWeight: '400'
    note: 'font-variant-numeric: tabular-nums — bắt buộc'
  mono-value:
    fontFamily: 'IBM Plex Mono'
    fontSize: 13px
    fontWeight: '400'
    note: 'Tên model, key, dung lượng, đồng hồ phiên; tabular-nums'
rounded:
  sm: 6px
  md: 8px
  lg: 10px
  xl: 12px
  2xl: 14px
  3xl: 16px
  full: 9999px
spacing:
  '1': 4px
  '2': 8px
  '3': 12px
  '4': 16px
  '5': 20px
  '6': 24px
  '8': 32px
  sidebar-width: 260px
  header-height: 64px
  panel-detail: 360px
  panel-live: 320px
  settings-nav: 220px
  settings-label-col: 220px
  transcript-gutter: 56px
  window-min: 1024px x 680px
  window-default: 1280px x 800px
components:
  button:
    height: 36px
    padding-x: 14px
    radius: '{rounded.md}'
    font: '{typography.button}'
    gap: '{spacing.2}'
    transition: 'background-color 150ms ease, color 150ms ease, border-color 150ms ease'
  button-primary:
    background: '{colors.accent}'
    foreground: '#FFFFFF'
    background-hover: '{colors.accent-hover}'
  button-secondary:
    background: '{colors.surface}'
    foreground: '{colors.text}'
    border: '1px solid {colors.border-strong}'
  button-ghost:
    background: 'transparent'
    foreground: '{colors.text-secondary}'
  button-danger:
    background: '{colors.danger}'
    foreground: '#FFFFFF'
  button-danger-soft:
    background: '{colors.surface}'
    foreground: '{colors.danger}'
    border: '1px solid {colors.danger-border}'
  button-lg:
    height: 44px
    padding-x: '{spacing.5}'
    radius: '{rounded.lg}'
  button-disabled:
    opacity: '0.45'
    cursor: 'not-allowed'
    note: 'Luôn kèm tooltip lý do + lối tắt; không bao giờ ẩn nút'
  input:
    height: 36px
    padding-x: '{spacing.3}'
    radius: '{rounded.md}'
    background: '{colors.surface}'
    border: '1px solid {colors.border-strong}'
  badge:
    height: 20px
    padding-x: 7px
    radius: '{rounded.sm}'
    font: '{typography.badge}'
  badge-memo:
    background: '{colors.memo-soft}'
    foreground: '{colors.memo}'
  badge-audio:
    background: '{colors.info-soft}'
    foreground: '{colors.info}'
  badge-partial:
    background: '{colors.warning-soft}'
    foreground: '{colors.warning}'
  badge-recover:
    background: '{colors.recover-soft}'
    foreground: '{colors.recover}'
  badge-live:
    background: '{colors.danger-soft}'
    foreground: '{colors.danger-strong}'
  badge-file:
    background: '{colors.surface-sunken}'
    foreground: '{colors.text-secondary}'
  badge-token:
    background: '{colors.token-soft}'
    foreground: '{colors.warning}'
    border: '1px solid {colors.warning-border}'
  status-pill:
    height: 30px
    padding-x: '{spacing.3}'
    radius: '{rounded.full}'
    font: '{typography.meta}'
    font-weight: '600'
  status-rec:
    background: '{colors.danger-soft}'
    foreground: '{colors.danger-strong}'
    note: 'Dot pulse 1.4s; tắt pulse khi prefers-reduced-motion'
  status-ok:
    background: '{colors.memo-soft}'
    foreground: '{colors.memo}'
  status-warn:
    background: '{colors.warning-soft}'
    foreground: '{colors.warning}'
  status-off:
    background: '{colors.surface-sunken}'
    foreground: '{colors.text-secondary}'
  transcript-segment:
    grid: '{spacing.transcript-gutter} 1fr'
    gap: '{spacing.3}'
    padding: '{spacing.2} {spacing.3}'
    radius: '{rounded.md}'
    background-hover: '#F1F2EE'
    background-active: '{colors.accent-soft}'
  transcript-timestamp:
    font: '{typography.timestamp}'
    foreground: '{colors.text-muted}'
  card:
    background: '{colors.surface}'
    border: '1px solid {colors.border}'
    radius: '{rounded.xl}'
    shadow: 'none'
  card-job:
    background: '{colors.warning-soft}'
    border: '1px solid {colors.warning-border}'
    radius: '{rounded.xl}'
  chip-tag:
    height: 26px
    radius: '{rounded.full}'
    background-selected: '{colors.accent-soft}'
    border-selected: '1px solid {colors.accent-border}'
  segmented-control:
    background: '{colors.surface-sunken}'
    radius: '{rounded.md}'
    item-selected-background: '{colors.surface}'
    item-selected-shadow: '0 1px 2px rgba(17,24,39,.08)'
  drop-zone:
    radius: '{rounded.2xl}'
    border: '1px dashed {colors.border-strong}'
    border-active: '1px dashed {colors.accent-border}'
    background-active: '{colors.accent-soft}'
  dialog:
    width: 480px
    radius: '{rounded.3xl}'
    background: '{colors.surface}'
    shadow: '0 10px 32px rgba(17,24,39,.12)'
    overlay: '{colors.overlay}'
  menu:
    radius: '{rounded.lg}'
    background: '{colors.surface}'
    shadow: '0 10px 32px rgba(17,24,39,.12)'
  ad-slot:
    width: 236px
    creative: '300x100 co theo chiều rộng, hoặc 320x50'
    background: '{colors.surface}'
    border: '1px solid {colors.border}'
    radius: '{rounded.lg}'
    note: 'Đáy sidebar 260px. Không render ở route Live ở mọi trạng thái.'
  banner:
    padding: '{spacing.3} {spacing.4}'
    radius: '{rounded.lg}'
    font: '{typography.body}'
    gap: '{spacing.3}'
    note: 'Icon + chữ + nút hành động trên một hàng; không bao giờ chỉ màu'
  banner-warning:
    background: '{colors.warning-soft}'
    foreground: '{colors.warning}'
    border: '1px solid {colors.warning-border}'
  banner-danger:
    background: '{colors.danger-soft}'
    foreground: '{colors.danger-strong}'
    border: '1px solid {colors.danger-border}'
  banner-info:
    background: '{colors.info-soft}'
    foreground: '{colors.info}'
    border: '1px solid {colors.border}'
  toast:
    width: 360px
    padding: '{spacing.3} {spacing.4}'
    radius: '{rounded.lg}'
    background: '{colors.surface}'
    border: '1px solid {colors.border}'
    shadow: '0 10px 32px rgba(17,24,39,.12)'
    position: 'góc dưới phải vùng nội dung, cách mép {spacing.6}'
    note: 'Vào 200ms / ra 140ms; tự tắt 4s; chỉ cho việc nền đã xong'
  popover:
    width: 320px
    radius: '{rounded.xl}'
    background: '{colors.surface}'
    border: '1px solid {colors.border}'
    shadow: '0 10px 32px rgba(17,24,39,.12)'
    note: 'Tag picker dùng chung ở Trang chủ, Transcript detail, LiveSetup'
  player:
    height: 64px
    background: '{colors.surface}'
    border-top: '1px solid {colors.border}'
    play-button-size: 40px
    play-button-radius: '{rounded.full}'
    time-font: '{typography.mono-value}'
    seek-track: '{colors.surface-sunken}'
    seek-fill: '{colors.accent}'
  focus-ring:
    outline: '2px solid {colors.accent}'
    outline-offset: 2px
---

# trans-kun v3 — Design Spine

> Bản chuẩn hoá của `design-system/trans-kun/MASTER.md` theo quy ước [DESIGN.md](https://github.com/google-labs-code/design.md). Tài liệu này sở hữu **"trông như thế nào"**; `EXPERIENCE.md` sở hữu **"hành xử như thế nào"**. Khi mâu thuẫn với mockup, MASTER.md hay bất kỳ bản import nào, **spine thắng**.

## Brand & Style

trans-kun là công cụ chạy **cạnh Zoom/Teams trong lúc họp**, không phải app người dùng ngắm. Tư thế thẩm mỹ vì thế là **quiet utility**: chrome lùi hẳn về sau, transcript và bản dịch là nhân vật chính và chiếm **≥ 60 % chiều rộng cửa sổ**. Mọi thứ còn lại — sidebar, header, panel — chỉ tồn tại để đưa người dùng tới nội dung rồi biến mất khỏi sự chú ý.

Ba dial định hình bề mặt: **Variance 4/10** (hiện đại, cân bằng, không thử nghiệm), **Motion 3/10** (chuyển động chỉ để giải thích, không để gây ấn tượng), **Density 7/10** (khá dày, kiểu công cụ — người dùng quét danh sách 500 phiên, không đọc landing page).

Nền **ấm** (`{colors.bg}` `#F6F6F2`, saturation < 0.02) thay vì trắng lạnh: app mở suốt buổi họp 60–90 phút, mắt phải chịu được. **Flat tuyệt đối** — không gradient, không đổ bóng ngoài hai ngoại lệ ở *Elevation & Depth*. Một accent teal duy nhất (`{colors.accent}`) kế thừa từ v2 để người dùng cũ nhận ra sản phẩm.

Nguyên tắc biên tập bao trùm, kéo thẳng từ NFR-9 và FR-4: **trạng thái trung thực**. Mọi thứ có thể sai — mạng, key, model, khoảng thiếu, quyền hệ thống — đều có chỉ báo riêng, có **chữ + icon** (không bao giờ chỉ màu), và có hành động đi kèm. Bề mặt này không có màn trắng, không có dialog lặp, không có nút biến mất.

## Colors

Bảng màu chia làm ba lớp: **nền trung tính ấm**, **một accent teal**, và **bốn màu ngữ nghĩa** cho trạng thái. Không có màu thứ năm nào được thêm mà không bỏ một màu đang có.

**Nền và chữ.** `{colors.bg}` là nền app; `{colors.bg-sidebar}` (`#F0F0EB`) tối hơn một bậc để tách cột trái mà không cần viền nặng; `{colors.surface}` trắng thuần cho card, list và editor — nơi nội dung sống; `{colors.surface-sunken}` cho những thứ *chìm xuống* nền: nền segmented control, nền chip, dòng "mất kết nối" trong luồng segment. Chữ có ba cấp và chỉ ba cấp: `{colors.text}` cho nội dung, `{colors.text-secondary}` cho label và nav, `{colors.text-muted}` cho help text và timestamp (5.9:1 trên trắng — vẫn là AA cho body).

**Accent teal `{colors.accent}` (`#0F766E`)** mang đúng một nghĩa: *hành động chính, hoặc thứ đang được chọn*. Nó xuất hiện ở nút primary, nav item đang active, nhãn speaker, segment đang phát (`{colors.accent-soft}`), chip tag đang lọc, và focus ring. Nó **không** dùng để trang trí, không dùng cho trạng thái thành công (đã có `status-ok`), không dùng làm màu nền lớn. 5.0:1 trên trắng và trắng-trên-accent cũng 5.0:1 — dùng được cả hai chiều.

**Bốn màu ngữ nghĩa** mỗi màu một câu chuyện, không hoán đổi:

- **`{colors.danger}` đỏ** — dừng và xoá. Nút Dừng ở Live, nút xoá phiên, badge LIVE (biến thể đậm `{colors.danger-strong}` để đạt tương phản trên nền `{colors.danger-soft}`).
- **`{colors.warning}` hổ phách** — *chưa xong, hoặc sẽ tốn tiền*. Transcript partial, đang nối lại, job đang chạy, và badge "Tốn token Gemini". Gộp hai nghĩa này vào một màu là cố ý: cả hai đều là "khoan, đọc kỹ trước khi đi tiếp".
- **`{colors.info}` xanh dương** — badge "Audio", thông tin trung tính không đòi hành động.
- **`{colors.recover}` tím** — chỉ dành cho badge "Phục hồi" (phiên mồ côi sau force-quit, FR-24). Màu riêng vì đây là trạng thái người dùng chưa từng chủ động tạo ra; nó cần khác hẳn "partial".

**`{colors.mark}` vàng** chỉ dùng cho highlight kết quả tìm kiếm — không bao giờ cho gì khác.

**Dark mode nằm trong bản submit đầu tiên** (chốt 2026-09-18). Bảng dark được **thiết kế song song, không đảo màu** từ light: nền `{colors.bg-dark}` `#141517` và các bậc surface đi lên chứ không đi xuống. Accent tách làm hai vai: `{colors.accent-dark}` `#2DD4BF` cho **chữ và icon** (teal `#0F766E` quá tối để đọc trên nền tối), còn **nút primary giữ nền `#0F766E` chữ trắng** vì mảng màu đặc không cần độ sáng như chữ. Tương phản phải **đo riêng cho từng theme** — không suy ra từ theme kia.

Các token dark gắn `[ASSUMPTION]` trong frontmatter là chỗ MASTER.md chưa nêu giá trị; chúng được suy ra theo cùng logic (soft = accent/semantic ở alpha 10–14 %) và **cần duyệt trước khi code**.

## Typography

**IBM Plex Sans** (400/500/600/700) kế thừa từ v2, hỗ trợ đầy đủ dấu tiếng Việt. Font được **bundle trong app**, không tải từ Google Fonts lúc chạy — đây là ràng buộc privacy và sandbox (NFR-8, NFR-4), không phải lựa chọn thẩm mỹ. Chữ Nhật lấy từ font hệ thống qua fallback stack `-apple-system, "Segoe UI", "Hiragino Sans", "Yu Gothic UI", "Noto Sans JP"`.

**IBM Plex Mono** dùng cho mọi thứ người dùng cần *so hàng theo cột hoặc đọc chính xác từng ký tự*: timestamp, đồng hồ phiên, tên model, API key, dung lượng lưu trữ. Mọi số trong mono **bắt buộc** `font-variant-numeric: tabular-nums` — timestamp nhảy ngang khi cuộn transcript là lỗi.

Thang chữ có tám bậc và dừng ở đó: 11 (badge) · 12 (help, timestamp) · 13 (label, meta) · 14 (body, segment) · 15 (option title) · 16 (h2) · 18 (tiêu đề màn detail) · 22 (h1). Không có bậc nào lớn hơn 22 — app này không có hero.

Line-height tách riêng cho transcript: body 1.5, nhưng **segment transcript 1.55**. Nửa điểm chênh đó là vì transcript là văn bản người dùng đọc liên tục hàng chục phút, thường trộn ja/vi/en trong cùng một khối.

Weight: heading 600, label 600, body 400, nút 500, badge 600. Không dùng 700 ngoài nhãn speaker.

## Layout & Spacing

Thang spacing 4-based, bảy bậc: `{spacing.1}` 4px (gap trong badge) → `{spacing.8}` 32px (padding card onboarding và settings). Density 7/10 nghĩa là padding chuẩn là `{spacing.4}` 16px chứ không phải 24px.

**App shell.** Sidebar trái **`{spacing.sidebar-width}` 260px** cố định — logo, ba nav item (Trang chủ / Live / Cài đặt), card "job đang chạy" thu gọn, và ad slot ở đáy. Bề rộng 260px là **quyết định 2026-09-18 ghi đè con số 232px trong MASTER.md và trong `mockup/build.py`**: creative 300×100 co xuống ~236px trong sidebar 260px vẫn đọc được, còn ở 232px thì không. Cột main chiếm phần còn lại.

Cửa sổ **tối thiểu 1024×680, mặc định 1280×800**. Không có breakpoint theo nghĩa web — đây là một cửa sổ desktop resize được, không phải trang responsive. Quy tắc co: panel phụ đóng trước, sidebar giữ nguyên bề rộng, transcript nhận toàn bộ phần dư.

**Header màn** cao `{spacing.header-height}` 64px, nền `{colors.surface}`, viền dưới 1px: tiêu đề bên trái, hành động bên phải. Một CTA chính mỗi màn — không bao giờ hai nút primary cạnh nhau.

**Panel phụ** rộng `{spacing.panel-detail}` 360px ở Transcript detail (tab Memo | Ghi chú) và `{spacing.panel-live}` 320px ở Live (Ghi chú). Trạng thái đóng/mở nhớ riêng theo từng màn.

**Transcript** là grid `{spacing.transcript-gutter} 1fr` — 56px cho timestamp, phần còn lại cho text. Chế độ cạnh nhau (side-by-side) là hai cột đều nhau, mỗi cột giữ nguyên grid con 56px.

**Ad slot** ngồi ở đáy sidebar, rộng 236px. Vì nó nằm *trong* sidebar và sidebar ad ẩn theo route, nó **tự động biến mất ở Live** — cấu trúc layout thực thi quy tắc, không phụ thuộc lập trình viên nhớ ẩn nó.

## Elevation & Depth

Bề mặt này **phẳng**. Phân lớp thị giác đến từ **tông nền**, không từ bóng: `{colors.bg}` (app) → `{colors.bg-sidebar}` (cột trái) → `{colors.surface}` (nội dung) → `{colors.surface-sunken}` (thứ chìm xuống). Card phân biệt với nền bằng viền 1px `{colors.border}`, không bằng shadow.

Đúng **hai ngoại lệ** được phép có bóng:

1. **Nút đang chọn trong segmented control** — `0 1px 2px rgba(17,24,39,.08)`. Bóng gần như không thấy; nó chỉ để nút trắng nổi khỏi nền sunken.
2. **Menu và dialog** — `0 10px 32px rgba(17,24,39,.12)`. Đây là thứ thật sự nổi *lên trên* mặt phẳng app, và bóng là tín hiệu duy nhất nói điều đó.

Ngoài hai trường hợp này, `box-shadow` trong code frontend là lỗi review PR.

**Motion** ở mức 3/10, chỉ để giải thích thay đổi: hover/active `150ms ease` (màu, nền, viền); panel mở/đóng `200ms ease-out`; dot "đang ghi" nhấp nháy `1.4s`; caret streaming `1s steps(2)`; toast vào 200ms / ra 140ms. `prefers-reduced-motion` **tắt pulse và làm caret tĩnh** — hai chuyển động duy nhất chạy liên tục.

## Shapes

Bán kính tăng theo kích thước phần tử, để mọi thứ trông cùng một họ:

- **`{rounded.sm}` 6px** — badge. Nhỏ nhất, gần vuông nhất.
- **`{rounded.md}` 8px** — nút, input, segmented control, nav item, dòng segment. Đây là bán kính mặc định của app; nếu phân vân, dùng nó.
- **`{rounded.lg}` 10px** — card nhỏ, radio card option, nút lớn (`button-lg`), menu.
- **`{rounded.xl}` 12px** — card (danh sách phiên, card job, card onboarding).
- **`{rounded.2xl}` 14px** — drop-zone. Mềm hơn card một bậc vì nó là vùng *mời thả vào*, không phải vùng chứa nội dung.
- **`{rounded.3xl}` 16px** — dialog.
- **`{rounded.full}`** — chip tag và status pill. Pill là hình dạng dành riêng cho *trạng thái* và *nhãn phân loại*; không dùng pill cho nút hành động.

## Components

Đặc tả thị giác. Hành vi của từng thành phần nằm ở `EXPERIENCE.md.Component Patterns`.

**Nút.** Cao 36px, padding ngang 14px, radius `{rounded.md}`, weight 500, gap 8px giữa icon và chữ. Năm biến thể: `button-primary` (nền accent, chữ trắng), `button-secondary` (nền surface, viền `{colors.border-strong}`), `button-ghost` (trong suốt, chữ `{colors.text-secondary}`), `button-danger` (nền đỏ đặc — chỉ cho "Dừng" ở Live), `button-danger-soft` (nền surface, chữ đỏ, viền `{colors.danger-border}` — cho Xoá và Huỷ). Biến thể `button-lg` cao 44px radius 10px dành cho Onboarding và "Bắt đầu ghi".

Nút vô hiệu dùng `opacity .45` và **luôn kèm tooltip nêu lý do + lối tắt tới nơi sửa**. Nút cần Gemini khi chưa có key bị vô hiệu, **không bao giờ bị ẩn** (FR-4).

**Input.** Cao 36px, padding ngang 12px, viền `{colors.border-strong}` (đậm hơn viền card một bậc để trường nhập nổi rõ). Label 13px weight 600; trong Settings **mọi label có icon (?) tooltip**. Help text 12px `{colors.text-muted}` và **bền, không phải placeholder** — placeholder biến mất đúng lúc người dùng cần nó nhất.

Ô API key: `type=password` + nút hiện/ẩn + nút "Kiểm tra key"; kết quả render ngay dưới trong vùng `role=status`.

**Badge** cao 20px, radius 6px, 11px/600. Bảy badge, mỗi cái một cặp nền/chữ cố định: memo, audio, partial, recover, live, file, và **token** (`badge-token` — nền `{colors.token-soft}`, viền `{colors.warning-border}`, đứng cạnh nút "Sinh memo", "Transcribe lại", "Chạy lại" để nói trước rằng thao tác này tốn tiền của người dùng).

**Status pill** cao 30px, radius full, 13px/600. Bốn trạng thái: `status-rec` (đỏ, có dot pulse), `status-ok`, `status-warn`, `status-off`. Ở màn Live, **chỉ báo ghi âm và chỉ báo kết nối là hai pill riêng biệt, không bao giờ gộp** — Recording vẫn chạy khi mạng chết, và giao diện phải nói đúng điều đó (FR-22, FR-23).

**Dòng transcript (`transcript-segment`).** Grid 56px | 1fr, padding 8px 12px, radius 8px. Hover `#F1F2EE`; đang phát nền `{colors.accent-soft}`. Timestamp mono 12px muted, tabular. Nhãn speaker 11px/700 màu accent — **giữ trong data model nhưng ẩn ở UI v3** (chốt 2026-09-18: model hiện không phân biệt được speaker).

Khoảng thiếu và mất kết nối là **dòng đặc biệt nằm trong chính luồng segment**, không phải banner tách rời: nền `{colors.warning-soft}` cho "Thiếu mm:ss–mm:ss" (kèm nút "Chạy lại khoảng này"), nền `{colors.surface-sunken}` cho "Mất kết nối mm:ss–mm:ss". Đặt chúng đúng vị trí thời gian là cách duy nhất để người dùng thấy *cái gì bị thiếu ở đâu*.

**Card** nền surface, viền 1px, radius 12, không bóng. `card-job` (job đang chạy) dùng nền và viền warning để tách khỏi danh sách phiên tĩnh.

**Chip tag** cao 26px, pill; đang chọn = `{colors.accent-soft}` + viền `{colors.accent-border}` + icon x.

**Segmented control** nền sunken, radius 8; nút đang chọn nền trắng + bóng 1px.

**Dialog** rộng 480px, radius 16, overlay `{colors.overlay}`. Dialog chỉ được dùng cho **đúng năm việc**: xác nhận xoá phiên, xoá toàn bộ dữ liệu (hai bước), đóng app khi đang ghi, đóng app khi có job chạy, xoá tag toàn cục. Hành động nguy hiểm tách sang phải và dùng `button-danger-soft`.

**Ad slot** rộng 236px ở đáy sidebar: creative (ảnh + text tĩnh, ≤ 100 KB), nhãn "Sponsored", nút "Báo cáo quảng cáo" và link "Vì sao tôi thấy quảng cáo này". Nền surface, viền 1px, radius 10.

**Banner** là khuôn hiển thị lỗi và cảnh báo *inline, gần nơi xảy ra* — padding 12/16, radius 10, một hàng gồm icon + chữ + nút hành động. Ba biến thể theo ngữ nghĩa: `banner-warning` (partial, đang nối lại, chưa có key), `banner-danger` (server từ chối setup, thao tác bị chặn), `banner-info` (thông tin trung tính). Banner **luôn có icon và chữ**, không bao giờ chỉ dựa vào màu nền.

**Toast** rộng 360px ở góc dưới phải vùng nội dung, radius 10, có bóng menu. Nó chỉ dành cho **việc nền đã xong** (memo xong, export xong) và tự tắt sau 4 s. Lỗi cần hành động đi vào banner, không vào toast.

**Popover** rộng 320px, radius 12, bóng menu — dùng cho tag picker ở cả ba nơi gọi nó.

**Trình phát (`player`)** cao 64px, nền surface, viền trên 1px: nút play/pause **tròn 40px**, thời gian `HH:MM:SS / HH:MM:SS` bằng mono tabular, thanh seek (track sunken, fill accent), tốc độ và âm lượng.

**Focus ring** `2px solid {colors.accent}`, offset 2px — cùng một ring trên mọi control, cả light lẫn dark.

**Icon:** Lucide, stroke 1.75, 16–18px trong nút, 14px trong badge/meta. **Một bộ duy nhất, không emoji** — v2 dùng emoji ở toolbar và v3 bỏ hẳn.

### Tham chiếu thị giác

17 artboard tĩnh nằm ở [`design-system/trans-kun/mockup/project/`](../../../../design-system/trans-kun/mockup/project/), sinh từ `build.py`:

| Artboard | Minh hoạ |
|---|---|
| `OnboardingLang` · `OnboardingConsent` · `OnboardingKey` | Card 600px, stepper 3 bước, sơ đồ luồng dữ liệu ở Consent, ô key + kết quả `role=status` |
| `HomeEmpty` · `Home` | Hai card trạng thái trống; drop-zone mỏng, hàng chip tag, card job, danh sách phiên |
| `TagPicker` | Popover tag dùng chung cho ba nơi |
| `Transcript` | Trình phát, banner partial, segmented chọn bản, grid segment 56px, panel phải 360px |
| `LiveSetup` | Card 680px, 3 radio card nguồn, ghi chú quyền |
| `Live` | Toolbar 64px với hai pill riêng, hai cột Gốc/Dịch, panel ghi chú 320px |
| `LiveSaving` | Overlay "Đang lưu phiên…" trước khi điều hướng sang Transcript detail |
| `Settings` · `SettingsMemo` | Bảng 2 cột 220px, master-detail editor template memo |
| `Components` | Bảng màu, 4 trạng thái Live, bộ badge, nút, 6 mẫu lỗi, giải phẫu ad slot |
| `HomeDark` · `TranscriptDark` · `LiveDark` · `ComponentsDark` | Bốn màn chủ chốt ở dark mode |

Mockup là **tham chiếu bố cục, không phải hợp đồng**: hai spine thắng khi mâu thuẫn. Đã đồng bộ với spine ngày 2026-09-18 (sidebar 260px, artboard `LiveSaving`, 4 màn dark); sinh lại bằng `python3 design-system/trans-kun/mockup/build.py`.

## Do's and Don'ts

| Do | Don't |
|---|---|
| Phân lớp bằng tông nền (bg → sidebar → surface → sunken) | Dùng `box-shadow` ngoài segmented-on và menu/dialog |
| Dùng `{colors.accent}` cho *hành động chính / đang được chọn* | Dùng accent để trang trí, hoặc làm màu "thành công" |
| Trạng thái = màu **+ icon + chữ** | Phân biệt trạng thái chỉ bằng màu |
| Hai pill riêng cho "đang ghi" và "kết nối" ở Live | Gộp chúng, hoặc hiện "đang transcribe" khi không còn kết nối |
| Nút cần Gemini: vô hiệu + tooltip + lối tắt | Ẩn nút, hoặc mở dialog lặp đòi key |
| Badge "Tốn token Gemini" cạnh mọi nút gọi Gemini | Để người dùng phát hiện chi phí sau khi đã bấm |
| Khoảng thiếu là dòng trong luồng segment, đúng vị trí thời gian | Giấu khoảng thiếu, hoặc lưu transcript partial như hoàn chỉnh |
| Bundle IBM Plex trong app | Tải font/script từ mạng lúc chạy |
| Ad slot ở đáy sidebar 260px, ẩn theo route Live | Ad ở Live, ad che nội dung, ad có âm thanh, interstitial |
| Icon Lucide một bộ, luôn có `aria-label` khi icon-only | Emoji làm icon; icon-only button không nhãn |
| Đo tương phản riêng cho light và dark | Suy giá trị dark bằng cách đảo màu light |
| `tabular-nums` cho mọi số trong mono | Timestamp nhảy ngang khi cuộn |
| Dialog cho đúng 5 việc đã liệt kê | Dialog cho xác nhận thường ngày |
| `min-width` cho nút/badge để chứa ja/vi | `width` cố định — chữ Nhật và tiếng Việt sẽ bị cắt |
