// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import PartialBanner from './PartialBanner.svelte';

vi.mock('../../lib/stores/settings.svelte', () => ({
  settingsStore: { timestampOffsetSec: 0 },
}));

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
});

describe('PartialBanner', () => {
  it('lists every gap range as mm:ss–mm:ss, with two rerun buttons each carrying a token badge', () => {
    const onRerun = vi.fn();
    render(PartialBanner, {
      gapRanges: [
        { startSec: 65, endSec: 90 },
        { startSec: 200, endSec: 210 },
      ],
      starting: false,
      errorMessage: null,
      onRerun,
    });

    expect(screen.getByText('01:05–01:30')).toBeTruthy();
    expect(screen.getByText('03:20–03:30')).toBeTruthy();
    expect(screen.getAllByText('Tốn token Gemini').length).toBe(2);
  });

  it('calls onRerun with the missing/all scope on the matching button', async () => {
    const onRerun = vi.fn();
    render(PartialBanner, { gapRanges: [], starting: false, errorMessage: null, onRerun });

    await fireEvent.click(screen.getByRole('button', { name: /Chạy lại phần thiếu/ }));
    expect(onRerun).toHaveBeenCalledWith({ kind: 'missing' });

    await fireEvent.click(screen.getByRole('button', { name: /Chạy lại toàn bộ/ }));
    expect(onRerun).toHaveBeenCalledWith({ kind: 'all' });
  });

  it('disables both buttons while starting and shows an error message', () => {
    render(PartialBanner, {
      gapRanges: [],
      starting: true,
      errorMessage: 'Không chạy lại được. Thử lại sau.',
      onRerun: vi.fn(),
    });

    expect((screen.getByRole('button', { name: /Chạy lại phần thiếu/ }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole('button', { name: /Chạy lại toàn bộ/ }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByRole('alert')).toHaveProperty('textContent', 'Không chạy lại được. Thử lại sau.');
  });
});
