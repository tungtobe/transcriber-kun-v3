// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { configureRouter } from '../lib/router';
import Banner from './Banner.svelte';

afterEach(() => cleanup());
beforeEach(() => configureRouter());

describe('Banner', () => {
  it('renders the three required parts: category, cause+fix, one action', () => {
    render(Banner, {
      variant: 'warning',
      title: 'Chưa có key Gemini hợp lệ',
      message: 'Thêm key để bật tính năng.',
      actionLabel: 'Nhập key',
      actionHref: '/settings/gemini',
    });

    expect(screen.getByRole('status')).toBeTruthy();
    expect(screen.getByText('Chưa có key Gemini hợp lệ')).toBeTruthy();
    expect(screen.getByText('Thêm key để bật tính năng.')).toBeTruthy();
    const action = screen.getByRole('link', { name: 'Nhập key' });
    expect(action.getAttribute('href')).toBe('/settings/gemini');
  });

  it('renders a button action and invokes the callback instead of navigating when there is no href', async () => {
    const onAction = vi.fn();
    render(Banner, {
      variant: 'danger',
      title: 'Lỗi',
      message: 'Có sự cố.',
      actionLabel: 'Thử lại',
      onAction,
    });

    const action = screen.getByRole('button', { name: 'Thử lại' });
    await fireEvent.click(action);
    expect(onAction).toHaveBeenCalledTimes(1);
  });

  it('renders with no action when actionLabel is omitted', () => {
    render(Banner, { variant: 'info', title: 'Thông tin', message: 'Chỉ để biết.' });

    expect(screen.queryByRole('link')).toBeNull();
    expect(screen.queryByRole('button')).toBeNull();
  });
});
