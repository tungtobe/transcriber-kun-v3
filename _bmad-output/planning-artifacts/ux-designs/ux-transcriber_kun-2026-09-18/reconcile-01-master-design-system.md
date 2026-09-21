# Đối chiếu — `design-system/trans-kun/MASTER.md` → `DESIGN.md`

**Ngày:** 2026-09-18 · **Kết luận:** giữ gần như trọn vẹn; một override có chủ ý, mười sáu token bổ sung, bốn component được đặc tả mới.

## Giữ nguyên

| Nhóm | Ghi chú |
|---|---|
| Bảng màu light (18 vai trò) | Chuyển sang token kebab-case theo quy ước DESIGN.md; giá trị hex **không đổi** |
| Typography | IBM Plex Sans/Mono, thang 8 bậc, line-height body 1.5 / segment 1.55, weight |
| Spacing | 7 bậc 4→32px, density 7/10 |
| Radius | 6/8/10/12/14/999 → `sm/md/lg/xl/2xl/full` (+ `3xl` 16px cho dialog, vốn đã nêu trong mục Modal) |
| Shadow | Đúng hai ngoại lệ: `seg-btn-on` và menu/dialog |
| Motion | 150ms hover · 200ms panel · 1.4s pulse · 1s caret · toast 200/140; `prefers-reduced-motion` |
| Component specs | Buttons (5 biến thể + `btn-lg` + disabled), Inputs, Badges (7), Status pills (4), Segment, Cards, Chips, Segmented, Modal |
| Anti-Patterns | Chuyển nguyên vẹn vào bảng Do's and Don'ts |
| Style Guidelines | Flat, Lucide stroke 1.75, không emoji, nội dung ≥ 60 % chiều rộng |

## Thay đổi có chủ ý

| Điểm | MASTER.md | DESIGN.md | Lý do |
|---|---|---|---|
| **Sidebar** | 232px | **260px** | Chốt 2026-09-18: creative 300×100 co còn ~236px mới đọc được. **ĐÃ cập nhật MASTER.md và `mockup/build.py` ngày 2026-09-18.** |
| **Ad slot** | "đáy sidebar, 300×100 hoặc 320×50 co theo chiều rộng" | Chốt hẳn: đáy sidebar 260px, slot rộng 236px | Đóng Open Question 1 của doc 06 |
| **Dark mode** | Bảng token song song, không nói có vào MVP hay không | **Bắt buộc trong bản submit đầu** | Chốt 2026-09-18 |

## Bổ sung

- **16 token dark** MASTER.md chưa nêu giá trị, tất cả **đã được chủ sản phẩm duyệt 2026-09-18**: `border-strong-dark`, `text-secondary-dark`, `accent-hover-dark`, `accent-border-dark`, `danger-soft-dark`, `danger-strong-dark`, `danger-border-dark`, `warning-soft-dark`, `warning-border-dark`, `info-soft-dark`, `recover-dark`, `recover-soft-dark`, `memo-dark`, `memo-soft-dark`, `token-soft-dark`, `mark-dark`, `overlay-dark`. Khi đo thực tế, `mark-dark` bị chỉnh từ `#8A6D1F` sang **`#7A6019`** vì giá trị cũ trượt AA (4.18:1).
- **4 họ component chưa từng được đặc tả** nhưng đã dùng khắp nơi trong doc 06: `banner` (+3 biến thể), `toast`, `popover`, `player`.
- Token layout được nâng thành token thật: `sidebar-width`, `header-height`, `panel-detail`, `panel-live`, `settings-nav`, `transcript-gutter`, `window-min`, `window-default`.

## Ý đã chuyển chỗ (không mất)

- **Pre-Delivery Checklist** của MASTER.md bị tách theo đúng quyền sở hữu: mục thị giác (tương phản, focus ring) → `DESIGN.md.Do's and Don'ts`; mục hành vi (phím tắt, virtual list 500 dòng, i18n key parity ở CI, reduced-motion) → `EXPERIENCE.md` (Accessibility Floor, Component Patterns, Đa ngôn ngữ).
- **Quy tắc "Ở Live, chỉ báo ghi âm và kết nối là hai pill riêng"** xuất hiện ở cả hai spine — thị giác ở `DESIGN.md.Components`, hành vi ở `EXPERIENCE.md` (Component Patterns + Độ bền phiên). Lặp có chủ ý vì đây là quy tắc dễ bị vi phạm nhất.

## Cần theo dõi

1. ~~Header "LOGIC" của MASTER.md~~ — **ĐÃ SỬA 2026-09-18.** Thay bằng "CHUỖI ƯU TIÊN": `DESIGN.md` + `EXPERIENCE.md` → `pages/<page>.md` (hiện chưa có file nào) → `MASTER.md`. Master nay đóng vai bảng tra token khi code.
2. **Link mockup trong MASTER.md** trỏ tới artifact claude.ai. Spine không phụ thuộc link ngoài; bản tĩnh trong repo là nguồn tham chiếu.
