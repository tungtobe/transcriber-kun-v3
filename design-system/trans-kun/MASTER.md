# Design System Master File — trans-kun v3

> **CHUỖI ƯU TIÊN (cao → thấp):**
> 1. `_bmad-output/planning-artifacts/ux-designs/ux-transcriber_kun-2026-09-18/DESIGN.md` + `EXPERIENCE.md` — **hai spine, thắng mọi mâu thuẫn**.
> 2. `design-system/trans-kun/pages/<page>.md` — quy tắc riêng cho một màn, nếu file tồn tại *(hiện chưa có file nào)*.
> 3. File Master này.
>
> Master giữ vai trò bảng tra token/component khi code. Khi Master và spine lệch nhau, **sửa Master theo spine**.

---

**Project:** trans-kun (rebuild v3 của Transcriber-kun)
**Generated:** 2026-09-18 (script `ui-ux-pro-max --design-system`, sau đó hiệu chỉnh tay theo PRD v3 và token v2)
**Category:** Desktop productivity tool (Tauri 2, macOS ≥ 14.4 + Windows 10/11, phân phối store)
**Design dials:** Variance 4/10 (cân bằng, hiện đại) · Motion 3/10 (tinh tế) · Density 7/10 (khá dày, kiểu công cụ)
**Hướng thiết kế:** *Quiet utility* — flat, không đổ bóng, nền ấm nhẹ, một accent teal kế thừa từ v2, ưu tiên transcript/bản dịch là nội dung chính.
**Mockup:** bản tĩnh `mockup/project/` — **17 artboard** (12 light + `LiveSaving` + 4 dark). Sinh lại bằng `python3 mockup/build.py`.

---

## Global Rules

### Color Palette (light)

| Role | Hex | CSS Variable | Ghi chú |
|------|-----|--------------|---------|
| Background app | `#F6F6F2` | `--bg` | Trắng ấm, saturation < 0.02 |
| Sidebar | `#F0F0EB` | `--bg-sidebar` | |
| Surface (card, list, editor) | `#FFFFFF` | `--surface` | |
| Surface sunken | `#ECEDE8` | `--surface-sunken` | Segmented control, chip nền |
| Border | `#E1E4E8` | `--border` | |
| Border strong (input) | `#CFD4DA` | `--border-strong` | |
| Text | `#171A1F` | `--text` | |
| Text secondary | `#3C4551` | `--text-secondary` | Label, nav |
| Text muted | `#5B6470` | `--text-muted` | Help text, timestamp — 5.9:1 trên trắng |
| Accent (teal) | `#0F766E` | `--accent` | Kế thừa v2; 5.0:1 trên trắng, trắng trên accent 5.0:1 |
| Accent hover | `#0B5D57` | `--accent-hover` | |
| Accent soft | `#E6F3F1` | `--accent-soft` | Active/selected, segment đang phát |
| Accent border | `#B7DDD8` | `--accent-border` | Drop-zone, chip đang chọn |
| Danger | `#B42318` | `--danger` | Dừng, xoá, LIVE badge chữ `#A11E1E` nền `#FDE8E8` |
| Warning | `#8A4B0A` | `--warning` | Partial, đang nối lại, tốn token; nền `#FDF0DC`, viền `#F5D9A8` |
| Info | `#1E4E9B` | `--info` | Badge "Audio"; nền `#E8EEF7` |
| Recover | `#5B2FA3` | `--recover` | Badge "Phục hồi"; nền `#F0EAFB` |
| Highlight tìm kiếm | `#FDE68A` | `--mark` | |
| Focus ring | `#0F766E` 2px, offset 2px | `--focus-ring` | |

### Color Palette (dark) — thiết kế song song, không đảo màu

| Role | Hex |
|------|-----|
| `--bg` | `#141517` |
| `--bg-sidebar` | `#1A1B1E` |
| `--surface` | `#1F2024` |
| `--surface-sunken` | `#2A2B30` |
| `--border` | `#2E3036` |
| `--text` | `#ECEDEF` |
| `--text-muted` | `#A0A6B0` (≥ 4.5:1 trên surface) |
| `--accent` | `#2DD4BF` cho chữ/icon; nút primary nền `#0F766E` chữ trắng |
| `--accent-soft` | `rgba(45,212,191,.14)` |
| `--accent-border` | `rgba(45,212,191,.38)` |
| `--text-secondary` | `#C2C7CF` |
| `--border-strong` | `#3C3F47` |
| Warning/Danger/Info | Dùng biến thể sáng hơn (`#F5B75C`, `#F87171`, `#7AA7F0`) trên nền tối, kiểm tra tương phản riêng |
| `--mark` (highlight tìm kiếm) | `#7A6019` — chữ `#ECEDEF` trên nền này đạt 5.10:1 |

