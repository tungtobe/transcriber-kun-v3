// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import Onboarding from './Onboarding.svelte';

const mocks = vi.hoisted(() => ({
  settingsStore: {
    uiLanguage: 'system' as 'system' | 'vi' | 'en' | 'ja',
    setUiLanguage: vi.fn(),
  },
}));

vi.mock('../lib/stores/settings.svelte', () => ({ settingsStore: mocks.settingsStore }));

afterEach(() => cleanup());

beforeEach(() => {
  Object.defineProperty(navigator, 'language', { configurable: true, value: 'en-US' });
  mocks.settingsStore.uiLanguage = 'system';
  mocks.settingsStore.setUiLanguage.mockReset().mockImplementation(async (next) => {
    mocks.settingsStore.uiLanguage = next;
    i18n.applyPreference(next);
  });
  i18n.applyPreference('system');
});

describe('Onboarding language step', () => {
  it('renders a three-step, three-language accessible selector with system badge', () => {
    const { container } = render(Onboarding);

    expect(container.querySelector('ol.stepper')).toBeTruthy();
    expect(container.querySelectorAll('ol.stepper > li')).toHaveLength(3);
    expect(container.querySelectorAll('[aria-current="step"]')).toHaveLength(1);
    expect(screen.getByLabelText('Setup progress').textContent).toContain('Language');
    expect(screen.getAllByRole('radio')).toHaveLength(3);
    expect(screen.getByRole('radio', { name: /English/ })).toHaveProperty('checked', true);
    expect(screen.getByText('System language')).toBeTruthy();
    expect(screen.queryByText(/environment/i)).toBeNull();
  });

  it('uses Japanese on first launch when the system locale is ja-JP', () => {
    Object.defineProperty(navigator, 'language', { configurable: true, value: 'ja-JP' });
    mocks.settingsStore.uiLanguage = 'system';
    i18n.applyPreference('system');

    render(Onboarding);

    expect(screen.getByRole('radio', { name: /日本語/ })).toHaveProperty('checked', true);
    expect(screen.getByRole('heading', { name: 'trans-kun へようこそ' })).toBeTruthy();
    expect(screen.getByText('システム言語')).toBeTruthy();
  });

  it('switches the entire step to Japanese immediately and persists through the domain store', async () => {
    render(Onboarding);

    await fireEvent.click(screen.getByRole('radio', { name: /日本語/ }));

    expect(mocks.settingsStore.setUiLanguage).toHaveBeenCalledWith('ja');
    expect(await screen.findByRole('heading', { name: 'trans-kun へようこそ' })).toBeTruthy();
    expect(screen.getByLabelText('セットアップの進行状況').textContent).toContain('データ');
    expect(document.documentElement.lang).toBe('ja');
  });
});
