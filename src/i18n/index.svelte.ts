import type { UiLanguage } from '../lib/bindings';
import en from './en.json';
import ja from './ja.json';
import vi from './vi.json';

export const SUPPORTED_LOCALES = ['vi', 'en', 'ja'] as const;
export type Locale = (typeof SUPPORTED_LOCALES)[number];
export type TranslationKey = keyof typeof en;
export type TranslationParams = Record<string, string | number>;

const FALLBACK_LOCALE: Locale = 'en';
const LANGUAGE_CACHE_KEY = 'trans-kun.ui-language';
const resources: Record<Locale, Record<TranslationKey, string>> = { vi, en, ja };

export function isUiLanguage(value: unknown): value is UiLanguage {
  return value === 'system' || SUPPORTED_LOCALES.includes(value as Locale);
}

export function detectSystemLocale(language?: string): Locale {
  const raw = language ?? (typeof navigator === 'undefined' ? '' : navigator.language);
  const primary = raw.toLowerCase().split(/[-_]/, 1)[0];
  return SUPPORTED_LOCALES.includes(primary as Locale) ? primary as Locale : FALLBACK_LOCALE;
}

function readCachedPreference(): UiLanguage | null {
  try {
    const value = globalThis.localStorage?.getItem(LANGUAGE_CACHE_KEY);
    return isUiLanguage(value) ? value : null;
  } catch {
    return null;
  }
}

function cachePreference(preference: UiLanguage): void {
  try {
    globalThis.localStorage?.setItem(LANGUAGE_CACHE_KEY, preference);
  } catch {
    // Rust settings remain authoritative; this cache only prevents first-render drift.
  }
}

export function createI18nStore() {
  let preference = $state<UiLanguage>('system');
  let locale = $state<Locale>(detectSystemLocale());
  let systemLocale = $state<Locale>(detectSystemLocale());

  function applyPreference(next: UiLanguage): Locale {
    preference = isUiLanguage(next) ? next : 'system';
    systemLocale = detectSystemLocale();
    locale = preference === 'system' ? systemLocale : preference;
    cachePreference(preference);
    if (typeof document !== 'undefined') {
      document.documentElement.lang = locale;
    }
    return locale;
  }

  function bootstrap(): Locale {
    return applyPreference(readCachedPreference() ?? 'system');
  }

  function t(key: TranslationKey, params?: TranslationParams): string {
    const template = resources[locale][key] ?? resources[FALLBACK_LOCALE][key] ?? key;
    if (!params) {
      return template;
    }
    return template.replace(/\{([\w]+)\}/g, (match, name: string) =>
      Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : match,
    );
  }

  bootstrap();

  return {
    get preference() {
      return preference;
    },
    get locale() {
      return locale;
    },
    get systemLocale() {
      return systemLocale;
    },
    applyPreference,
    bootstrap,
    t,
  };
}

export const i18n = createI18nStore();
