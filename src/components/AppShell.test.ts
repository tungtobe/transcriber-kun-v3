// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, cleanup, render, screen } from '@testing-library/svelte';
import AppShell from './AppShell.svelte';

const mocks = vi.hoisted(() => ({
  settingsStore: {
    theme: 'system' as const,
    error: null as null | { category: string; code: string; detailRedacted: string },
    setTheme: vi.fn(),
    load: vi.fn(),
  },
  appStore: {
    version: { status: 'ok' as const, version: '0.1.0' },
  },
}));

vi.mock('../lib/stores/settings.svelte', () => ({ settingsStore: mocks.settingsStore }));
vi.mock('../lib/stores/app.svelte', () => ({ appStore: mocks.appStore }));

afterEach(() => cleanup());

beforeEach(() => {
  window.history.replaceState({}, '', '/home');
  window.dispatchEvent(new PopStateEvent('popstate', { state: {} }));
  mocks.settingsStore.theme = 'system';
  mocks.settingsStore.error = null;
  mocks.settingsStore.setTheme.mockReset();
  mocks.settingsStore.load.mockReset();
});

describe('AppShell', () => {
  it('always renders accessible shell landmarks and theme control', () => {
    render(AppShell);

    expect(screen.getByRole('complementary', { name: 'Thanh điều hướng chính' })).toBeTruthy();
    expect(screen.getByRole('navigation', { name: 'Điều hướng' })).toBeTruthy();
    expect(screen.getByRole('banner')).toBeTruthy();
    expect(screen.getByRole('main', { name: 'Trang chủ' })).toBeTruthy();
    expect(screen.getByRole('combobox', { name: 'Chủ đề giao diện' })).toBeTruthy();
    expect(screen.getByText('Không có job nào đang chạy.')).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Trang chủ' }).getAttribute('aria-current')).toBe('page');
    expect(screen.getByRole('button', { name: 'Live — sẽ có ở story Live' }).getAttribute('aria-disabled')).toBe('true');
  });

  it('renders a generic typed persistence error and can reload settings', async () => {
    mocks.settingsStore.error = { category: 'storage', code: 'storage', detailRedacted: 'save failed' };
    render(AppShell);

    expect(screen.getByText('Không thể đồng bộ cài đặt')).toBeTruthy();
    expect(screen.getByText('Giá trị hiển thị có thể chưa được lưu. Tải lại để đồng bộ cài đặt.')).toBeTruthy();
    expect(screen.queryByText(/theme hệ thống/i)).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Tải lại' }));
    expect(mocks.settingsStore.load).toHaveBeenCalledTimes(1);
  });

  it('routes a rendered theme-select change through the settings store', async () => {
    render(AppShell);

    await fireEvent.change(screen.getByRole('combobox', { name: 'Chủ đề giao diện' }), {
      target: { value: 'dark' },
    });

    expect(mocks.settingsStore.setTheme).toHaveBeenCalledWith('dark');
  });

});
