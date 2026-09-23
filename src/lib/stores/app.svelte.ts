// Store domain `app`: bọc `commands.appVersion()` (binding sinh tự động) và giữ
// trạng thái loading/ok/error để UI không bao giờ hiện màn trắng (AD-13).
// `error` mang đúng kiểu `AppError` (category/code/detailRedacted) sinh từ
// story 1.2 khi có; `null` khi lỗi không phải một `AppError` có cấu trúc
// (invoke ném exception thẳng, ví dụ IPC chưa sẵn sàng).
import { commands, events, type AppError } from '../bindings';

export type AppVersionState =
  | { status: 'loading' }
  | { status: 'ok'; version: string }
  | { status: 'error'; error: AppError | null };

function createAppStore() {
  let version = $state<AppVersionState>({ status: 'loading' });

  async function loadVersion() {
    version = { status: 'loading' };
    try {
      const result = await commands.appVersion();
      if (result.status === 'ok') {
        version = { status: 'ok', version: result.data };
      } else {
        version = { status: 'error', error: result.error };
      }
    } catch {
      version = { status: 'error', error: null };
    }
  }

  /**
   * Story 2.4: `CloseRequested` fires when the OS close was intercepted
   * because the Job registry is busy (spec Design Notes). `CloseConfirm`
   * calls this once on mount; the returned unlisten function must be called
   * on unmount.
   */
  function listenForCloseRequested(callback: () => void): Promise<() => void> {
    return events.closeRequested.listen(() => callback());
  }

  /** The user confirmed closing while Jobs were running — cancel everything
   * and exit (spec Design Notes: "đồng ý → huỷ sạch rồi thoát"). */
  async function confirmClose(): Promise<void> {
    await commands.appCloseConfirm();
  }

  return {
    get version() {
      return version;
    },
    loadVersion,
    listenForCloseRequested,
    confirmClose,
  };
}

export const appStore = createAppStore();
