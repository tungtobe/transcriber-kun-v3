// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import SettingsChunking from './SettingsChunking.svelte';

const mocks = vi.hoisted(() => ({
  settingsStore: {
    chunkMinutes: 5,
    timestampOffsetSec: 0,
    setChunkMinutes: vi.fn(),
    setTimestampOffsetSec: vi.fn(),
  },
}));

vi.mock('../../lib/stores/settings.svelte', () => ({ settingsStore: mocks.settingsStore }));

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  mocks.settingsStore.chunkMinutes = 5;
  mocks.settingsStore.timestampOffsetSec = 0;
  mocks.settingsStore.setChunkMinutes.mockReset();
  mocks.settingsStore.setTimestampOffsetSec.mockReset();
});

describe('SettingsChunking', () => {
  it('renders the current chunkMinutes and offset values', () => {
    mocks.settingsStore.chunkMinutes = 3;
    mocks.settingsStore.timestampOffsetSec = 90;
    render(SettingsChunking);

    expect((screen.getByLabelText('Độ dài Chunk') as HTMLInputElement).value).toBe('3');
    expect((screen.getByLabelText('Offset timestamp') as HTMLInputElement).value).toBe('90');
  });

  it('saves a valid chunkMinutes on change', async () => {
    render(SettingsChunking);
    const input = screen.getByLabelText('Độ dài Chunk');

    await fireEvent.input(input, { target: { value: '3' } });
    await fireEvent.change(input);

    expect(mocks.settingsStore.setChunkMinutes).toHaveBeenCalledWith(3);
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it.each(['0', '-2', '1.5', 'abc', '', '61', '100'])(
    'blocks an invalid chunkMinutes value (%s) inline without saving',
    async (value) => {
      render(SettingsChunking);
      const input = screen.getByLabelText('Độ dài Chunk');

      await fireEvent.input(input, { target: { value } });
      await fireEvent.change(input);

      expect(mocks.settingsStore.setChunkMinutes).not.toHaveBeenCalled();
      expect(screen.getByRole('alert')).toBeTruthy();
    },
  );

  it('saves the max chunkMinutes boundary (60)', async () => {
    render(SettingsChunking);
    const input = screen.getByLabelText('Độ dài Chunk');

    await fireEvent.input(input, { target: { value: '60' } });
    await fireEvent.change(input);

    expect(mocks.settingsStore.setChunkMinutes).toHaveBeenCalledWith(60);
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('saves a valid offset (including zero) on change', async () => {
    render(SettingsChunking);
    const input = screen.getByLabelText('Offset timestamp');

    await fireEvent.input(input, { target: { value: '3600' } });
    await fireEvent.change(input);
    expect(mocks.settingsStore.setTimestampOffsetSec).toHaveBeenCalledWith(3_600);

    mocks.settingsStore.setTimestampOffsetSec.mockClear();
    await fireEvent.input(input, { target: { value: '0' } });
    await fireEvent.change(input);
    expect(mocks.settingsStore.setTimestampOffsetSec).toHaveBeenCalledWith(0);
  });

  it.each(['-5', '2.5', 'abc', ''])(
    'blocks an invalid offset value (%s) inline without saving',
    async (value) => {
      render(SettingsChunking);
      const input = screen.getByLabelText('Offset timestamp');

      await fireEvent.input(input, { target: { value } });
      await fireEvent.change(input);

      expect(mocks.settingsStore.setTimestampOffsetSec).not.toHaveBeenCalled();
      expect(screen.getByRole('alert')).toBeTruthy();
    },
  );

  it('gives every field a persistent helper line and a (?) tooltip', () => {
    render(SettingsChunking);

    expect(screen.getAllByRole('button')).toHaveLength(2); // one (?) trigger per field
  });
});
