// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/svelte';
import App from './App.svelte';
import { configureRouter } from './lib/router';
import { i18n } from './i18n/index.svelte';

const mocks = vi.hoisted(() => ({
  removeKeymap: vi.fn(),
  installKeymap: vi.fn(),
  settingsStore: {
    theme: 'system' as const,
    error: null,
    setTheme: vi.fn(),
    load: vi.fn(),
    destroy: vi.fn(),
  },
  appStore: {
    version: { status: 'ok' as const, version: '0.1.0' },
    loadVersion: vi.fn(),
    listenForCloseRequested: vi.fn(() => Promise.resolve(() => {})),
    confirmClose: vi.fn(() => Promise.resolve()),
  },
}));

vi.mock('./lib/keymap', () => ({
  installKeymap: (...args: unknown[]) => mocks.installKeymap(...args),
}));
vi.mock('./lib/stores/settings.svelte', () => ({ settingsStore: mocks.settingsStore }));
vi.mock('./lib/stores/app.svelte', () => ({ appStore: mocks.appStore }));

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  configureRouter();
  window.scrollTo = vi.fn();
  window.history.replaceState({}, '', '/home');
  window.dispatchEvent(new PopStateEvent('popstate', { state: {} }));
  mocks.installKeymap.mockReset().mockReturnValue(mocks.removeKeymap);
  mocks.removeKeymap.mockReset();
  mocks.settingsStore.destroy.mockReset();
  mocks.settingsStore.setTheme.mockReset();
  mocks.settingsStore.load.mockReset();
  mocks.appStore.loadVersion.mockReset();
});

describe('App composition', () => {
  it('renders the production shell and router together, then cleans up both listeners', async () => {
    const view = render(App);

    expect(screen.getByRole('complementary', { name: 'Thanh điều hướng chính' })).toBeTruthy();
    expect(screen.getByRole('heading', { name: 'Trang chủ' })).toBeTruthy();
    expect(await screen.findByText('Kéo file vào đây')).toBeTruthy();
    expect(mocks.appStore.loadVersion).toHaveBeenCalledTimes(1);
    expect(mocks.installKeymap).toHaveBeenCalledTimes(1);

    view.unmount();

    expect(mocks.removeKeymap).toHaveBeenCalledTimes(1);
    expect(mocks.settingsStore.destroy).toHaveBeenCalledTimes(1);
  });
});
