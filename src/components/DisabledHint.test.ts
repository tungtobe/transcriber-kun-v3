// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import DisabledHintHarness from './DisabledHint.test-harness.svelte';

afterEach(() => cleanup());

describe('DisabledHint', () => {
  it('is aria-disabled, stays reachable by Tab, and shows opacity styling', () => {
    render(DisabledHintHarness, {
      reason: 'Chưa có key Gemini',
      shortcut: 'Nhập key trong Cài đặt',
      label: 'Bắt đầu Live',
    });

    const trigger = screen.getByRole('button', { name: 'Bắt đầu Live' });
    expect(trigger.getAttribute('aria-disabled')).toBe('true');
    expect(trigger.hasAttribute('disabled')).toBe(false);
    expect(trigger.tabIndex).not.toBe(-1);

    trigger.focus();
    expect(document.activeElement).toBe(trigger);
  });

  it('exposes the reason and shortcut through an associated tooltip', () => {
    render(DisabledHintHarness, {
      reason: 'Chưa có key Gemini',
      shortcut: 'Nhập key trong Cài đặt',
      label: 'Bắt đầu Live',
    });

    const trigger = screen.getByRole('button', { name: 'Bắt đầu Live' });
    const describedBy = trigger.getAttribute('aria-describedby');
    expect(describedBy).toBeTruthy();
    const tooltip = document.getElementById(describedBy!);
    expect(tooltip?.getAttribute('role')).toBe('tooltip');
    expect(tooltip?.textContent).toContain('Chưa có key Gemini');
    expect(tooltip?.textContent).toContain('Nhập key trong Cài đặt');
  });

  it('never activates on click', async () => {
    render(DisabledHintHarness, { reason: 'Chưa có key Gemini', label: 'Bắt đầu Live' });

    const trigger = screen.getByRole('button', { name: 'Bắt đầu Live' });
    await fireEvent.click(trigger);

    expect(trigger.getAttribute('aria-disabled')).toBe('true');
  });
});
