// Domain settings store: the only frontend owner of typed settings IPC,
// optimistic persistence, backend events, and preference side effects.
import { i18n, isUiLanguage } from '../../i18n/index.svelte';
import {
  commands,
  events,
  type AppError,
  type ConsentPolicy,
  type ConsentStatus,
  type ModelKind,
  type Settings,
  type Theme,
  type TranscribeLanguage,
  type UiLanguage,
} from '../bindings';

export type ResolvedTheme = 'light' | 'dark';
export type SettingsStatus = 'idle' | 'loading' | 'ready' | 'error';

export type SettingsState = Settings & {
  resolvedTheme: ResolvedTheme;
  status: SettingsStatus;
  error: AppError | null;
};

const THEME_CACHE_KEY = 'trans-kun.theme';
// Mirrors `core::model_defaults` (Rust): duplicated here only as an
// offline-first fallback before the real settings snapshot loads, exactly
// like the other `DEFAULT_SETTINGS` fields already do for theme/uiLanguage.
const DEFAULT_TRANSCRIBE_MODEL = 'gemini-flash-lite-latest';
const DEFAULT_LIVE_MODEL = 'gemini-3.5-live-translate-preview';
const DEFAULT_MEMO_MODEL = 'gemini-flash-lite-latest';
// Mirrors `settings::DEFAULT_CHUNK_MINUTES`/`TranscribeLanguage::default()`
// (Rust) as an offline-first fallback, same reasoning as the model defaults
// above.
const DEFAULT_CHUNK_MINUTES = 5;
// Mirrors `settings::MAX_CHUNK_MINUTES` (Rust, P0 review fix) -- a defense-
// in-depth ceiling matching Rust's `save`/`load` range check, on top of the
// primary inline block in Settings -> Chunking.
const MAX_CHUNK_MINUTES = 60;
const DEFAULT_TIMESTAMP_OFFSET_SEC = 0;
const DEFAULT_TRANSCRIBE_LANGUAGE: TranscribeLanguage = 'auto';
const DEFAULT_SETTINGS: Settings = {
  theme: 'system',
  uiLanguage: 'system',
  onboardingCompleted: false,
  consentAcceptedVersion: 0,
  consentDeclined: false,
  transcribeModel: DEFAULT_TRANSCRIBE_MODEL,
  liveModel: DEFAULT_LIVE_MODEL,
  memoModel: DEFAULT_MEMO_MODEL,
  chunkMinutes: DEFAULT_CHUNK_MINUTES,
  timestampOffsetSec: DEFAULT_TIMESTAMP_OFFSET_SEC,
  transcribeLanguage: DEFAULT_TRANSCRIBE_LANGUAGE,
};

function isTheme(value: unknown): value is Theme {
  return value === 'system' || value === 'light' || value === 'dark';
}

function isTranscribeLanguage(value: unknown): value is TranscribeLanguage {
  return value === 'auto' || value === 'ja' || value === 'vi' || value === 'en';
}

/** A non-negative integer, e.g. a hand-edited/legacy row's `0.5` or `-1`. */
function isNonNegativeInteger(value: unknown): value is number {
  return typeof value === 'number' && Number.isInteger(value) && value >= 0;
}

/** Same, plus the `>= 1` floor `chunkMinutes` itself requires. */
function isPositiveInteger(value: unknown): value is number {
  return isNonNegativeInteger(value) && value >= 1;
}

/** `chunkMinutes` itself: a positive integer capped at `MAX_CHUNK_MINUTES` (spec Boundaries Always: "1..=60"). */
function isChunkMinutes(value: unknown): value is number {
  return isPositiveInteger(value) && value <= MAX_CHUNK_MINUTES;
}

