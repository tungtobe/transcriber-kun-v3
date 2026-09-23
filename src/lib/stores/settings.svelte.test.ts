// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  settingsGet: vi.fn(),
  settingsSave: vi.fn(),
  listen: vi.fn(),
}));

function settings(
  theme: 'system' | 'light' | 'dark' = 'system',
  uiLanguage: 'system' | 'vi' | 'en' | 'ja' = 'system',
  onboardingCompleted = false,
  transcribeModel = 'gemini-flash-lite-latest',
  liveModel = 'gemini-3.5-live-translate-preview',
  memoModel = 'gemini-flash-lite-latest',
  chunkMinutes = 5,
  timestampOffsetSec = 0,
  transcribeLanguage: 'auto' | 'ja' | 'vi' | 'en' = 'auto',
) {
  return {
    theme,
    uiLanguage,
    onboardingCompleted,
    transcribeModel,
    liveModel,
    memoModel,
    chunkMinutes,
    timestampOffsetSec,
    transcribeLanguage,
  };
}

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
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: settings('dark') });
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
    let resolveLoad: ((value: { status: 'ok'; data: ReturnType<typeof settings> }) => void) | undefined;
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
    resolveLoad?.({ status: 'ok', data: settings('dark') });
    await pending;
  });

  it('tracks system scheme changes only when preference is system', async () => {
    const scheme = installMatchMedia(false);
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: settings() });
    mocks.settingsSave.mockResolvedValue({ status: 'ok', data: null });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();

    await store.load();
    expect(store.resolvedTheme).toBe('light');
    scheme.setDark(true);
    expect(store.resolvedTheme).toBe('dark');

    await store.setTheme('light');
    expect(mocks.settingsSave).toHaveBeenCalledWith(settings('light'));
    scheme.setDark(false);
    expect(store.resolvedTheme).toBe('light');
    scheme.setDark(true);
    expect(store.resolvedTheme).toBe('light');
  });

  it('removes the system scheme listener on teardown', async () => {
    const scheme = installMatchMedia(false);
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: settings() });
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
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: settings('light') });
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

  it('waits for an active load before saving the full loaded snapshot', async () => {
    let resolveLoad: ((value: { status: 'ok'; data: ReturnType<typeof settings> }) => void) | undefined;
    mocks.settingsGet.mockReturnValue(new Promise((resolve) => {
      resolveLoad = resolve;
    }));
    mocks.settingsSave.mockResolvedValue({ status: 'ok', data: null });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();

    const load = store.load();
    const save = store.setOnboardingCompleted(true);
    await Promise.resolve();
    expect(mocks.settingsSave).not.toHaveBeenCalled();
    resolveLoad?.({ status: 'ok', data: settings('light', 'ja', false) });
    await load;
    await save;

    expect(mocks.settingsSave).toHaveBeenCalledWith(settings('light', 'ja', true));
    expect(store.onboardingCompleted).toBe(true);
  });

  it('serializes full-snapshot saves and rolls the latest failure back to the latest success', async () => {
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: settings('light', 'en', false) });
    const backendListeners: Array<(event: { payload: ReturnType<typeof settings> }) => void> = [];
    mocks.listen.mockImplementation(async (callback) => {
      backendListeners.push(callback);
      return () => undefined;
    });
    let resolveFirst: ((value: { status: 'ok'; data: null }) => void) | undefined;
    mocks.settingsSave
      .mockReturnValueOnce(new Promise((resolve) => { resolveFirst = resolve; }))
      .mockResolvedValueOnce({
        status: 'error',
        error: { category: 'storage', code: 'storage', detailRedacted: 'save failed' },
      });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();
    await store.load();
    await Promise.resolve();

    const first = store.setOnboardingCompleted(true);
    const second = store.setUiLanguage('ja');
    expect(store.onboardingCompleted).toBe(true);
    expect(store.uiLanguage).toBe('ja');
    await Promise.resolve();
    expect(mocks.settingsSave).toHaveBeenCalledTimes(1);
    expect(mocks.settingsSave).toHaveBeenLastCalledWith(settings('light', 'en', true));

    backendListeners.at(-1)?.({ payload: settings('light', 'en', true) });
    expect(store.uiLanguage).toBe('ja');

    resolveFirst?.({ status: 'ok', data: null });
    await first;
    await second;

    expect(mocks.settingsSave).toHaveBeenCalledTimes(2);
    expect(mocks.settingsSave).toHaveBeenLastCalledWith(settings('light', 'ja', true));
    expect(store.uiLanguage).toBe('en');
    expect(store.onboardingCompleted).toBe(true);
    expect(store.error?.category).toBe('storage');
  });

  it('maps a rejected save transport to a typed storage error and rolls back', async () => {
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: settings('light', 'en', true) });
    mocks.settingsSave.mockRejectedValue(new Error('transport down'));
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();
    await store.load();

    await store.setOnboardingCompleted(false);

    expect(store.onboardingCompleted).toBe(true);
    expect(store.error).toEqual({
      category: 'storage',
      code: 'storage',
      detailRedacted: 'settings save unavailable',
    });
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
    let listener: ((event: { payload: ReturnType<typeof settings> }) => void) | undefined;
    const unlisten = vi.fn();
    mocks.listen.mockImplementation(async (callback) => {
      listener = callback;
      return unlisten;
    });
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: settings('light') });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();
    await store.load();

    listener?.({ payload: settings('dark', 'ja', true) });
    expect(store.theme).toBe('dark');
    expect(store.uiLanguage).toBe('ja');
    expect(store.onboardingCompleted).toBe(true);
    expect(document.documentElement.dataset.theme).toBe('dark');
    expect(document.documentElement.lang).toBe('ja');

    store.destroy();
    await Promise.resolve();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it('applies a language immediately, persists the full snapshot, and rolls back on error', async () => {
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: settings('dark', 'en') });
    mocks.settingsSave.mockResolvedValueOnce({ status: 'ok', data: null }).mockResolvedValueOnce({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'save failed' },
    });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();
    await store.load();

    const saved = store.setUiLanguage('ja');
    expect(store.uiLanguage).toBe('ja');
    expect(document.documentElement.lang).toBe('ja');
    await Promise.resolve();
    expect(mocks.settingsSave).toHaveBeenLastCalledWith(settings('dark', 'ja'));
    await saved;

    const failed = store.setUiLanguage('vi');
    expect(store.uiLanguage).toBe('vi');
    await failed;
    expect(store.uiLanguage).toBe('ja');
    expect(document.documentElement.lang).toBe('ja');
    expect(store.error?.category).toBe('storage');
  });

  it('falls back only malformed language and preserves valid theme/completion fields', async () => {
    mocks.settingsGet.mockResolvedValue({
      status: 'ok',
      data: { theme: 'dark', uiLanguage: 'xx', onboardingCompleted: true },
    });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();

    await store.load();

    expect(store.theme).toBe('dark');
    expect(store.uiLanguage).toBe('system');
    expect(store.onboardingCompleted).toBe(true);
  });

  it.each([
    [{ theme: 'light', uiLanguage: 'ja' }],
    [{ theme: 'light', uiLanguage: 'ja', onboardingCompleted: 'yes' }],
  ] as const)('defaults missing or corrupt onboarding completion without losing valid fields', async (data) => {
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();

    await store.load();

    expect(store.theme).toBe('light');
    expect(store.uiLanguage).toBe('ja');
    expect(store.onboardingCompleted).toBe(false);
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

describe('settingsStore.setModel', () => {
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

  it('optimistically persists a custom transcribe model name not in any loaded list', async () => {
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: settings() });
    mocks.settingsSave.mockResolvedValue({ status: 'ok', data: null });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();
    await store.load();

    await store.setModel('transcribe', 'my-custom-model');

    expect(store.transcribeModel).toBe('my-custom-model');
    expect(mocks.settingsSave).toHaveBeenCalledWith(settings('system', 'system', false, 'my-custom-model'));
  });

  it('persists the live and memo models independently of each other', async () => {
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: settings() });
    mocks.settingsSave.mockResolvedValue({ status: 'ok', data: null });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();
    await store.load();

    await store.setModel('live', 'my-live-model');
    await store.setModel('memo', 'my-memo-model');

    expect(store.liveModel).toBe('my-live-model');
    expect(store.memoModel).toBe('my-memo-model');
    expect(store.transcribeModel).toBe('gemini-flash-lite-latest');
    expect(mocks.settingsSave).toHaveBeenLastCalledWith(
      settings('system', 'system', false, 'gemini-flash-lite-latest', 'my-live-model', 'my-memo-model'),
    );
  });

  it('never calls settingsSave for an empty or whitespace-only model name', async () => {
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: settings() });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();
    await store.load();

    await store.setModel('transcribe', '   ');

    expect(mocks.settingsSave).not.toHaveBeenCalled();
    expect(store.transcribeModel).toBe('gemini-flash-lite-latest');
  });

  it('trims surrounding whitespace before saving', async () => {
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: settings() });
    mocks.settingsSave.mockResolvedValue({ status: 'ok', data: null });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();
    await store.load();

    await store.setModel('memo', '  spaced-model  ');

    expect(store.memoModel).toBe('spaced-model');
    expect(mocks.settingsSave).toHaveBeenCalledWith(
      settings('system', 'system', false, 'gemini-flash-lite-latest', 'gemini-3.5-live-translate-preview', 'spaced-model'),
    );
  });

  it('rolls back to the last persisted model name on a typed save failure', async () => {
    mocks.settingsGet.mockResolvedValue({
      status: 'ok',
      data: settings('system', 'system', false, 'original-model'),
    });
    mocks.settingsSave.mockResolvedValue({
      status: 'error',
      error: { category: 'format', code: 'format', detailRedacted: 'rejected' },
    });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();
    await store.load();

    const pending = store.setModel('transcribe', 'attempted-model');
    expect(store.transcribeModel).toBe('attempted-model');
    await pending;

    expect(store.transcribeModel).toBe('original-model');
    expect(store.status).toBe('error');
    expect(store.error?.category).toBe('format');
  });

  it('falls back a missing or blank model field to the shared Gemini default on load', async () => {
    mocks.settingsGet.mockResolvedValue({
      status: 'ok',
      data: { theme: 'system', uiLanguage: 'system', onboardingCompleted: false, liveModel: '   ' },
    });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();

    await store.load();

    expect(store.transcribeModel).toBe('gemini-flash-lite-latest');
    expect(store.liveModel).toBe('gemini-3.5-live-translate-preview');
    expect(store.memoModel).toBe('gemini-flash-lite-latest');
  });
});

