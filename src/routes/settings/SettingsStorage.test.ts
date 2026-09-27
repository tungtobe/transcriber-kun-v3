// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import { libraryStore } from '../../lib/stores/library.svelte';
import SettingsStorage from './SettingsStorage.svelte';

const mocks = vi.hoisted(() => ({
  libraryStorageStats: vi.fn(),
  libraryOpenDataDir: vi.fn(),
  libraryWipeAll: vi.fn(),
  librarySessionsList: vi.fn(),
  tagsList: vi.fn(),
}));

vi.mock('../../lib/bindings', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../lib/bindings')>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      libraryStorageStats: (...args: unknown[]) => mocks.libraryStorageStats(...args),
      libraryOpenDataDir: (...args: unknown[]) => mocks.libraryOpenDataDir(...args),
      libraryWipeAll: (...args: unknown[]) => mocks.libraryWipeAll(...args),
      librarySessionsList: (...args: unknown[]) => mocks.librarySessionsList(...args),
      tagsList: (...args: unknown[]) => mocks.tagsList(...args),
    },
  };
});

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  libraryStore.reset();
  mocks.libraryStorageStats.mockReset().mockResolvedValue({
    status: 'ok',
    data: { mediaBytes: 125_829_120, dbBytes: 40_960, sessionCount: 3 },
  });
  mocks.libraryOpenDataDir.mockReset();
  mocks.libraryWipeAll.mockReset();
  mocks.librarySessionsList.mockReset().mockResolvedValue({ status: 'ok', data: [] });
  mocks.tagsList.mockReset().mockResolvedValue({ status: 'ok', data: [] });
});

describe('SettingsStorage', () => {
  it('shows Media, DB, and session count once loaded, with no cache-folder option', async () => {
    render(SettingsStorage);

    expect(await screen.findByText('120 MB')).toBeTruthy();
    expect(screen.getByText('40 KB')).toBeTruthy();
    expect(screen.getByText('3')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Mở thư mục' })).toBeTruthy();
    expect(screen.queryByLabelText(/cache/i)).toBeNull();
    expect(screen.queryByText(/cache/i)).toBeNull();
  });

  it('shows an inline error banner when measuring storage fails', async () => {
    mocks.libraryStorageStats.mockReset().mockResolvedValue({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'boom' },
    });
    render(SettingsStorage);

    expect(await screen.findByText('Lỗi kho khoá')).toBeTruthy();
  });

  it('"Mở thư mục" calls the Rust command and surfaces an inline error on failure', async () => {
    mocks.libraryOpenDataDir.mockResolvedValueOnce({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'no opener' },
    });
    render(SettingsStorage);
    await screen.findByText('120 MB');

    await fireEvent.click(screen.getByRole('button', { name: 'Mở thư mục' }));

    expect(mocks.libraryOpenDataDir).toHaveBeenCalledTimes(1);
    expect(await screen.findByText('Lỗi kho khoá')).toBeTruthy();
  });

  it('walks the two-step wipe dialog and only calls libraryWipeAll after the final confirm', async () => {
    mocks.libraryWipeAll.mockResolvedValueOnce({ status: 'ok', data: 'wiped' });
    render(SettingsStorage);
    await screen.findByText('120 MB');

    await fireEvent.click(screen.getByRole('button', { name: 'Xoá toàn bộ dữ liệu…' }));
    expect(screen.getByRole('alertdialog')).toBeTruthy();
    expect(screen.getByText('Xoá toàn bộ dữ liệu?')).toBeTruthy();
    expect(mocks.libraryWipeAll).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole('button', { name: 'Tiếp tục' }));
    expect(screen.getByText('Xác nhận xoá toàn bộ dữ liệu?')).toBeTruthy();
    expect(mocks.libraryWipeAll).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole('button', { name: 'Xoá toàn bộ dữ liệu' }));
    expect(mocks.libraryWipeAll).toHaveBeenCalledTimes(1);
    expect(await screen.findByText('Đã xoá toàn bộ dữ liệu.')).toBeTruthy();
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  it('Escape at either step closes the dialog, calls nothing, and returns focus to the opening button', async () => {
    render(SettingsStorage);
    await screen.findByText('120 MB');
    const openButton = screen.getByRole('button', { name: 'Xoá toàn bộ dữ liệu…' });

    await fireEvent.click(openButton);
    await fireEvent.keyDown(screen.getByRole('alertdialog'), { key: 'Escape' });

    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(mocks.libraryWipeAll).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(openButton);
  });

  it('shows a busy notice when the outcome is Busy, without reloading sessions/tags', async () => {
    mocks.libraryWipeAll.mockResolvedValueOnce({ status: 'ok', data: 'busy' });
    render(SettingsStorage);
    await screen.findByText('120 MB');

    await fireEvent.click(screen.getByRole('button', { name: 'Xoá toàn bộ dữ liệu…' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Tiếp tục' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Xoá toàn bộ dữ liệu' }));

    expect(await screen.findByText('Có tác vụ đang chạy, thử lại khi xong.')).toBeTruthy();
    expect(mocks.librarySessionsList).not.toHaveBeenCalled();
    expect(mocks.tagsList).not.toHaveBeenCalled();
  });

  it('shows an inline error banner when the wipe itself fails', async () => {
    mocks.libraryWipeAll.mockResolvedValueOnce({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'fs failed' },
    });
    render(SettingsStorage);
    await screen.findByText('120 MB');

    await fireEvent.click(screen.getByRole('button', { name: 'Xoá toàn bộ dữ liệu…' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Tiếp tục' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Xoá toàn bộ dữ liệu' }));

    expect(await screen.findByText('Lỗi kho khoá')).toBeTruthy();
  });
});
