// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import SettingsGeneral from './SettingsGeneral.svelte';

const mocks = vi.hoisted(() => ({
  settingsStore: {
    uiLanguage: 'system' as 'system' | 'vi' | 'en' | 'ja',
    theme: 'system' as 'system' | 'light' | 'dark',
    setUiLanguage: vi.fn(),
    setTheme: vi.fn(),
  },
}));

vi.mock('../../lib/stores/settings.svelte', () => ({ settingsStore: mocks.settingsStore }));

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  mocks.settingsStore.uiLanguage = 'system';
  mocks.settingsStore.theme = 'system';
  mocks.settingsStore.setUiLanguage.mockReset();
  mocks.settingsStore.setTheme.mockReset();
});

describe('SettingsGeneral', () => {
  it('renders the language select with the four options and the current value selected', () => {
    mocks.settingsStore.uiLanguage = 'ja';
    render(SettingsGeneral);

    const select = screen.getByLabelText('Ngôn ngữ') as HTMLSelectElement;
    expect(select.value).toBe('ja');
    expect(Array.from(select.options).map((option) => option.value)).toEqual([
      'system',
      'vi',
      'en',
      'ja',
    ]);
  });

  it('calls setUiLanguage on change', async () => {
    render(SettingsGeneral);

    const select = screen.getByLabelText('Ngôn ngữ') as HTMLSelectElement;
    await fireEvent.change(select, { target: { value: 'en' } });

    expect(mocks.settingsStore.setUiLanguage).toHaveBeenCalledWith('en');
  });

  it('renders the theme select with the current value selected', () => {
    mocks.settingsStore.theme = 'dark';
    render(SettingsGeneral);

    const select = screen.getByLabelText('Giao diện') as HTMLSelectElement;
    expect(select.value).toBe('dark');
    expect(Array.from(select.options).map((option) => option.value)).toEqual([
      'system',
      'light',
      'dark',
    ]);
  });

  it('calls setTheme on change', async () => {
    render(SettingsGeneral);

    const select = screen.getByLabelText('Giao diện') as HTMLSelectElement;
    await fireEvent.change(select, { target: { value: 'light' } });

    expect(mocks.settingsStore.setTheme).toHaveBeenCalledWith('light');
  });

  it('gives every field a persistent helper line and a (?) tooltip', () => {
    render(SettingsGeneral);

    // Both fields happen to share the same persistent helper copy.
    expect(screen.getAllByText('Áp dụng ngay, không cần khởi động lại.')).toHaveLength(2);
    expect(screen.getAllByRole('button')).toHaveLength(2); // one (?) trigger per field
  });
});
