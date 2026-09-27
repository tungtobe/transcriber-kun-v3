// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import WipeAllDialog from './WipeAllDialog.svelte';

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
});

function props(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    onConfirm: vi.fn(),
    onCancel: vi.fn(),
    ...overrides,
  };
}

describe('WipeAllDialog', () => {
  it('opens on step 1, listing what gets deleted and what is kept, focused on Huỷ', () => {
    render(WipeAllDialog, props());

    expect(screen.getByRole('alertdialog')).toBeTruthy();
    expect(screen.getByText('Xoá toàn bộ dữ liệu?')).toBeTruthy();
    expect(screen.getByText('Sẽ xoá:')).toBeTruthy();
    expect(screen.getByText('Vẫn giữ:')).toBeTruthy();
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Huỷ' }));
    // Step 1 never shows the danger confirm button yet (spec: "đi qua hai bước").
    expect(screen.queryByRole('button', { name: 'Xoá toàn bộ dữ liệu' })).toBeNull();
  });

  it('"Tiếp tục" advances to step 2 without calling onConfirm', async () => {
    const onConfirm = vi.fn();
    render(WipeAllDialog, props({ onConfirm }));

    await fireEvent.click(screen.getByRole('button', { name: 'Tiếp tục' }));

    expect(screen.getByText('Xác nhận xoá toàn bộ dữ liệu?')).toBeTruthy();
    expect(onConfirm).not.toHaveBeenCalled();
    const confirmButton = screen.getByRole('button', { name: 'Xoá toàn bộ dữ liệu' });
    expect(confirmButton.className).toContain('button-danger-soft');
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Huỷ' }));
  });

  it('clicking the final confirm on step 2 calls onConfirm', async () => {
    const onConfirm = vi.fn();
    render(WipeAllDialog, props({ onConfirm }));
    await fireEvent.click(screen.getByRole('button', { name: 'Tiếp tục' }));

    await fireEvent.click(screen.getByRole('button', { name: 'Xoá toàn bộ dữ liệu' }));

    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it('Huỷ on step 1 calls onCancel', async () => {
    const onCancel = vi.fn();
    render(WipeAllDialog, props({ onCancel }));

    await fireEvent.click(screen.getByRole('button', { name: 'Huỷ' }));

    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it('Huỷ on step 2 calls onCancel (closes entirely, not "back")', async () => {
    const onCancel = vi.fn();
    render(WipeAllDialog, props({ onCancel }));
    await fireEvent.click(screen.getByRole('button', { name: 'Tiếp tục' }));

    await fireEvent.click(screen.getByRole('button', { name: 'Huỷ' }));

    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it('Escape calls onCancel on either step', async () => {
    const onCancel = vi.fn();
    render(WipeAllDialog, props({ onCancel }));

    await fireEvent.keyDown(screen.getByRole('alertdialog'), { key: 'Escape' });
    expect(onCancel).toHaveBeenCalledTimes(1);

    await fireEvent.click(screen.getByRole('button', { name: 'Tiếp tục' }));
    await fireEvent.keyDown(screen.getByRole('alertdialog'), { key: 'Escape' });
    expect(onCancel).toHaveBeenCalledTimes(2);
  });

  it('disables both step-2 buttons while confirming', async () => {
    render(WipeAllDialog, props({ confirming: true }));
    await fireEvent.click(screen.getByRole('button', { name: 'Tiếp tục' }));

    for (const button of screen.getAllByRole('button')) {
      expect((button as HTMLButtonElement).disabled).toBe(true);
    }
    expect(screen.getByText('Đang xoá…')).toBeTruthy();
  });
});
