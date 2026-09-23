// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import CloseConfirm from './CloseConfirm.svelte';

const mocks = vi.hoisted(() => ({
  appStore: {
    listenForCloseRequested: vi.fn(),
    confirmClose: vi.fn(),
  },
}));

vi.mock('../lib/stores/app.svelte', () => ({ appStore: mocks.appStore }));

let closeRequestedCallback: (() => void) | null = null;
const unlisten = vi.fn();

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  closeRequestedCallback = null;
  unlisten.mockReset();
  mocks.appStore.listenForCloseRequested.mockReset().mockImplementation((cb: () => void) => {
    closeRequestedCallback = cb;
    return Promise.resolve(unlisten);
  });
  mocks.appStore.confirmClose.mockReset().mockResolvedValue(undefined);
});

describe('CloseConfirm', () => {
  it('renders nothing until CloseRequested fires', async () => {
    render(CloseConfirm);
    await Promise.resolve();

    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  it('shows the dialog on CloseRequested and dismisses on "stay"', async () => {
    render(CloseConfirm);
    await Promise.resolve();

    closeRequestedCallback?.();
    await Promise.resolve();

    const dialog = screen.getByRole('alertdialog');
    expect(dialog).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Ở lại' }));
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(mocks.appStore.confirmClose).not.toHaveBeenCalled();
  });

  it('calls confirmClose and hides the dialog when the user confirms', async () => {
    render(CloseConfirm);
    await Promise.resolve();
    closeRequestedCallback?.();
    await Promise.resolve();

    await fireEvent.click(screen.getByRole('button', { name: 'Huỷ và thoát' }));

    expect(mocks.appStore.confirmClose).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });
});
