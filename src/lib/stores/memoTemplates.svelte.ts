// Store domain `memoTemplates` (story 3.6, spec Approach): danh sách Template
// memo của nhóm Settings → Memo -- theo mẫu `createXStore()` (`library.svelte.ts`/
// `keys.svelte.ts`). `load(locale)` seed lười phía Rust (không seed ở đây) rồi
// trả đúng "2 mặc định của locale đó + mọi mẫu người dùng" theo thứ tự server
// đã sắp (spec Boundaries Always) -- store chỉ giữ nguyên thứ tự đó, không tự
// sắp lại.
import { commands, type AppError, type MemoTemplate } from '../bindings';

export type MemoTemplatesStatus = 'idle' | 'loading' | 'ready' | 'error';

const LOAD_UNAVAILABLE_ERROR: AppError = {
  category: 'network',
  code: 'network',
  detailRedacted: 'memo templates list unavailable',
};
const SAVE_UNAVAILABLE_ERROR: AppError = {
  category: 'network',
  code: 'network',
  detailRedacted: 'memo template save unavailable',
};
const DELETE_UNAVAILABLE_ERROR: AppError = {
  category: 'network',
  code: 'network',
  detailRedacted: 'memo template delete unavailable',
};
const RESTORE_UNAVAILABLE_ERROR: AppError = {
  category: 'network',
  code: 'network',
  detailRedacted: 'memo templates restore unavailable',
};

export type MemoTemplateResult =
  | { status: 'ok'; template: MemoTemplate }
  | { status: 'error'; error: AppError };

export type MemoTemplateVoidResult = { status: 'ok' } | { status: 'error'; error: AppError };

export function createMemoTemplatesStore() {
  let templates = $state<MemoTemplate[]>([]);
  let status = $state<MemoTemplatesStatus>('idle');
  let error = $state<AppError | null>(null);
  // Locale của lần `load` gần nhất đã ACK -- `create`/`update`/`delete` chèn/
  // sửa/xoá tại chỗ trên mảng này (không đổi thứ tự defaults/created_at),
  // `restoreDefaults` thay nguyên mảng vì Rust đã trả đúng danh sách mới của
  // locale này (spec Design Notes).
  let currentLocale: string | null = null;
  // Tăng mỗi lần `load` mới bắt đầu -- một `load` cũ bay về sau (đổi locale
  // liên tiếp nhanh) bị bỏ qua, cùng khuôn `keysStore.mutationGeneration`.
  let loadGeneration = 0;

  async function load(locale: string): Promise<void> {
    const generation = ++loadGeneration;
    status = 'loading';
    error = null;
    try {
      const result = await commands.memoTemplatesList(locale);
      if (generation !== loadGeneration) return;
      if (result.status === 'ok') {
        templates = result.data;
        currentLocale = locale;
        status = 'ready';
      } else {
        error = result.error;
        status = 'error';
      }
    } catch {
      if (generation !== loadGeneration) return;
      error = LOAD_UNAVAILABLE_ERROR;
      status = 'error';
    }
  }

  /** "+ Thêm mẫu" đã lưu -- mẫu mới luôn có `createdAt` lớn nhất nên nối vào
   * cuối giữ đúng thứ tự server (spec Boundaries Always: "mặc định trước ...
   * rồi mẫu người dùng theo `created_at`"). */
  async function create(name: string, prompt: string): Promise<MemoTemplateResult> {
    try {
      const result = await commands.memoTemplateCreate(name, prompt);
      if (result.status !== 'ok') {
        return { status: 'error', error: result.error };
      }
      templates = [...templates, result.data];
      return { status: 'ok', template: result.data };
    } catch {
      return { status: 'error', error: SAVE_UNAVAILABLE_ERROR };
    }
  }

  /** Lưu tên/prompt -- vị trí trong danh sách không đổi (sửa không chạm
   * `is_default`/`created_at`) nên cập nhật tại chỗ thay vì nạp lại. */
  async function update(id: string, name: string, prompt: string): Promise<MemoTemplateResult> {
    try {
      const result = await commands.memoTemplateUpdate(id, name, prompt);
      if (result.status !== 'ok') {
        return { status: 'error', error: result.error };
      }
      templates = templates.map((template) => (template.id === id ? result.data : template));
      return { status: 'ok', template: result.data };
    } catch {
      return { status: 'error', error: SAVE_UNAVAILABLE_ERROR };
    }
  }

  /** Xoá một mẫu người dùng -- Rust từ chối mẫu mặc định (`Code::Request`),
   * UI (editor) phải tự vô hiệu nút cho mẫu mặc định trước khi gọi tới đây. */
  async function remove(id: string): Promise<MemoTemplateVoidResult> {
    try {
      const result = await commands.memoTemplateDelete(id);
      if (result.status !== 'ok') {
        return { status: 'error', error: result.error };
      }
      templates = templates.filter((template) => template.id !== id);
      return { status: 'ok' };
    } catch {
      return { status: 'error', error: DELETE_UNAVAILABLE_ERROR };
    }
  }

  /** "Khôi phục mẫu mặc định" -- Rust trả nguyên danh sách mới của locale
   * hiện tại (2 mặc định về gốc + mẫu người dùng giữ nguyên), thay thẳng vào
   * `templates` thay vì tự suy luận lại thứ tự ở đây. */
  async function restoreDefaults(
    locale: string,
  ): Promise<{ status: 'ok'; templates: MemoTemplate[] } | { status: 'error'; error: AppError }> {
    try {
      const result = await commands.memoTemplatesRestoreDefaults(locale);
      if (result.status !== 'ok') {
        return { status: 'error', error: result.error };
      }
      if (locale === currentLocale) {
        templates = result.data;
      }
      return { status: 'ok', templates: result.data };
    } catch {
      return { status: 'error', error: RESTORE_UNAVAILABLE_ERROR };
    }
  }

  /** Test-only seam: xoá sạch state giữa các test case độc lập. */
  function reset(): void {
    loadGeneration += 1;
    templates = [];
    status = 'idle';
    error = null;
    currentLocale = null;
  }

  return {
    get templates() {
      return templates;
    },
    get status() {
      return status;
    },
    get error() {
      return error;
    },
    get currentLocale() {
      return currentLocale;
    },
    load,
    create,
    update,
    remove,
    restoreDefaults,
    reset,
  };
}

export const memoTemplatesStore = createMemoTemplatesStore();
