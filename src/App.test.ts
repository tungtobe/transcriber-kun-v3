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
// story 2.9: `AppShell`/`Home` now subscribe `jobsStore` (a real
// `@tauri-apps/api` Channel, which has no window to bind to outside a real
// Tauri runtime) and `libraryStore` (a real IPC call) on mount — this suite
// composes the real shell/router so it stubs both domain stores wholesale,
// the same way it already stubs `settingsStore`/`appStore`.
vi.mock('./lib/stores/jobs.svelte', () => ({
  jobsStore: {
    jobs: new Map(),
    resultSeq: 0,
    synced: true,
    status: 'subscribed',
    subscribe: vi.fn(() => Promise.resolve()),
    unsubscribe: vi.fn(),
    cancel: vi.fn(),
  },
}));
vi.mock('./lib/stores/library.svelte', () => ({
  libraryStore: {
    sessions: [],
    status: 'ready',
    error: null,
    reloadError: false,
    load: vi.fn(() => Promise.resolve()),
  },
}));
// AppShell registers a webview-wide drag-drop listener (story 2.8) — the
// real `@tauri-apps/api/webview` has no window to bind to outside a real
// Tauri runtime, so this suite (composing the real App shell) mocks the
// same seam module AppShell.test.ts mocks.
vi.mock('./lib/dragdrop', () => ({
  onDragDropEvent: vi.fn(() => Promise.resolve(() => {})),
}));

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