function systemTheme(): ResolvedTheme {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return 'light';
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
    // Rust settings remain authoritative; this is only a first-render hint.
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

function isNonEmptyModelName(value: unknown): value is string {
  return typeof value === 'string' && value.trim().length > 0;
}

function normalizeSettings(value: Partial<Settings> | null | undefined): Settings {
  return {
    theme: isTheme(value?.theme) ? value.theme : 'system',
    uiLanguage: isUiLanguage(value?.uiLanguage) ? value.uiLanguage : 'system',
    onboardingCompleted: typeof value?.onboardingCompleted === 'boolean'
      ? value.onboardingCompleted
      : false,
    consentAcceptedVersion: typeof value?.consentAcceptedVersion === 'number'
      && Number.isInteger(value.consentAcceptedVersion)
      && value.consentAcceptedVersion >= 0
      ? value.consentAcceptedVersion
      : 0,
    consentDeclined: typeof value?.consentDeclined === 'boolean' ? value.consentDeclined : false,
    transcribeModel: isNonEmptyModelName(value?.transcribeModel)
      ? value.transcribeModel
      : DEFAULT_TRANSCRIBE_MODEL,
    liveModel: isNonEmptyModelName(value?.liveModel) ? value.liveModel : DEFAULT_LIVE_MODEL,
    memoModel: isNonEmptyModelName(value?.memoModel) ? value.memoModel : DEFAULT_MEMO_MODEL,
    chunkMinutes: isChunkMinutes(value?.chunkMinutes)
      ? value.chunkMinutes
      : DEFAULT_CHUNK_MINUTES,
    timestampOffsetSec: isNonNegativeInteger(value?.timestampOffsetSec)
      ? value.timestampOffsetSec
      : DEFAULT_TIMESTAMP_OFFSET_SEC,
    transcribeLanguage: isTranscribeLanguage(value?.transcribeLanguage)
      ? value.transcribeLanguage
      : DEFAULT_TRANSCRIBE_LANGUAGE,
  };
}

function policyForSettings(policy: ConsentPolicy | null, settings: Settings): ConsentPolicy | null {
  if (!policy) return null;
  const status: ConsentStatus = settings.consentDeclined
    ? 'declined'
    : settings.consentAcceptedVersion === policy.currentVersion
      ? 'current'
      : settings.consentAcceptedVersion === 0 ? 'pending' : 'stale';
  return {
    ...policy,
    acceptedVersion: settings.consentAcceptedVersion,
    declined: settings.consentDeclined,
    status,
  };
}

type MediaQueryListWithLegacyListener = MediaQueryList & {
  addListener?: (listener: (event: MediaQueryListEvent) => void) => void;
  removeListener?: (listener: (event: MediaQueryListEvent) => void) => void;
};

export function createSettingsStore() {
  let theme = $state<Theme>('system');
  let resolvedTheme = $state<ResolvedTheme>('light');
  let uiLanguage = $state<UiLanguage>('system');
  let onboardingCompleted = $state(false);
  let consentAcceptedVersion = $state(0);
  let consentDeclined = $state(false);
  let transcribeModel = $state(DEFAULT_TRANSCRIBE_MODEL);
  let liveModel = $state(DEFAULT_LIVE_MODEL);
  let memoModel = $state(DEFAULT_MEMO_MODEL);
  let chunkMinutes = $state(DEFAULT_CHUNK_MINUTES);
  let timestampOffsetSec = $state(DEFAULT_TIMESTAMP_OFFSET_SEC);
  let transcribeLanguage = $state<TranscribeLanguage>(DEFAULT_TRANSCRIBE_LANGUAGE);
  let consentPolicy = $state<ConsentPolicy | null>(null);
  let status = $state<SettingsStatus>('idle');
  let error = $state<AppError | null>(null);
  let persistedSettings: Settings = { ...DEFAULT_SETTINGS };

  let mediaQuery: MediaQueryListWithLegacyListener | undefined;
  let removeMediaListener: (() => void) | undefined;
  let removeBackendListener: (() => void) | undefined;
  let backendListenerPromise: Promise<(() => void) | undefined> | undefined;
  let backendListenerGeneration = 0;
  let listenersReady = false;
  let saveGeneration = 0;
  let activeLoad: Promise<void> | undefined;
  let saveQueue: Promise<void> = Promise.resolve();
  let pendingSaveCount = 0;

  function transportSaveError(): AppError {
    return {
      category: 'storage',
      code: 'storage',
      detailRedacted: 'settings save unavailable',
    };
  }

  function snapshot(): Settings {
    const base = {
      theme,
      uiLanguage,
      onboardingCompleted,
      transcribeModel,
      liveModel,
      memoModel,
      chunkMinutes,
      timestampOffsetSec,
      transcribeLanguage,
    } as Settings;
    // Keep compatibility with pre-consent test doubles/older WebViews while
    // the real Rust snapshot always includes these fields after load.
    if (consentPolicy || consentAcceptedVersion !== 0 || consentDeclined) {
      base.consentAcceptedVersion = consentAcceptedVersion;
      base.consentDeclined = consentDeclined;
    }
    return base;
  }

  function applySettings(next: Settings): void {
    theme = next.theme;
    uiLanguage = next.uiLanguage;
    onboardingCompleted = next.onboardingCompleted;
    consentAcceptedVersion = next.consentAcceptedVersion;
    consentDeclined = next.consentDeclined;
    transcribeModel = next.transcribeModel;
    liveModel = next.liveModel;
    memoModel = next.memoModel;
    chunkMinutes = next.chunkMinutes;
    timestampOffsetSec = next.timestampOffsetSec;
    transcribeLanguage = next.transcribeLanguage;
    cacheTheme(theme);
    resolvedTheme = applyTheme(theme);
    i18n.applyPreference(uiLanguage);
  }

  function onSystemThemeChanged(): void {
    if (theme === 'system') resolvedTheme = applyTheme(theme);
  }

  function installMediaListener(): void {
    if (listenersReady || typeof window === 'undefined' || typeof window.matchMedia !== 'function') return;
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
    if (backendListenerPromise || !events?.settingsChanged?.listen) return;
    const generation = backendListenerGeneration;
    backendListenerPromise = Promise.resolve()
      .then(() => events.settingsChanged.listen(({ payload }) => {
        if (generation !== backendListenerGeneration) return;
        // Local saves are authoritative while queued. Their command results
        // advance `persistedSettings`; replaying their events here could
        // overwrite a newer optimistic snapshot with an older queued one.
        if (pendingSaveCount > 0) return;
        const next = normalizeSettings(payload);
        persistedSettings = next;
        applySettings(next);
        error = null;
        status = 'ready';
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
    const cached: Settings = {
      theme: readCachedTheme() ?? 'system',
      uiLanguage: i18n.preference,
      onboardingCompleted: false,
      consentAcceptedVersion: 0,
      consentDeclined: false,
      transcribeModel: DEFAULT_TRANSCRIBE_MODEL,
      liveModel: DEFAULT_LIVE_MODEL,
      memoModel: DEFAULT_MEMO_MODEL,
      chunkMinutes: DEFAULT_CHUNK_MINUTES,
      timestampOffsetSec: DEFAULT_TIMESTAMP_OFFSET_SEC,
      transcribeLanguage: DEFAULT_TRANSCRIBE_LANGUAGE,
    };
    persistedSettings = cached;
    applySettings(cached);
    installMediaListener();
    installBackendListener();
  }

  function load(): Promise<void> {
    if (activeLoad) return activeLoad;
    bootstrap();
    const generation = saveGeneration;
    status = 'loading';
    error = null;
    let request: Promise<void>;
    request = (async () => {
      try {
        const [result, policyResult] = await Promise.all([
          commands.settingsGet(),
          typeof commands.consentPolicy === 'function'
            ? commands.consentPolicy()
            : Promise.resolve(null),
        ]);
        if (generation !== saveGeneration) return;
        if (result.status === 'ok') {
          const next = normalizeSettings(result.data);
          persistedSettings = next;
          applySettings(next);
          consentPolicy = policyResult && policyResult.status === 'ok' ? policyResult.data : null;
          status = 'ready';
          return;
        }
        persistedSettings = { ...DEFAULT_SETTINGS };
        applySettings(persistedSettings);
        consentPolicy = null;
        error = result.error;
      } catch {
        if (generation !== saveGeneration) return;
        persistedSettings = { ...DEFAULT_SETTINGS };
        applySettings(persistedSettings);
        error = null;
      }
      status = error ? 'error' : 'ready';
    })().finally(() => {
      if (activeLoad === request) activeLoad = undefined;
    });
    activeLoad = request;
    return request;
  }

  function ensureReady(): Promise<void> | undefined {
    bootstrap();
    if (activeLoad) return activeLoad;
    if (status === 'idle') return load();
    return undefined;
  }

  function persist(
    next: Settings,
    saveCommand: (settings: Settings) => Promise<
      { status: 'ok'; data: unknown } | { status: 'error'; error: AppError }
    > = commands.settingsSave,
  ): Promise<void> {
    const generation = ++saveGeneration;
    pendingSaveCount += 1;
    applySettings(next);
    error = null;
    status = 'ready';
    const operation = saveQueue.then(async () => {
      let failure: AppError | null = null;
      let savedSettings = next;
      try {
        const result = await saveCommand(next);
        if (result.status === 'error') failure = result.error;
        else if (result.data && typeof result.data === 'object') savedSettings = normalizeSettings(result.data);
      } catch {
        failure = transportSaveError();
      }

      if (!failure) {
        persistedSettings = { ...savedSettings };
        consentPolicy = policyForSettings(consentPolicy, savedSettings);
        if (generation === saveGeneration) {
          // Re-apply because a backend event from this queued save may have
          // arrived while a newer optimistic snapshot was visible.
          applySettings(savedSettings);
          error = null;
          status = 'ready';
        }
        return;
      }

      // A superseded failure must not disturb a newer optimistic save. The
      // latest successful snapshot is still the rollback anchor for that save.
      if (generation !== saveGeneration) return;
      applySettings(persistedSettings);
      error = failure;
      status = 'error';
    }).finally(() => {
      pendingSaveCount -= 1;
    });
    saveQueue = operation.catch(() => undefined);
    return operation;
  }

  async function setTheme(next: Theme): Promise<void> {
    if (!isTheme(next)) return;
    const waiting = ensureReady();
    applySettings({ ...snapshot(), theme: next });
    error = null;
    status = 'ready';
    if (waiting) await waiting;
    return persist({ ...snapshot(), theme: next });
  }

  async function setUiLanguage(next: UiLanguage): Promise<void> {
    if (!isUiLanguage(next)) return;
    const waiting = ensureReady();
    applySettings({ ...snapshot(), uiLanguage: next });
    error = null;
    status = 'ready';
    if (waiting) await waiting;
    return persist({ ...snapshot(), uiLanguage: next });
  }

  function withModel(base: Settings, kind: ModelKind, name: string): Settings {
    switch (kind) {
      case 'transcribe':
        return { ...base, transcribeModel: name };
      case 'live':
        return { ...base, liveModel: name };
      case 'memo':
        return { ...base, memoModel: name };
    }
  }

  /**
   * Persist a free-text model name for one of the three kinds, following the
   * same optimistic/persist/rollback shape as `setTheme`/`setUiLanguage`.
   * Empty or whitespace-only input is blocked here too (spec Always: "rỗng
   * hoặc chỉ khoảng trắng bị chặn inline ngay khi nhập (không lưu)") — the
   * primary inline block lives in the Settings → Gemini UI, this is defense
   * in depth so the store itself never sends a blank value to `settingsSave`.
   */
  async function setModel(kind: ModelKind, name: string): Promise<void> {
    const trimmed = name.trim();
    if (!trimmed) return;
    const waiting = ensureReady();
    applySettings(withModel(snapshot(), kind, trimmed));
    error = null;
    status = 'ready';
    if (waiting) await waiting;
    return persist(withModel(snapshot(), kind, trimmed));
  }

  /**
   * Persist `chunkMinutes`, following the same optimistic/persist/rollback
   * shape as `setTheme`. Invalid input (not a positive integer) is blocked
   * here too, in defense in depth alongside the primary inline block in
   * Settings → Chunking (spec Always: "UI chặn tại chỗ giá trị không hợp lệ
   * ... và không lưu").
   */
  async function setChunkMinutes(next: number): Promise<void> {
    if (!isChunkMinutes(next)) return;
    const waiting = ensureReady();
    applySettings({ ...snapshot(), chunkMinutes: next });
    error = null;
    status = 'ready';
    if (waiting) await waiting;
    return persist({ ...snapshot(), chunkMinutes: next });
  }

  /** Same shape as `setChunkMinutes`, for `timestampOffsetSec` (>= 0). */
  async function setTimestampOffsetSec(next: number): Promise<void> {
    if (!isNonNegativeInteger(next)) return;
    const waiting = ensureReady();
    applySettings({ ...snapshot(), timestampOffsetSec: next });
    error = null;
    status = 'ready';
    if (waiting) await waiting;
    return persist({ ...snapshot(), timestampOffsetSec: next });
  }

  async function setTranscribeLanguage(next: TranscribeLanguage): Promise<void> {
    if (!isTranscribeLanguage(next)) return;
    const waiting = ensureReady();
    applySettings({ ...snapshot(), transcribeLanguage: next });
    error = null;
    status = 'ready';
    if (waiting) await waiting;
    return persist({ ...snapshot(), transcribeLanguage: next });
  }

  async function setOnboardingCompleted(next: boolean): Promise<void> {
    const waiting = ensureReady();
    applySettings({ ...snapshot(), onboardingCompleted: next });
    error = null;
    status = 'ready';
    if (waiting) await waiting;
    return persist({ ...snapshot(), onboardingCompleted: next });
  }

  async function acceptConsent(): Promise<void> {
    const waiting = ensureReady();
    if (waiting) await waiting;
    const next = snapshot();
    return persist(next, commands.consentAccept);
  }

  async function declineConsent(): Promise<void> {
    const waiting = ensureReady();
    if (waiting) await waiting;
    const next = snapshot();
    return persist(next, commands.consentDecline);
  }

  function destroy(): void {
    saveGeneration += 1;
    activeLoad = undefined;
    backendListenerGeneration += 1;
    removeMediaListener?.();
    removeMediaListener = undefined;
    mediaQuery = undefined;
    listenersReady = false;
    removeBackendListener?.();
    removeBackendListener = undefined;
    backendListenerPromise = undefined;
  }

  bootstrap();

  return {
    get theme() { return theme; },
    get resolvedTheme() { return resolvedTheme; },
    get uiLanguage() { return uiLanguage; },
    get onboardingCompleted() { return onboardingCompleted; },
    get consentAcceptedVersion() { return consentAcceptedVersion; },
    get consentDeclined() { return consentDeclined; },
    get transcribeModel() { return transcribeModel; },
    get liveModel() { return liveModel; },
    get memoModel() { return memoModel; },
    get chunkMinutes() { return chunkMinutes; },
    get timestampOffsetSec() { return timestampOffsetSec; },
    get transcribeLanguage() { return transcribeLanguage; },
    get consentPolicy() { return consentPolicy; },
    get consentStatus(): ConsentStatus { return consentPolicy?.status ?? 'pending'; },
    get status() { return status; },
    get error() { return error; },
    get state(): SettingsState {
      return { ...snapshot(), resolvedTheme, status, error };
    },
    bootstrap,
    load,
    setTheme,
    setUiLanguage,
    setModel,
    setChunkMinutes,
    setTimestampOffsetSec,
    setTranscribeLanguage,
    setOnboardingCompleted,
    acceptConsent,
    declineConsent,
    destroy,
  };
}

export const settingsStore = createSettingsStore();
