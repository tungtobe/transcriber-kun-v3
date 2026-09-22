// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import { configureRouter } from '../lib/router';
import Home from './Home.svelte';

const mocks = vi.hoisted(() => ({
  keysStore: {
    hasUsableKey: false,
    status: 'ready' as 'idle' | 'loading' | 'ready' | 'error',
    load: vi.fn(),
  },
}));

vi.mock('../lib/stores/keys.svelte', () => ({ keysStore: mocks.keysStore }));

afterEach(() => cleanup());

beforeEach(() => {
  configureRouter();
  i18n.applyPreference('system');
  mocks.keysStore.hasUsableKey = false;
  mocks.keysStore.status = 'ready';
  mocks.keysStore.load.mockReset().mockResolvedValue(undefined);
});

describe('Home locale rendering', () => {
  it.each([
    ['en', 'Home', 'Drop a file here'],
    ['ja', 'ホーム', 'ここにファイルをドロップ'],
  ] as const)('renders representative copy in %s', (locale, heading, fileTitle) => {
    i18n.applyPreference(locale);
    render(Home);

    expect(screen.getByRole('heading', { name: heading })).toBeTruthy();
    expect(screen.getByRole('heading', { name: fileTitle })).toBeTruthy();
  });
});

describe('Home missing-key banner', () => {
  beforeEach(() => i18n.applyPreference('vi'));

  it('loads the keys store on mount', () => {
    render(Home);

    expect(mocks.keysStore.load).toHaveBeenCalledTimes(1);
  });

  it('shows a warning banner with a link to Settings → Gemini when there is no usable key', () => {
    mocks.keysStore.hasUsableKey = false;
    mocks.keysStore.status = 'ready';
    render(Home);

    expect(screen.getByText('Chưa có key Gemini hợp lệ')).toBeTruthy();
    const action = screen.getByRole('link', { name: 'Nhập key' });
    expect(action.getAttribute('href')).toBe('/settings/gemini');
  });

  it('shows no banner once a usable key is confirmed present', () => {
    mocks.keysStore.hasUsableKey = true;
    mocks.keysStore.status = 'ready';
    render(Home);

    expect(screen.queryByText('Chưa có key Gemini hợp lệ')).toBeNull();
    expect(screen.queryByRole('status', { name: /key/i })).toBeNull();
  });

  it('does not flash the banner while the key list is still loading', () => {
    mocks.keysStore.hasUsableKey = false;
    mocks.keysStore.status = 'loading';
    render(Home);

    expect(screen.queryByText('Chưa có key Gemini hợp lệ')).toBeNull();
  });
});

describe('Home disabled actions', () => {
  beforeEach(() => i18n.applyPreference('vi'));

  it('renders the file and Live actions as aria-disabled, focusable, non-activating controls', async () => {
    render(Home);

    const fileAction = screen.getByRole('button', { name: 'Chọn file' });
    const liveAction = screen.getByRole('button', { name: 'Bắt đầu Live' });

    for (const action of [fileAction, liveAction]) {
      expect(action.getAttribute('aria-disabled')).toBe('true');
      expect(action.hasAttribute('disabled')).toBe(false);
      action.focus();
      expect(document.activeElement).toBe(action);
      await fireEvent.click(action);
      expect(action.getAttribute('aria-disabled')).toBe('true');
    }
  });
});
