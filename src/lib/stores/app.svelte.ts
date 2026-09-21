// Store domain `app`: bọc `commands.appVersion()` (binding sinh tự động) và giữ
// trạng thái loading/ok/error để UI không bao giờ hiện màn trắng (AD-13).
import { commands } from '../bindings';

export type AppVersionState =
  | { status: 'loading' }
  | { status: 'ok'; version: string }
  | { status: 'error' };

function createAppStore() {
  let version = $state<AppVersionState>({ status: 'loading' });

  async function loadVersion() {
    version = { status: 'loading' };
    try {
      const result = await commands.appVersion();
      if (result.status === 'ok') {
        version = { status: 'ok', version: result.data };
      } else {
        version = { status: 'error' };
      }
    } catch {
      version = { status: 'error' };
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
