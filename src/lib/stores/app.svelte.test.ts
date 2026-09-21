import { describe, expect, it, vi, beforeEach } from 'vitest';

const appVersion = vi.fn();

vi.mock('../bindings', () => ({
  commands: {
    appVersion: (...args: unknown[]) => appVersion(...args),
  },
}));

describe('store app', () => {
  beforeEach(() => {
    vi.resetModules();
    appVersion.mockReset();
  });

  it('chuyển sang trạng thái ok khi binding trả version', async () => {
    appVersion.mockResolvedValue({ status: 'ok', data: '0.1.0' });
    const { appStore } = await import('./app.svelte');

    await appStore.loadVersion();

    expect(appStore.version).toEqual({ status: 'ok', version: '0.1.0' });
  });

  it('chuyển sang trạng thái error khi binding trả lỗi, không throw', async () => {
    appVersion.mockResolvedValue({ status: 'error', error: 'boom' });
    const { appStore } = await import('./app.svelte');

    await appStore.loadVersion();

    expect(appStore.version).toEqual({ status: 'error' });
  });

  it('chuyển sang trạng thái error khi invoke ném exception', async () => {
    appVersion.mockRejectedValue(new Error('ipc unavailable'));
    const { appStore } = await import('./app.svelte');

    await appStore.loadVersion();

    expect(appStore.version).toEqual({ status: 'error' });
  });
});