**Dark mode nằm trong bản submit store đầu tiên** (chốt 2026-09-18). Áp dụng qua `prefers-color-scheme` và `data-theme`, có ô ép theme trong Cài đặt → Chung; không có màu hex thô trong component. Bảng dark đầy đủ (kể cả các token Master này chưa nêu) nằm trong frontmatter của `DESIGN.md`. Tương phản **đo riêng từng theme**, không suy từ theme kia.

### Typography

- **Font:** IBM Plex Sans (400/500/600/700) — kế thừa v2, hỗ trợ tiếng Việt đầy đủ. **Bundle trong app**, không tải từ Google Fonts lúc chạy (NFR-8, privacy).
- **Fallback:** `-apple-system, "Segoe UI", "Hiragino Sans", "Yu Gothic UI", "Noto Sans JP", sans-serif` — chữ Nhật lấy từ font hệ thống.
- **Mono:** IBM Plex Mono cho timestamp, tên model, key, dung lượng; luôn `font-variant-numeric: tabular-nums`.
- **Scale:** 11 (badge) · 12 (help, timestamp) · 13 (label, meta) · 14 (body, segment) · 15 (option title) · 16 (h2) · 18 (title màn detail) · 22 (h1).
- **Line-height:** body 1.5; segment transcript 1.55; help 1.5.
- **Weight:** heading 600, label 600, body 400, nút 500, badge 600.

### Spacing (density 7/10)

| Token | Value | Usage |
|-------|-------|-------|
| `--space-1` | 4px | Gap trong badge/chip |
| `--space-2` | 8px | Gap giữa nút, icon-text |
| `--space-3` | 12px | Padding card nhỏ, sidebar |
| `--space-4` | 16px | Padding chuẩn, gap section |
| `--space-5` | 20px | Padding header ngang |
| `--space-6` | 24px | Padding nội dung chính |
| `--space-8` | 32px | Padding card onboarding, settings |

### Radius, Shadow, Motion

- Radius: 6 (badge) · 8 (nút, input, segment, nav item) · 10 (card nhỏ, option) · 12 (card) · 14 (drop-zone) · 999 (chip, status pill).
- **Không đổ bóng** ngoài `seg-btn-on` (`0 1px 2px rgba(17,24,39,.08)`) và menu/dialog (`0 10px 32px rgba(17,24,39,.12)`).
- Motion: hover/active `150ms ease` (màu, nền, viền); panel mở/đóng `200ms ease-out`; chỉ báo ghi âm nhấp nháy `1.4s`; caret streaming `1s steps(2)`; toast vào 200ms / ra 140ms. Tôn trọng `prefers-reduced-motion` (tắt pulse, caret tĩnh).

---

## Layout

- **App shell:** sidebar trái **260px** (logo, Trang chủ / Live / Cài đặt, card "job đang chạy", ad slot ở đáy) + main column. *(260px chốt 2026-09-18, thay cho 232px: creative 300×100 co còn ~236px mới đọc được.)* Cửa sổ tối thiểu 1024×680; mặc định 1280×800.
- **Header màn:** cao 64px, nền `--surface`, viền dưới; tiêu đề trái, hành động phải.
- **Ad slot:** đáy sidebar, vùng rộng **236px**; creative 300×100 hoặc 320×50 co theo chiều rộng; **ẩn toàn bộ sidebar ad khi route là Live** (kể cả sau khi dừng).
- **Panel phụ (Memo/Ghi chú):** 360px ở Transcript detail, 320px ở Live; đóng/mở nhớ theo màn.
- **Transcript:** grid `56px 1fr` (timestamp | text); side-by-side là grid 2 cột đều nhau.

---

## Component Specs

### Buttons

```css
.btn{display:inline-flex;align-items:center;gap:8px;height:36px;padding:0 14px;border-radius:8px;font-weight:500;border:1px solid transparent;transition:background-color 150ms ease,color 150ms ease,border-color 150ms ease}
.btn-primary{background:var(--accent);color:#fff}      .btn-primary:hover{background:var(--accent-hover)}
.btn-secondary{background:var(--surface);color:var(--text);border-color:var(--border-strong)}
.btn-ghost{background:transparent;color:var(--text-secondary)}
.btn-danger{background:var(--danger);color:#fff}       /* Dừng live */
.btn-danger-soft{background:var(--surface);color:var(--danger);border-color:#F0B8B3}  /* Xoá, Huỷ */
.btn-lg{height:44px;padding:0 20px;border-radius:10px} /* Onboarding, Bắt đầu ghi */
.btn[aria-disabled=true]{opacity:.45;cursor:not-allowed} /* Cần key: kèm tooltip + lối tắt Cài đặt */
.btn:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
```

Một CTA chính mỗi màn: Onboarding "Tiếp tục/Đồng ý/Vào ứng dụng"; Home "Live"; LiveSetup "Bắt đầu ghi"; Live "Dừng" (danger); Transcript "Sinh lại memo".

### Inputs

