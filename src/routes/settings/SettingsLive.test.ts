// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import SettingsLive from './SettingsLive.svelte';

const mocks = vi.hoisted(() => ({
  settingsStore: {
    liveTarget: 'none' as 'none' | 'ja' | 'vi' | 'en',
    setLiveTarget: vi.fn(),
  },
}));

vi.mock('../../lib/stores/settings.svelte', () => ({ settingsStore: mocks.settingsStore }));

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  mocks.settingsStore.liveTarget = 'none';
  mocks.settingsStore.setLiveTarget.mockReset();
});

describe('SettingsLive', () => {
  it('renders the default Target select with "Không dịch" and the three languages', () => {
    mocks.settingsStore.liveTarget = 'vi';
    render(SettingsLive);

    const select = screen.getByLabelText('Ngôn ngữ dịch mặc định') as HTMLSelectElement;
    expect(select.value).toBe('vi');
    expect(Array.from(select.options).map((option) => option.value)).toEqual([
      'none',
      'ja',
      'vi',
      'en',
    ]);
    expect(select.options[0].textContent).toBe('Không dịch');
  });

  it('saves the chosen Target through the settings store', async () => {
    render(SettingsLive);

    const select = screen.getByLabelText('Ngôn ngữ dịch mặc định') as HTMLSelectElement;
    await fireEvent.change(select, { target: { value: 'en' } });

    expect(mocks.settingsStore.setLiveTarget).toHaveBeenCalledWith('en');
  });
});
