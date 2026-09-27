import { describe, expect, it, vi, beforeEach } from 'vitest';

const appVersion = vi.fn();
const appCloseConfirm = vi.fn();
const closeRequestedListen = vi.fn();

vi.mock('../bindings', () => ({
  commands: {
    appVersion: (...args: unknown[]) => appVersion(...args),
    appCloseConfirm: (...args: unknown[]) => appCloseConfirm(...args),
  },
  events: {
    closeRequested: {
      listen: (...args: unknown[]) => closeRequestedListen(...args),
    },
  },
}));

describe('store app', () => {
  beforeEach(() => {
    vi.resetModules();
    appVersion.mockReset();
    appCloseConfirm.mockReset();
    closeRequestedListen.mockReset();
  });

  it('chuyển sang trạng thái ok khi binding trả version', async () => {
    appVersion.mockResolvedValue({ status: 'ok', data: '0.1.0' });
    const { appStore } = await import('./app.svelte');

    await appStore.loadVersion();

    expect(appStore.version).toEqual({ status: 'ok', version: '0.1.0' });
  });

  it('chuyển sang trạng thái error kèm AppError typed khi binding trả lỗi, không throw', async () => {
    const appError = { category: 'storage', code: 'storage', detailRedacted: 'boom' };
    appVersion.mockResolvedValue({ status: 'error', error: appError });
    const { appStore } = await import('./app.svelte');

    await appStore.loadVersion();

    expect(appStore.version).toEqual({ status: 'error', error: appError });
  });

  it('chuyển sang trạng thái error với error null khi invoke ném exception', async () => {
    appVersion.mockRejectedValue(new Error('ipc unavailable'));
    const { appStore } = await import('./app.svelte');

    await appStore.loadVersion();

    expect(appStore.version).toEqual({ status: 'error', error: null });
  });

  it('listenForCloseRequested forwards the busy flag from the event payload', async () => {
    const unlisten = vi.fn();
    closeRequestedListen.mockImplementation((cb: (event: unknown) => void) => {
      cb({ event: 'close-requested', payload: { busy: true } } as never);
      return Promise.resolve(unlisten);
    });
    const { appStore } = await import('./app.svelte');
    const callback = vi.fn();

    const returned = await appStore.listenForCloseRequested(callback);

    expect(callback).toHaveBeenCalledTimes(1);
    expect(callback).toHaveBeenCalledWith(true);
    expect(returned).toBe(unlisten);
  });

  it('confirmClose calls appCloseConfirm', async () => {
    appCloseConfirm.mockResolvedValue({ status: 'ok', data: null });
    const { appStore } = await import('./app.svelte');

    await appStore.confirmClose();

    expect(appCloseConfirm).toHaveBeenCalledTimes(1);
  });
});
