// Store domain `memo` (story 3.7, spec Approach): trạng thái sinh/hiển thị
// memo theo cặp (Phiên, Template) -- theo mẫu `createXStore()`
// (`notes.svelte.ts`/`memoTemplates.svelte.ts`). Store là một singleton
// (`export const memoStore`) nên promise của `generate()` vẫn được xử lý dù
// `MemoPanel` đã unmount (đổi tab/rời Phiên) -- spec Boundaries Always: "store
// memo là singleton nên promise vẫn được xử lý sau khi unmount". Khi request
// hoàn tất mà cặp đó không còn "đang thấy" (`setVisible`), một toast ngắn
// thay cho việc chỉ âm thầm cập nhật state.
import { commands, type AppError, type MemoView } from '../bindings';
import { errorHint, errorTitle } from '../errors';
import { i18n } from '../../i18n/index.svelte';
import { toastStore } from './toast.svelte';

export type MemoStatus = 'idle' | 'loading' | 'generating' | 'ready';

export interface MemoEntryState {
  status: MemoStatus;
  memo: MemoView | null;
  /** Lỗi của lần `load`/`generate` gần nhất -- không xoá `memo` cũ khi có
   * lỗi (spec Boundaries Always: "lỗi/huỷ giữ memo cũ"). */
  error: AppError | null;
}

const EMPTY_ENTRY: MemoEntryState = { status: 'idle', memo: null, error: null };

function entryKey(sessionId: string, templateId: string, transcriptId = ''): string {
  return `${sessionId}:${templateId}:${transcriptId}`;
}

export function createMemoStore() {
  let entries = $state<Map<string, MemoEntryState>>(new Map());
  // Cặp (Phiên, Template) người dùng đang thực sự thấy panel Memo của nó --
  // `null` khi không có panel nào đang mở. Đặt bởi `MemoPanel`/`Session.svelte`
  // lúc mount/đổi tab/đổi Phiên/unmount.
  let visibleKey = $state<string | null>(null);

  function getEntry(sessionId: string, templateId: string, transcriptId = ''): MemoEntryState {
    return entries.get(entryKey(sessionId, templateId, transcriptId)) ?? EMPTY_ENTRY;
  }

  function setEntry(sessionId: string, templateId: string, transcriptId: string, next: MemoEntryState): void {
    const map = new Map(entries);
    map.set(entryKey(sessionId, templateId, transcriptId), next);
    entries = map;
  }

  /** Gọi khi panel Memo của một cặp (Phiên, Template) đang hiện trên màn
   * hình, hoặc `null`/`null` khi không panel nào đang hiện (rời tab/Phiên). */
  function setVisible(sessionId: string | null, templateId: string | null, transcriptId: string | null = null): void {
    visibleKey = sessionId && templateId ? entryKey(sessionId, templateId, transcriptId ?? '') : null;
  }

  function isVisible(sessionId: string, templateId: string, transcriptId: string): boolean {
    return visibleKey === entryKey(sessionId, templateId, transcriptId);
  }

  /** Đọc memo đã cache (spec Boundaries Always: "Mở lại memo chỉ đọc DB,
   * không gọi Gemini") -- gọi lúc `MemoPanel` mount hoặc đổi template. */
  async function load(sessionId: string, templateId: string, transcriptId = ''): Promise<void> {
    setEntry(sessionId, templateId, transcriptId, { ...getEntry(sessionId, templateId, transcriptId), status: 'loading', error: null });
    try {
      const result = await commands.memoGet(sessionId, templateId, transcriptId);
      if (result.status !== 'ok') {
        setEntry(sessionId, templateId, transcriptId, { status: 'idle', memo: null, error: result.error });
        return;
      }
      setEntry(sessionId, templateId, transcriptId, {
        status: result.data ? 'ready' : 'idle',
        memo: result.data,
        error: null,
      });
    } catch {
      setEntry(sessionId, templateId, transcriptId, { status: 'idle', memo: null, error: null });
    }
  }

  /** Sinh (hoặc sinh lại) một memo. Trạng thái hiển thị ngay `generating`
   * trước khi gọi lệnh (không chờ round-trip đầu tiên) -- nút "Huỷ" của
   * panel gọi [`cancel`] song song trong lúc promise này vẫn đang bay. */
  async function generate(sessionId: string, templateId: string, locale: string, transcriptId = ''): Promise<void> {
    const before = getEntry(sessionId, templateId, transcriptId);
    setEntry(sessionId, templateId, transcriptId, { ...before, status: 'generating', error: null });

    const fallback = (error: AppError | null): MemoEntryState => ({
      status: before.memo ? 'ready' : 'idle',
      memo: before.memo,
      error,
    });

    let result: Awaited<ReturnType<typeof commands.memoGenerate>>;
    try {
      result = await commands.memoGenerate(sessionId, templateId, transcriptId, locale);
    } catch {
      setEntry(sessionId, templateId, transcriptId, fallback(null));
      return;
    }

    if (result.status !== 'ok') {
      setEntry(sessionId, templateId, transcriptId, fallback(result.error));
      if (!isVisible(sessionId, templateId, transcriptId)) {
        toastStore.show(`${errorTitle(result.error)} — ${errorHint(result.error)}`, 'error');
      }
      return;
    }

    const outcome = result.data;
    if (outcome.kind === 'generated') {
      setEntry(sessionId, templateId, transcriptId, { status: 'ready', memo: outcome.memo, error: null });
      if (!isVisible(sessionId, templateId, transcriptId)) {
        toastStore.show(i18n.t('memoPanel.toast.ready'));
      }
      return;
    }
    // `cancelled` | `alreadyRunning` -- giữ nguyên memo cũ, không báo lỗi
    // (huỷ là một hành động chủ động, `alreadyRunning` nghĩa là một lần gọi
    // khác đang sở hữu request và sẽ tự cập nhật state khi xong).
    setEntry(sessionId, templateId, transcriptId, fallback(null));
  }

  /** Huỷ request đang chạy (nếu có) -- không lỗi khi không có gì đang chạy
   * (spec: `memo_cancel` idempotent). Không tự đổi `status` ở đây: promise
   * của [`generate`] đang bay sẽ tự cập nhật về `Cancelled` khi tỉnh dậy. */
  async function cancel(sessionId: string, templateId: string): Promise<void> {
    try {
      await commands.memoCancel(sessionId, templateId);
    } catch {
      // Best effort -- `generate()`'s own promise resolves the state either way.
    }
  }

  function view(sessionId: string, templateId: string, transcriptId = ''): MemoEntryState {
    return getEntry(sessionId, templateId, transcriptId);
  }

  /** Test-only seam. */
  function reset(): void {
    entries = new Map();
    visibleKey = null;
  }

  return {
    view,
    load,
    generate,
    cancel,
    setVisible,
    reset,
  };
}

export const memoStore = createMemoStore();
