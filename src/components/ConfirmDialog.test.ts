// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import ConfirmDialog from './ConfirmDialog.svelte';

afterEach(() => cleanup());

function props(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    title: 'Xoá phiên này?',
    body: 'Không thể hoàn tác.',
    confirmLabel: 'Xoá phiên',
    cancelLabel: 'Huỷ',
    onConfirm: vi.fn(),
    onCancel: vi.fn(),
    ...overrides,
  };
}

describe('ConfirmDialog', () => {
  it('renders the title and body, focusing the cancel button on mount (UX-DR16)', () => {
    render(ConfirmDialog, props());

    expect(screen.getByRole('alertdialog')).toBeTruthy();
    expect(screen.getByText('Xoá phiên này?')).toBeTruthy();
    expect(screen.getByText('Không thể hoàn tác.')).toBeTruthy();
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Huỷ' }));
  });

  it('the danger action is the confirm button, on the right', () => {
    render(ConfirmDialog, props());
    const buttons = screen.getAllByRole('button');
    expect(buttons.map((b) => b.textContent?.trim())).toEqual(['Huỷ', 'Xoá phiên']);
  });

  it('clicking confirm/cancel calls the matching callback', async () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(ConfirmDialog, props({ onConfirm, onCancel }));

    await fireEvent.click(screen.getByRole('button', { name: 'Xoá phiên' }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(onCancel).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole('button', { name: 'Huỷ' }));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it('Escape calls onCancel', async () => {
    const onCancel = vi.fn();
    render(ConfirmDialog, props({ onCancel }));

    await fireEvent.keyDown(screen.getByRole('alertdialog'), { key: 'Escape' });
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it('disables both buttons while confirming', () => {
    render(ConfirmDialog, props({ confirming: true }));
    for (const button of screen.getAllByRole('button')) {
      expect((button as HTMLButtonElement).disabled).toBe(true);
    }
  });
});
