// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import SettingsRowHarness from './SettingsRow.test-harness.svelte';

afterEach(() => cleanup());

describe('SettingsRow', () => {
  it('renders the label, the control, and a persistent (always-visible) helper line', () => {
    render(SettingsRowHarness, {
      label: 'Ngôn ngữ',
      help: 'Đổi ngôn ngữ hiển thị của ứng dụng.',
      helperText: 'Áp dụng ngay, không cần khởi động lại.',
      fieldId: 'row-language',
      controlText: 'system',
    });

    expect(screen.getByText('Ngôn ngữ')).toBeTruthy();
    expect(screen.getByDisplayValue('system')).toBeTruthy();
    // Helper text renders unconditionally, not behind a hover/focus trigger.
    expect(screen.getByText('Áp dụng ngay, không cần khởi động lại.')).toBeTruthy();
  });

  it('wires the label to the control through `for`/`id`', () => {
    render(SettingsRowHarness, {
      label: 'Giao diện',
      help: 'Đổi bảng màu.',
      helperText: 'Áp dụng ngay.',
      fieldId: 'row-theme',
      controlText: 'dark',
    });

    const label = screen.getByText('Giao diện');
    expect(label.getAttribute('for')).toBe('row-theme');
  });

  it('includes a (?) tooltip trigger reachable by Tab, opening on focus and closing on Escape', async () => {
    render(SettingsRowHarness, {
      label: 'Ngôn ngữ',
      help: 'Đổi ngôn ngữ hiển thị của ứng dụng.',
      helperText: 'Áp dụng ngay.',
      fieldId: 'row-language',
      controlText: 'system',
    });

    const tooltipTrigger = screen.getByRole('button');
    expect(tooltipTrigger.tabIndex).not.toBe(-1);

    await fireEvent.focus(tooltipTrigger);
    const describedBy = tooltipTrigger.getAttribute('aria-describedby');
    const tooltip = document.getElementById(describedBy!);
    expect(tooltip?.getAttribute('role')).toBe('tooltip');
    expect(tooltip?.classList.contains('visible')).toBe(true);

    await fireEvent.keyDown(tooltipTrigger, { key: 'Escape' });
    expect(tooltip?.classList.contains('visible')).toBe(false);
  });
});
