// Theme is a domain store: components talk to this module and never to the
// generated IPC binding directly. It owns persistence, backend events, and
// the OS colour-scheme listener in one lifecycle.
import { commands, events, type AppError, type Theme } from '../bindings';

export type ResolvedTheme = 'light' | 'dark';
export type SettingsStatus = 'idle' | 'loading' | 'ready' | 'error';

export type SettingsState = {
  theme: Theme;
  resolvedTheme: ResolvedTheme;
  status: SettingsStatus;
  error: AppError | null;
};

const THEME_CACHE_KEY = 'trans-kun.theme';

function isTheme(value: unknown): value is Theme {
  return value === 'system' || value === 'light' || value === 'dark';
}

function systemTheme(): ResolvedTheme {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return 'light';
  }
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function readCachedTheme(): Theme | null {
  try {
    const value = globalThis.localStorage?.getItem(THEME_CACHE_KEY);
    return isTheme(value) ? value : null;
  } catch {
    return null;
  }
}

function cacheTheme(theme: Theme): void {
  try {
    globalThis.localStorage?.setItem(THEME_CACHE_KEY, theme);
  } catch {
    // The cache is only a first-paint hint. Persistence still belongs to IPC.
  }
}

function applyTheme(theme: Theme): ResolvedTheme {
  const resolved = theme === 'system' ? systemTheme() : theme;
  if (typeof document !== 'undefined') {
    document.documentElement.dataset.theme = resolved;
    document.documentElement.dataset.themePreference = theme;
  }
  return resolved;
}

type MediaQueryListWithLegacyListener = MediaQueryList & {
  addListener?: (listener: (event: MediaQueryListEvent) => void) => void;
  removeListener?: (listener: (event: MediaQueryListEvent) => void) => void;
};

export function createSettingsStore() {
  let theme = $state<Theme>('system');
  let resolvedTheme = $state<ResolvedTheme>('light');
  let status = $state<SettingsStatus>('idle');
  let error = $state<AppError | null>(null);
  let persistedTheme = $state<Theme>('system');

  let mediaQuery: MediaQueryListWithLegacyListener | undefined;
  let removeMediaListener: (() => void) | undefined;
  let removeBackendListener: (() => void) | undefined;
  let backendListenerPromise: Promise<(() => void) | undefined> | undefined;
  let backendListenerGeneration = 0;
  let listenersReady = false;
  let saveGeneration = 0;

  function renderTheme(next: Theme): void {
    resolvedTheme = applyTheme(next);
  }

  function onSystemThemeChanged(): void {
    if (theme === 'system') {
      renderTheme(theme);
    }
  }

  function installMediaListener(): void {
    if (listenersReady || typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
      return;
    }

    mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    const listener = () => onSystemThemeChanged();
    if (typeof mediaQuery.addEventListener === 'function') {
      mediaQuery.addEventListener('change', listener);
      removeMediaListener = () => mediaQuery?.removeEventListener('change', listener);
    } else if (typeof mediaQuery.addListener === 'function') {
      mediaQuery.addListener(listener);
      removeMediaListener = () => mediaQuery?.removeListener?.(listener);
    }

    listenersReady = true;
  }

  function installBackendListener(): void {
    if (backendListenerPromise || !events?.settingsChanged?.listen) {
      return;
    }

    const generation = backendListenerGeneration;
    backendListenerPromise = Promise.resolve()
      .then(() => events.settingsChanged.listen(({ payload }) => {
        if (generation !== backendListenerGeneration) {
          return;
        }
        const next = payload?.theme;
        if (!isTheme(next)) {
          return;
        }
        persistedTheme = next;
        theme = next;
        error = null;
        status = 'ready';
        cacheTheme(next);
        renderTheme(next);
      }))
      .then((unlisten) => {
        if (generation !== backendListenerGeneration) {
          unlisten();
          return undefined;
        }
        removeBackendListener = unlisten;
        return unlisten;
      })
      .catch(() => undefined);
  }

  function bootstrap(): void {
    if (status !== 'idle') {
      installMediaListener();
      installBackendListener();
      return;
    }

    const cached = readCachedTheme() ?? 'system';
    theme = cached;
    persistedTheme = cached;
    renderTheme(cached);
    installMediaListener();
    installBackendListener();
  }

  async function load(): Promise<void> {
    bootstrap();
    const generation = saveGeneration;
    status = 'loading';
    error = null;

    try {
      const result = await commands.settingsGet();
      if (generation !== saveGeneration) {
        return;
      }

      if (result.status === 'ok' && isTheme(result.data.theme)) {
        persistedTheme = result.data.theme;
        theme = result.data.theme;
        cacheTheme(theme);
        renderTheme(theme);
        status = 'ready';
        return;
      }

      persistedTheme = 'system';
      theme = 'system';
      cacheTheme('system');
      renderTheme(theme);
      if (result.status === 'error') {
        error = result.error;
      }
    } catch {
      if (generation !== saveGeneration) {
        return;
      }

      persistedTheme = 'system';
      theme = 'system';
      cacheTheme('system');
      renderTheme(theme);
      error = null;
    }

    status = error ? 'error' : 'ready';
  }

  async function setTheme(next: Theme): Promise<void> {
    if (!isTheme(next)) {
      return;
    }

    bootstrap();
    const previousPersisted = persistedTheme;
    const generation = ++saveGeneration;
    theme = next;
    error = null;
    status = 'ready';
    cacheTheme(next);
    renderTheme(next);

    try {
      const result = await commands.settingsSave({ theme: next });
      if (generation !== saveGeneration) {
        return;
      }

      if (result.status === 'ok') {
        persistedTheme = next;
        status = 'ready';
        return;
      }

      theme = previousPersisted;
      error = result.error;
      status = 'error';
      cacheTheme(previousPersisted);
      renderTheme(previousPersisted);
    } catch {
      if (generation !== saveGeneration) {
        return;
      }
      theme = previousPersisted;
      error = null;
      status = 'error';
      cacheTheme(previousPersisted);
      renderTheme(previousPersisted);
    }
  }

  function destroy(): void {
    saveGeneration += 1;
    backendListenerGeneration += 1;
    removeMediaListener?.();
    removeMediaListener = undefined;
    mediaQuery = undefined;
    listenersReady = false;
    removeBackendListener?.();
    removeBackendListener = undefined;
    backendListenerPromise = undefined;
  }

  // Set a useful first-paint value even when a caller mounts the store
  // directly in a test or a non-Tauri browser preview.
  bootstrap();

  return {
    get theme() {
      return theme;
    },
    get resolvedTheme() {
      return resolvedTheme;
    },
    get status() {
      return status;
    },
    get error() {
      return error;
    },
    get state(): SettingsState {
      return { theme, resolvedTheme, status, error };
    },
    bootstrap,
    load,
    setTheme,
    destroy,
  };
}

export const settingsStore = createSettingsStore();
