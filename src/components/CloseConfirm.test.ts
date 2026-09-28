// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import CloseConfirm from './CloseConfirm.svelte';

const mocks = vi.hoisted(() => ({
  appStore: {
    listenForCloseRequested: vi.fn(),
    confirmClose: vi.fn(),
    stayOpen: vi.fn(),
  },
  notesStore: { flushAll: vi.fn() },
}));

vi.mock('../lib/stores/app.svelte', () => ({ appStore: mocks.appStore }));
vi.mock('../lib/stores/notes.svelte', () => ({ notesStore: mocks.notesStore }));

let closeRequestedCallback: ((request: { liveBusy: boolean; jobBusy: boolean }) => void) | null = null;
const unlisten = vi.fn();

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  closeRequestedCallback = null;
  unlisten.mockReset();
  mocks.appStore.listenForCloseRequested.mockReset().mockImplementation(
    (cb: (request: { liveBusy: boolean; jobBusy: boolean }) => void) => {
      closeRequestedCallback = cb;
      return Promise.resolve(unlisten);
    },
  );
  mocks.appStore.confirmClose.mockReset().mockResolvedValue(null);
  mocks.appStore.stayOpen.mockReset().mockResolvedValue(undefined);
  mocks.notesStore.flushAll.mockReset().mockResolvedValue(true);
});

async function requestClose(request: { liveBusy: boolean; jobBusy: boolean }): Promise<void> {
  await waitFor(() => expect(closeRequestedCallback).not.toBeNull());
  closeRequestedCallback!(request);
}

describe('CloseConfirm', () => {
  it('renders nothing until the shared close coordinator emits a request', async () => {
    render(CloseConfirm);
    await Promise.resolve();
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  it('flushes notes and closes an idle app without an extra dialog', async () => {
    render(CloseConfirm);
    await Promise.resolve();

    await requestClose({ liveBusy: false, jobBusy: false });
    await waitFor(() => expect(mocks.appStore.confirmClose).toHaveBeenCalledTimes(1));

    expect(mocks.notesStore.flushAll).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  it('shows the Live prompt and saves Live when confirmed', async () => {
    render(CloseConfirm);
    await Promise.resolve();
    await requestClose({ liveBusy: true, jobBusy: false });

    expect(screen.getByRole('alertdialog')).toBeTruthy();
    expect(screen.getByText('Phiên đang ghi — dừng và lưu trước khi thoát?')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Dừng, lưu và thoát' }));

    expect(mocks.notesStore.flushAll).toHaveBeenCalledTimes(1);
    expect(mocks.appStore.confirmClose).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  it('asks once for Live plus Job and confirms both through the same action', async () => {
    render(CloseConfirm);
    await Promise.resolve();
    await requestClose({ liveBusy: true, jobBusy: true });

    expect(screen.getByRole('alertdialog')).toBeTruthy();
    expect(screen.getByText('Phiên đang ghi sẽ được lưu; các tác vụ transcribe đang chạy hoặc chờ sẽ bị huỷ.')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Lưu phiên, huỷ tác vụ và thoát' }));

    expect(mocks.appStore.confirmClose).toHaveBeenCalledTimes(1);
    expect(mocks.notesStore.flushAll).toHaveBeenCalledTimes(1);
  });

  it('shows the Job prompt and stay leaves the app open', async () => {
    render(CloseConfirm);
    await Promise.resolve();
    await requestClose({ liveBusy: false, jobBusy: true });

    await fireEvent.click(screen.getByRole('button', { name: 'Ở lại' }));
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(mocks.appStore.confirmClose).not.toHaveBeenCalled();
    expect(mocks.appStore.stayOpen).toHaveBeenCalledTimes(1);
  });

  it('keeps the app open when note flush fails and offers retry or stay only', async () => {
    mocks.notesStore.flushAll.mockResolvedValue(false);
    render(CloseConfirm);
    await Promise.resolve();
    await requestClose({ liveBusy: false, jobBusy: false });

    await waitFor(() => expect(screen.getByRole('alertdialog')).toBeTruthy());
    expect(mocks.appStore.confirmClose).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Ở lại' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Thử lưu và thoát lại' })).toBeTruthy();
    expect(screen.queryByText('Vẫn thoát')).toBeNull();
  });

  it('keeps the dialog open when Live metadata commit fails, then permits a retry', async () => {
    mocks.appStore.confirmClose.mockResolvedValueOnce({ category: 'storage', code: 'storage' });
    render(CloseConfirm);
    await Promise.resolve();
    await requestClose({ liveBusy: true, jobBusy: false });
    await fireEvent.click(screen.getByRole('button', { name: 'Dừng, lưu và thoát' }));

    expect(await screen.findByRole('alertdialog')).toBeTruthy();
    expect(mocks.appStore.confirmClose).toHaveBeenCalledTimes(1);
    await fireEvent.click(screen.getByRole('button', { name: 'Thử lưu và thoát lại' }));
    expect(mocks.appStore.confirmClose).toHaveBeenCalledTimes(2);
  });
});