describe('settingsStore chunking/offset/language setters', () => {
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

  it('optimistically persists a valid chunkMinutes and never calls settingsSave for an invalid one', async () => {
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: settings() });
    mocks.settingsSave.mockResolvedValue({ status: 'ok', data: null });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();
    await store.load();

    await store.setChunkMinutes(3);
    expect(store.chunkMinutes).toBe(3);
    expect(mocks.settingsSave).toHaveBeenCalledWith(settings('system', 'system', false, undefined, undefined, undefined, 3));

    mocks.settingsSave.mockClear();
    for (const invalid of [0, -2, 1.5, Number.NaN]) {
      await store.setChunkMinutes(invalid);
    }
    expect(mocks.settingsSave).not.toHaveBeenCalled();
    expect(store.chunkMinutes).toBe(3);
  });

  it('optimistically persists a valid timestampOffsetSec and never calls settingsSave for an invalid one', async () => {
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: settings() });
    mocks.settingsSave.mockResolvedValue({ status: 'ok', data: null });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();
    await store.load();

    await store.setTimestampOffsetSec(3_600);
    expect(store.timestampOffsetSec).toBe(3_600);
    expect(mocks.settingsSave).toHaveBeenCalledWith(
      settings('system', 'system', false, undefined, undefined, undefined, undefined, 3_600),
    );

    mocks.settingsSave.mockClear();
    for (const invalid of [-1, 2.5, Number.NaN]) {
      await store.setTimestampOffsetSec(invalid);
    }
    expect(mocks.settingsSave).not.toHaveBeenCalled();
    expect(store.timestampOffsetSec).toBe(3_600);
  });

  it('persists transcribeLanguage and rejects an unknown value', async () => {
    mocks.settingsGet.mockResolvedValue({ status: 'ok', data: settings() });
    mocks.settingsSave.mockResolvedValue({ status: 'ok', data: null });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();
    await store.load();

    await store.setTranscribeLanguage('ja');
    expect(store.transcribeLanguage).toBe('ja');
    expect(mocks.settingsSave).toHaveBeenLastCalledWith(
      settings('system', 'system', false, undefined, undefined, undefined, undefined, undefined, 'ja'),
    );

    mocks.settingsSave.mockClear();
    // @ts-expect-error deliberately invalid at the runtime boundary
    await store.setTranscribeLanguage('ko');
    expect(mocks.settingsSave).not.toHaveBeenCalled();
    expect(store.transcribeLanguage).toBe('ja');
  });

  it('rolls back chunkMinutes to the last persisted value on a typed save failure', async () => {
    mocks.settingsGet.mockResolvedValue({
      status: 'ok',
      data: settings('system', 'system', false, undefined, undefined, undefined, 5),
    });
    mocks.settingsSave.mockResolvedValue({
      status: 'error',
      error: { category: 'format', code: 'format', detailRedacted: 'rejected' },
    });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();
    await store.load();

    const pending = store.setChunkMinutes(2);
    expect(store.chunkMinutes).toBe(2);
    await pending;

    expect(store.chunkMinutes).toBe(5);
    expect(store.status).toBe('error');
    expect(store.error?.category).toBe('format');
  });

  it('falls back missing chunking/offset/language fields to defaults on load', async () => {
    mocks.settingsGet.mockResolvedValue({
      status: 'ok',
      data: { theme: 'system', uiLanguage: 'system', onboardingCompleted: false },
    });
    const { createSettingsStore } = await import('./settings.svelte');
    const store = createSettingsStore();

    await store.load();

    expect(store.chunkMinutes).toBe(5);
    expect(store.timestampOffsetSec).toBe(0);
    expect(store.transcribeLanguage).toBe('auto');
  });
});
