// Store domain `app`: bọc `commands.appVersion()` (binding sinh tự động) và giữ
// trạng thái loading/ok/error để UI không bao giờ hiện màn trắng (AD-13).
// `error` mang đúng kiểu `AppError` (category/code/detailRedacted) sinh từ
// story 1.2 khi có; `null` khi lỗi không phải một `AppError` có cấu trúc
// (invoke ném exception thẳng, ví dụ IPC chưa sẵn sàng).
import { commands, type AppError } from '../bindings';

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

  return {
    get version() {
      return version;
    },
    loadVersion,
  };
}

export const appStore = createAppStore();
