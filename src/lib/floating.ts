// Neo một popover vào nút đã mở nó bằng `position: fixed` trên
// `document.body`. Cần cho popover mở từ dòng phiên ở Home: danh sách là
// virtual list `overflow-y: auto` dịch dòng bằng `transform`, nên một
// popover `position: absolute` bên trong dòng bị cắt ở mép danh sách (và
// `fixed` dưới tổ tiên có `transform` lại bám theo tổ tiên đó).
// Căn mép phải theo anchor; lật lên trên khi phía dưới không đủ chỗ.
const GAP = 4;
const MARGIN = 8;

export function placeFloating(node: HTMLElement, anchor: HTMLElement): void {
  const rect = anchor.getBoundingClientRect();
  const width = node.offsetWidth;
  const height = node.offsetHeight;
  const left = Math.min(
    Math.max(MARGIN, rect.right - width),
    Math.max(MARGIN, window.innerWidth - width - MARGIN),
  );
  let top = rect.bottom + GAP;
  if (top + height > window.innerHeight - MARGIN && rect.top - GAP - height >= MARGIN) {
    top = rect.top - GAP - height;
  }
  node.style.position = 'fixed';
  node.style.left = `${left}px`;
  node.style.top = `${top}px`;
  node.style.right = 'auto';
}

export function floating(node: HTMLElement, anchor: HTMLElement | null | undefined) {
  let current = anchor ?? null;
  if (!current) return {};
  document.body.appendChild(node);
  const update = () => {
    if (current) placeFloating(node, current);
  };
  update();
  window.addEventListener('scroll', update, true);
  window.addEventListener('resize', update);
  return {
    update(next: HTMLElement | null | undefined) {
      current = next ?? null;
      update();
    },
    destroy() {
      window.removeEventListener('scroll', update, true);
      window.removeEventListener('resize', update);
      node.remove();
    },
  };
}
