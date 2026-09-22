// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import HelpTip from './HelpTip.svelte';

afterEach(() => cleanup());

describe('HelpTip', () => {
  it('is reachable by Tab and describes the tooltip content via aria-describedby', () => {
    render(HelpTip, { props: { label: 'Ngôn ngữ', text: 'Đổi ngôn ngữ hiển thị.' } });

    const trigger = screen.getByRole('button');
    expect(trigger.tabIndex).not.toBe(-1);
    const describedBy = trigger.getAttribute('aria-describedby');
    expect(describedBy).toBeTruthy();
    const tooltip = document.getElementById(describedBy!);
    expect(tooltip?.getAttribute('role')).toBe('tooltip');
    expect(tooltip?.textContent?.trim()).toBe('Đổi ngôn ngữ hiển thị.');
  });

  it('folds the field label into the trigger accessible name', () => {
    render(HelpTip, { props: { label: 'Ngôn ngữ', text: 'Đổi ngôn ngữ hiển thị.' } });

    const trigger = screen.getByRole('button');
    expect(trigger.getAttribute('aria-label')).toContain('Ngôn ngữ');
  });

  it('opens the tooltip on focus and closes it again on blur', async () => {
    render(HelpTip, { props: { label: 'Giao diện', text: 'Đổi bảng màu.' } });
    const trigger = screen.getByRole('button');

    trigger.focus();
    await fireEvent.focus(trigger);
    expect(trigger.nextElementSibling?.classList.contains('visible')).toBe(true);

    await fireEvent.blur(trigger);
    expect(trigger.nextElementSibling?.classList.contains('visible')).toBe(false);
  });

  it('opens the tooltip on hover (mouseenter) and closes it on mouseleave', async () => {
    render(HelpTip, { props: { label: 'Giao diện', text: 'Đổi bảng màu.' } });
    const trigger = screen.getByRole('button');

    await fireEvent.mouseEnter(trigger);
    expect(trigger.nextElementSibling?.classList.contains('visible')).toBe(true);

    await fireEvent.mouseLeave(trigger);
    expect(trigger.nextElementSibling?.classList.contains('visible')).toBe(false);
  });

  it('closes on Escape while the pointer is still hovering', async () => {
    render(HelpTip, { props: { label: 'Giao diện', text: 'Đổi bảng màu.' } });
    const trigger = screen.getByRole('button');

    await fireEvent.mouseEnter(trigger);
    expect(trigger.nextElementSibling?.classList.contains('visible')).toBe(true);

    await fireEvent.keyDown(trigger, { key: 'Escape' });
    expect(trigger.nextElementSibling?.classList.contains('visible')).toBe(false);
  });
});
