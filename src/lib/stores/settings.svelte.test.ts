// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  settingsGet: vi.fn(),
  settingsSave: vi.fn(),
  listen: vi.fn(),
}));

vi.mock('../../lib/bindings', () => ({
  commands: {
    settingsGet: (...args: unknown[]) => mocks.settingsGet(...args),
    settingsSave: (...args: unknown[]) => mocks.settingsSave(...args),
  },
  events: {
    settingsChanged: {
      listen: (...args: unknown[]) => mocks.listen(...args),
    },
  },
}));

function installMatchMedia(initialDark = false) {
  let dark = initialDark;
  const listeners = new Set<(event: MediaQueryListEvent) => void>();
  const matchMedia = vi.fn((query: string) => ({
    media: query,
    get matches() {
      return dark;
    },
    onchange: null,
    addEventListener: (_type: string, listener: (event: MediaQueryListEvent) => void) => {
      listeners.add(listener);
    },
    removeEventListener: (_type: string, listener: (event: MediaQueryListEvent) => void) => {
      listeners.delete(listener);
    },
    addListener: undefined,
    removeListener: undefined,
    dispatchEvent: () => true,
  } as unknown as MediaQueryList));
  Object.defineProperty(window, 'matchMedia', { configurable: true, value: matchMedia });
  return {
    listenerCount() {
      return listeners.size;
    },
    setDark(next: boolean) {
      dark = next;
      for (const listener of listeners) {
        listener({ matches: next } as MediaQueryListEvent);
      }
    },
  };
}

describe('settingsStore', () => {
  beforeEach(() => {
    vi.resetModules();
    mocks.settingsGet.mockReset();
    mocks.settingsSave.mockReset();
    mocks.listen.mockReset().mockResolvedValue(() => undefined);
    localStorage.clear();
    document.documentElement.removeAttribute('data-theme');
    document.documentElement.removeAttribute('data-theme-preference');
    installMatchMedia(false);
  });

  it('loads persisted theme and applies it before the shell settles', async () => {
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: { theme: 'dark' } });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();

    await store.load();

    expect(store.theme).toBe('dark');
    expect(store.resolvedTheme).toBe('dark');
    expect(store.status).toBe('ready');
    expect(document.documentElement.dataset.theme).toBe('dark');
    expect(document.documentElement.dataset.themePreference).toBe('dark');
  });

  it('applies the cached theme synchronously while persisted settings are still loading', async () => {
    localStorage.setItem('trans-kun.theme', 'dark');
    let resolveLoad: ((value: { status: 'ok'; data: { theme: 'dark' } }) => void) | undefined;
    mocks.settingsGet.mockReturnValue(new Promise((resolve) => {
      resolveLoad = resolve;
    }));
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();

    expect(store.theme).toBe('dark');
    expect(document.documentElement.dataset.theme).toBe('dark');

    const pending = store.load();
    expect(store.status).toBe('loading');
    expect(document.documentElement.dataset.theme).toBe('dark');
    resolveLoad?.({ status: 'ok', data: { theme: 'dark' } });
    await pending;
  });

  it('tracks system scheme changes only when preference is system', async () => {
    const scheme = installMatchMedia(false);
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: { theme: 'system' } });
    mocks.settingsSave.mockResolvedValue({ status: 'ok', data: null });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();

    await store.load();
    expect(store.resolvedTheme).toBe('light');
    scheme.setDark(true);
    expect(store.resolvedTheme).toBe('dark');

    await store.setTheme('light');
    expect(mocks.settingsSave).toHaveBeenCalledWith({ theme: 'light' });
    scheme.setDark(false);
    expect(store.resolvedTheme).toBe('light');
    scheme.setDark(true);
    expect(store.resolvedTheme).toBe('light');
  });

  it('removes the system scheme listener on teardown', async () => {
    const scheme = installMatchMedia(false);
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: { theme: 'system' } });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();
    await store.load();
    const beforeDestroy = scheme.listenerCount();

    store.destroy();

    expect(scheme.listenerCount()).toBe(beforeDestroy - 1);
    scheme.setDark(true);
    expect(store.resolvedTheme).toBe('light');
  });

  it('optimistically applies a theme and rolls back on typed save failure', async () => {
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: { theme: 'light' } });
    mocks.settingsSave.mockResolvedValue({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'save failed' },
    });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();
    await store.load();

    const pending = store.setTheme('dark');
    expect(store.theme).toBe('dark');
    expect(store.resolvedTheme).toBe('dark');
    await pending;

    expect(store.theme).toBe('light');
    expect(store.resolvedTheme).toBe('light');
    expect(store.status).toBe('error');
    expect(store.error?.category).toBe('storage');
  });

  it('does not let a pending load overwrite a newer optimistic save', async () => {
    let resolveLoad: ((value: { status: 'ok'; data: { theme: 'light' } }) => void) | undefined;
    mocks.settingsGet.mockReturnValue(new Promise((resolve) => {
      resolveLoad = resolve;
    }));
    mocks.settingsSave.mockResolvedValue({ status: 'ok', data: null });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();

    const load = store.load();
    await store.setTheme('dark');
    resolveLoad?.({ status: 'ok', data: { theme: 'light' } });
    await load;

    expect(store.theme).toBe('dark');
    expect(store.resolvedTheme).toBe('dark');
  });

  it('keeps the shell usable with system theme when IPC is unavailable', async () => {
    localStorage.setItem('trans-kun.theme', 'dark');
    mocks.settingsGet.mockRejectedValue(new Error('IPC unavailable'));
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();

    await store.load();

    expect(store.theme).toBe('system');
    expect(store.resolvedTheme).toBe('light');
    expect(store.status).toBe('ready');
    expect(store.error).toBeNull();
    expect(document.documentElement.dataset.theme).toBe('light');
    expect(localStorage.getItem('trans-kun.theme')).toBe('system');
  });

  it('adopts backend settingsChanged events and removes listeners on teardown', async () => {
    let listener: ((event: { payload: { theme: string } }) => void) | undefined;
    const unlisten = vi.fn();
    mocks.listen.mockImplementation(async (callback) => {
      listener = callback;
      return unlisten;
    });
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: { theme: 'light' } });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();
    await store.load();

    listener?.({ payload: { theme: 'dark' } });
    expect(store.theme).toBe('dark');
    expect(document.documentElement.dataset.theme).toBe('dark');

    store.destroy();
    await Promise.resolve();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it('unlistens stale backend registrations that resolve after destroy and bootstrap', async () => {
    const pending: Array<(unlisten: () => void) => void> = [];
    mocks.listen.mockImplementation(() => new Promise((resolve) => pending.push(resolve)));
    const { createSettingsStore, settingsStore } = await import('./settings.svelte');
    await Promise.resolve();
    const singletonUnlisten = vi.fn();
    pending[0]?.(singletonUnlisten);
    await Promise.resolve();
    await Promise.resolve();
    settingsStore.destroy();
    pending.splice(0, pending.length);

    const store = createSettingsStore();
    await Promise.resolve();
    expect(pending).toHaveLength(1);

    store.destroy();
    store.bootstrap();
    await Promise.resolve();
    expect(pending).toHaveLength(2);

    const staleUnlisten = vi.fn();
    const activeUnlisten = vi.fn();
    pending[0](staleUnlisten);
    pending[1](activeUnlisten);
    await Promise.resolve();
    await Promise.resolve();

    expect(staleUnlisten).toHaveBeenCalledTimes(1);
    expect(activeUnlisten).not.toHaveBeenCalled();
    store.destroy();
    expect(activeUnlisten).toHaveBeenCalledTimes(1);
  });
});
