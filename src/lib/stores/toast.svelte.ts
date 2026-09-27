// Toast host cấp app (story 3.7, spec Boundaries Always: "Khi request hoàn
// tất mà người dùng không còn thấy panel Memo của Phiên đó ... hiện toast
// ngắn 'Memo đã sẵn sàng' (hoặc lỗi) ở cấp app"). Không có component toast
// chung trước story này -- store nhỏ này + `ToastHost.svelte` (gắn ở
// `App.svelte`) là nguồn duy nhất, theo mẫu `createXStore()` (`notes.svelte.ts`).
// Hàng đợi (không phải một giá trị đơn) vì hai toast có thể tới gần nhau
// (hai memo sinh xong gần như cùng lúc ở hai Phiên khác nhau).
const DEFAULT_DURATION_MS = 4_000;

export type ToastVariant = 'info' | 'error';

export interface ToastItem {
  id: number;
  message: string;
  variant: ToastVariant;
}

export function createToastStore() {
  let items = $state<ToastItem[]>([]);
  let nextId = 0;
  const timers = new Map<number, ReturnType<typeof setTimeout>>();

  function dismiss(id: number): void {
    items = items.filter((item) => item.id !== id);
    const timer = timers.get(id);
    if (timer) {
      clearTimeout(timer);
      timers.delete(id);
    }
  }

  function show(message: string, variant: ToastVariant = 'info', durationMs = DEFAULT_DURATION_MS): number {
    const id = ++nextId;
    items = [...items, { id, message, variant }];
    timers.set(
      id,
      setTimeout(() => dismiss(id), durationMs),
    );
    return id;
  }

  /** Test-only seam: xoá sạch state, huỷ mọi timer đang chờ. */
  function reset(): void {
    for (const timer of timers.values()) clearTimeout(timer);
    timers.clear();
    items = [];
    nextId = 0;
  }

  return {
    get items() {
      return items;
    },
    show,
    dismiss,
    reset,
  };
}

export const toastStore = createToastStore();