```css
.input,.select{height:36px;padding:0 12px;border:1px solid var(--border-strong);border-radius:8px;background:var(--surface)}
.label{font-size:13px;font-weight:600;color:var(--text-secondary)}  /* luôn có icon (?) tooltip trong Settings */
.help{font-size:12px;color:var(--text-muted)}                       /* helper text bền, không chỉ placeholder */
```

Key: `type=password` + nút hiện/ẩn + nút "Kiểm tra key"; kết quả hiển thị `role=status` ngay dưới, phân loại lỗi (key / mạng / CA / quota).

### Badges & Status pills

```css
.badge{height:20px;padding:0 7px;border-radius:6px;font-size:11px;font-weight:600}
.badge-memo{#E0F2F1/#0B5D57} .badge-audio{#E8EEF7/#1E4E9B} .badge-partial{#FDF0DC/#8A4B0A}
.badge-recover{#F0EAFB/#5B2FA3} .badge-live{#FDE8E8/#A11E1E} .badge-file{#ECEDE8/#3C4551}
.badge-token{#FFF7E6/#8A4B0A, viền #F5D9A8}  /* "Tốn token Gemini" cạnh Sinh memo, Transcribe lại */
.status{height:30px;padding:0 10px;border-radius:999px;font-size:13px;font-weight:600}
.status-rec{#FDE8E8/#A11E1E + dot pulse}  .status-ok{#E0F2F1/#0B5D57}  .status-warn{#FDF0DC/#8A4B0A}  .status-off{#ECEDE8/#3C4551}
```

Ở Live, **chỉ báo ghi âm và chỉ báo kết nối là hai pill riêng**, không bao giờ gộp.

### Segment (dòng transcript)

```css
.seg{display:grid;grid-template-columns:56px 1fr;gap:12px;padding:8px 12px;border-radius:8px;cursor:pointer}
.seg:hover{background:#F1F2EE} .seg-active{background:var(--accent-soft)}
.ts{font-family:mono;font-size:12px;color:var(--text-muted);tabular-nums}
.speaker{font-size:11px;font-weight:700;color:var(--accent)}
```

Khoảng thiếu / mất kết nối là một dòng đặc biệt trong luồng segment (nền warning hoặc sunken) kèm nút "Chạy lại khoảng này".

### Cards, Chips, Segmented

- Card: nền surface, viền 1px, radius 12, không bóng; card job đang chạy dùng viền/nền warning.
- Chip tag: cao 26px, pill; đang chọn = accent-soft + accent-border, có icon x.
- Segmented control: nền sunken, radius 8, nút chọn nền trắng + bóng 1px.

### Modal / Dialog

Chỉ cho: xác nhận xoá phiên, xoá toàn bộ dữ liệu (2 bước), đóng app khi đang ghi/đang chạy job, xoá tag toàn cục. Overlay `rgba(23,26,31,.45)`, card 480px radius 16, hành động nguy hiểm tách phải và dùng danger.

---

## Style Guidelines

**Style:** Flat Design, quiet utility.
**Icons:** Lucide (stroke 1.75, 16–18px trong nút, 14px trong badge/meta), một bộ duy nhất; **không emoji** (v2 dùng emoji ở toolbar — bỏ).
**Nội dung là chính:** transcript/bản dịch chiếm ≥ 60% chiều rộng; chrome tối giản.

---

## Anti-Patterns (Do NOT Use)

- ❌ Emoji làm icon, icon-only button không `aria-label`.
- ❌ Ad slot trong màn Live, che nội dung, có âm thanh, interstitial.
- ❌ Hiển thị "đang transcribe" khi không còn kết nối model; gộp trạng thái ghi âm với trạng thái kết nối.
- ❌ Màn trắng / dialog lặp khi chưa có key; nút cần Gemini biến mất thay vì vô hiệu có giải thích.
- ❌ Stack trace, key, URL trong thông báo lỗi; lỗi không kèm hành động.
- ❌ Giấu khoảng thiếu; lưu transcript partial như hoàn chỉnh.
- ❌ Tải font/script từ mạng lúc chạy; gradient nền, bóng nặng, card có viền trái màu.
- ❌ Chỉ dùng màu để phân biệt trạng thái (luôn có icon + chữ).

---

## Pre-Delivery Checklist

- [ ] Tương phản chữ ≥ 4.5:1 ở cả light và dark (đo riêng từng theme)
- [ ] Focus ring nhìn thấy trên mọi control; Tab order theo thứ tự thị giác
- [ ] Phím tắt: bắt đầu/dừng Live, tìm kiếm (⌘F), export, prev/next kết quả (Enter/Shift+Enter)
- [ ] `prefers-reduced-motion`: tắt pulse/caret
- [ ] Mọi setting có tooltip; giá trị sai bị chặn tại chỗ
- [ ] Không có creative ở Live; có nhãn Sponsored + Báo cáo + "Vì sao tôi thấy quảng cáo này" ở nơi khác
- [ ] Danh sách phiên 500 dòng dùng virtual list
- [ ] i18n vi/en/ja cùng tập key; chữ Nhật/Việt không bị cắt ở nút, badge
