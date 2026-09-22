// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';

describe('i18n runtime', () => {
  beforeEach(() => {
    vi.resetModules();
    localStorage.clear();
    document.documentElement.lang = 'en';
  });

  it.each([
    ['vi-VN', 'vi'],
    ['en-US', 'en'],
    ['ja-JP', 'ja'],
    ['fr-FR', 'en'],
  ] as const)('normalizes system locale %s to %s', async (system, expected) => {
    const { detectSystemLocale } = await import('./index.svelte');
    expect(detectSystemLocale(system)).toBe(expected);
  });

  it('applies cached/system preference before render and updates document language', async () => {
    Object.defineProperty(navigator, 'language', { configurable: true, value: 'ja-JP' });
    localStorage.setItem('trans-kun.ui-language', 'system');
    const { createI18nStore } = await import('./index.svelte');
    const store = createI18nStore();

    expect(store.preference).toBe('system');
    expect(store.locale).toBe('ja');
    expect(store.systemLocale).toBe('ja');
    expect(document.documentElement.lang).toBe('ja');
    expect(store.t('onboarding.stepper.language')).toBe('言語');
  });

  it('switches synchronously and interpolates text without HTML insertion', async () => {
    const { createI18nStore } = await import('./index.svelte');
    const store = createI18nStore();

    store.applyPreference('vi');

    expect(store.locale).toBe('vi');
    expect(store.t('settings.meta.title', { group: '<b>Chung</b>' }))
      .toBe('<b>Chung</b> · Cài đặt · trans-kun');
    expect(document.documentElement.lang).toBe('vi');
    expect(localStorage.getItem('trans-kun.ui-language')).toBe('vi');
  });
});
