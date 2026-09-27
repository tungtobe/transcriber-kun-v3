---
title: '3.3 Tìm phiên theo tên kết hợp lọc tag'
type: 'feature'
created: '2026-09-27'
status: 'done'
baseline_commit: '6d3cc7b3bada8b85e9af82b3446336761bb052c9'
route: 'full'
route_source: 'auto'
review: 'none'
review_source: 'pinned'
lenses_ran: []
review_loop_iteration: 0
context: []
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** Có tag rồi nhưng vẫn chưa gõ tên để thu hẹp danh sách Home, và khi lọc ra rỗng/ra ít thì người dùng không thấy đang lọc gì hay cách bỏ lọc.

**Approach:** Thêm ô tìm theo tên (có nút xoá) ở header Home, ghép query vào hàm lọc thuần `filterSessions` sẵn có (AND với bộ lọc tag), thêm footer đếm "N / M phiên · lọc…" + "Xoá bộ lọc", trạng thái rỗng có nút "Xoá bộ lọc", và `⌘F`/`Ctrl+F` focus ô tìm.

## Boundaries & Constraints

**Always:** So khớp chuỗi con, không phân biệt hoa thường, bỏ khoảng trắng thừa: cả query và tên được `normalize('NFC')`, `toLowerCase()` (không phụ thuộc locale), trim và gộp khoảng trắng liên tiếp thành một; query rỗng sau chuẩn hoá = không lọc tên. Danh sách cập nhật khi gõ (không debounce dài; tính toán phải ≤ 200 ms với 500 phiên — có test đo). Query + tag là AND; xoá query (nút × hoặc Esc trong ô khi có query) giữ nguyên bộ lọc tag. Nút × chỉ hiện khi có query, có `aria-label`, xoá xong trả focus về ô. Query sống trong `libraryStore` cạnh `tagFilter` (không persist). Khi có Phiên nhưng kết quả lọc rỗng: trạng thái rỗng với thông báo và nút "Xoá bộ lọc" (xoá cả query lẫn tag). Footer dưới danh sách luôn hiển thị khi có ít nhất một Phiên: không lọc → "38 phiên"; đang lọc → "6 / 38 phiên · lọc…" (phần "lọc…" nêu ngắn gọn query và/hoặc số tag/"Chưa gắn tag") kèm nút "Xoá bộ lọc"; dùng `aria-live="polite"`. `⌘F` (Meta+F) và `Ctrl+F` focus + chọn nội dung ô tìm khi đang ở Home, đăng ký qua `registerKeymap` lúc mount và gỡ lúc destroy (mẫu `Session.svelte`). Chuỗi mới đủ vi/en/ja, số nhiều qua cơ chế i18n sẵn có.

**Never:** Không gọi IPC để tìm (lọc client-side); không tìm trong nội dung transcript/ghi chú; không persist query; không đổi hành vi lọc tag của 3.2; không cho `⌘F` ở Home chiếm phím khi route khác đang hiển thị.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Hoa thường + khoảng trắng | query `"  HỌP   sprint "`, tên `Họp sprint 12` | Khớp | — |
| Query + tag | query `họp`, tag `A` | Chỉ Phiên có `A` và tên chứa `họp` | — |
| Xoá query | đang có query + tag `A` | Query rỗng, tag `A` vẫn lọc | — |
| Không kết quả | query `zzz` | Trạng thái rỗng + "Xoá bộ lọc"; bấm → toàn bộ danh sách | — |
| Footer | 38 phiên, lọc ra 6 | `6 / 38 phiên · lọc…` + "Xoá bộ lọc" | — |
| Không lọc | 38 phiên | `38 phiên`, không có nút xoá | — |
| Hiệu năng | 500 phiên, query 1 ký tự | Lọc xong ≤ 200 ms | — |
| Phím tắt | Home, `Ctrl+F`/`⌘F` | Focus ô tìm | — |

</frozen-after-approval>

## Code Map

- `src/lib/session-filter.ts` -- hàm thuần `filterSessions(sessions, filter)` + `TagFilterState`/`EMPTY_TAG_FILTER`/`isTagFilterActive`; thêm `query` vào filter (ví dụ `SessionFilterState = TagFilterState & { query: string }`) và `normalizeSearchText`; test ở `session-filter.test.ts`.
- `src/lib/stores/library.svelte.ts` -- `tagFilter` ($state, dòng ~95), `filteredSessions` getter, `clearTagFilter` (~341); thêm `nameQuery`, `setNameQuery`, `clearNameQuery`, `clearAllFilters`; `filteredSessions` dùng cả hai.
- `src/routes/Home.svelte` -- header `.screen-header` (dòng ~119) chứa ô tìm; khu danh sách (~224–229) có `TagFilterBar`, thông báo `home.list.filterEmpty` (thêm nút "Xoá bộ lọc"), `SessionList`; thêm footer. Header tìm có thể tách `src/routes/home/SessionSearch.svelte`, footer `src/routes/home/SessionListFooter.svelte`.
- `src/lib/keymap.ts` -- `registerKeymap({ id, combo: 'Meta+F' | 'Control+F', handler })`; mẫu dùng ở `src/routes/Session.svelte` dòng ~108.
- `src/routes/Home.test.ts`, `src/routes/Home.integration.test.ts` -- mẫu test Home (mock `commands`, `sessionListViewportHeight`).
- `src/i18n/{vi,en,ja}.json` -- nhãn ô tìm, nút xoá query, footer, "Xoá bộ lọc", mô tả lọc.

## Tasks & Acceptance

**Execution:**
- [ ] `src/lib/session-filter.ts` (+test) -- chuẩn hoá query/tên, AND với tag, test hiệu năng 500 phiên.
- [ ] `src/lib/stores/library.svelte.ts` (+test) -- state query, clear riêng query và clear tất cả.
- [ ] `src/routes/home/SessionSearch.svelte`, `src/routes/home/SessionListFooter.svelte` (+test) -- ô tìm có nút ×, footer đếm + xoá bộ lọc.
- [ ] `src/routes/Home.svelte` (+test) -- gắn ô tìm, footer, trạng thái rỗng có nút, phím tắt `⌘F`/`Ctrl+F`.
- [ ] `src/i18n/{vi,en,ja}.json` -- chuỗi mới.

**Acceptance Criteria:**
- Given Home có danh sách, when gõ từng ký tự, then danh sách cập nhật ngay sau mỗi lần nhập, không cần Enter.
- Given đang ở route khác Home, when bấm `⌘F`/`Ctrl+F`, then Home không chiếm phím (entry đã gỡ khi rời Home).

## Verification

**Commands:**
- `npm test && npm run check && npm run check:i18n && npm run check:ui && npm run check:deps` -- pass
