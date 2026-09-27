// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import CloseConfirm from './CloseConfirm.svelte';

const mocks = vi.hoisted(() => ({
  appStore: {
    listenForCloseRequested: vi.fn(),
    confirmClose: vi.fn(),
  },
  notesStore: {
    flushAll: vi.fn(),
  },
}));

vi.mock('../lib/stores/app.svelte', () => ({ appStore: mocks.appStore }));
vi.mock('../lib/stores/notes.svelte', () => ({ notesStore: mocks.notesStore }));

let closeRequestedCallback: ((busy: boolean) => void) | null = null;
const unlisten = vi.fn();

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  closeRequestedCallback = null;
  unlisten.mockReset();
  mocks.appStore.listenForCloseRequested.mockReset().mockImplementation((cb: (busy: boolean) => void) => {
    closeRequestedCallback = cb;
    return Promise.resolve(unlisten);
  });
  mocks.appStore.confirmClose.mockReset().mockResolvedValue(undefined);
  mocks.notesStore.flushAll.mockReset().mockResolvedValue(true);
});

describe('CloseConfirm', () => {
  it('renders nothing until CloseRequested fires', async () => {
    render(CloseConfirm);
    await Promise.resolve();

    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  // Spec Acceptance: "Given không có Job, when đóng cửa sổ và ghi chú lưu
  // được, then app thoát như trước (không có dialog thừa)".
  it('busy=false and flush ok: flushes notes then confirms close with no dialog', async () => {
    render(CloseConfirm);
    await Promise.resolve();

    closeRequestedCallback?.(false);
    await Promise.resolve();
    await Promise.resolve();

    expect(mocks.notesStore.flushAll).toHaveBeenCalledTimes(1);
    expect(mocks.appStore.confirmClose).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  // Spec Code Map: "flush lỗi → dialog cho chọn 'Ở lại' (mặc định, focus)
  // hoặc 'Vẫn thoát'".
  it('busy=false and flush fails: shows the stay/exit-anyway dialog without closing', async () => {
    mocks.notesStore.flushAll.mockResolvedValue(false);
    render(CloseConfirm);
    await Promise.resolve();

    closeRequestedCallback?.(false);

    const dialog = await waitFor(() => screen.getByRole('alertdialog'));
    expect(dialog).toBeTruthy();
    expect(mocks.appStore.confirmClose).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Ở lại' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Vẫn thoát' })).toBeTruthy();
  });

  it('flush-error dialog: "Ở lại" dismisses without exiting', async () => {
    mocks.notesStore.flushAll.mockResolvedValue(false);
    render(CloseConfirm);
    await Promise.resolve();
    closeRequestedCallback?.(false);
    await waitFor(() => screen.getByRole('alertdialog'));

    await fireEvent.click(screen.getByRole('button', { name: 'Ở lại' }));

    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(mocks.appStore.confirmClose).not.toHaveBeenCalled();
  });

  it('flush-error dialog: "Vẫn thoát" exits without a second flush attempt', async () => {
    mocks.notesStore.flushAll.mockResolvedValue(false);
    render(CloseConfirm);
    await Promise.resolve();
    closeRequestedCallback?.(false);
    await waitFor(() => screen.getByRole('alertdialog'));

    await fireEvent.click(screen.getByRole('button', { name: 'Vẫn thoát' }));

    expect(mocks.appStore.confirmClose).toHaveBeenCalledTimes(1);
    expect(mocks.notesStore.flushAll).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  it('busy=true shows the Job dialog and dismisses on "stay"', async () => {
    render(CloseConfirm);
    await Promise.resolve();

    closeRequestedCallback?.(true);
    await Promise.resolve();

    const dialog = screen.getByRole('alertdialog');
    expect(dialog).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Ở lại' }));
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(mocks.appStore.confirmClose).not.toHaveBeenCalled();
  });

  it('busy=true: confirming flushes notes then confirms close', async () => {
    render(CloseConfirm);
    await Promise.resolve();
    closeRequestedCallback?.(true);
    await Promise.resolve();

    await fireEvent.click(screen.getByRole('button', { name: 'Huỷ và thoát' }));

    expect(mocks.notesStore.flushAll).toHaveBeenCalledTimes(1);
    expect(mocks.appStore.confirmClose).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });
});
