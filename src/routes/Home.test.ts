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
  intakeStore: {
    notices: [] as unknown[],
    pick: vi.fn(),
    dismiss: vi.fn(),
  },
}));

vi.mock('../lib/stores/keys.svelte', () => ({ keysStore: mocks.keysStore }));
vi.mock('../lib/stores/intake.svelte', () => ({ intakeStore: mocks.intakeStore }));

afterEach(() => cleanup());

beforeEach(() => {
  configureRouter();
  i18n.applyPreference('system');
  mocks.keysStore.hasUsableKey = false;
  mocks.keysStore.status = 'ready';
  mocks.keysStore.load.mockReset().mockResolvedValue(undefined);
  mocks.intakeStore.notices = [];
  mocks.intakeStore.pick.mockReset().mockResolvedValue(undefined);
  mocks.intakeStore.dismiss.mockReset();
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

  it('shows the banner after a list error even if an old key is cached', () => {
    mocks.keysStore.hasUsableKey = true;
    mocks.keysStore.status = 'error';
    render(Home);

    expect(screen.getByText('Chưa có key Gemini hợp lệ')).toBeTruthy();
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

  it('renders Live as an aria-disabled, focusable, non-activating control', async () => {
    render(Home);

    const liveAction = screen.getByRole('button', { name: 'Bắt đầu Live' });

    expect(liveAction.getAttribute('aria-disabled')).toBe('true');
    expect(liveAction.hasAttribute('disabled')).toBe(false);
    liveAction.focus();
    expect(document.activeElement).toBe(liveAction);
    await fireEvent.click(liveAction);
    expect(liveAction.getAttribute('aria-disabled')).toBe('true');
    expect(mocks.intakeStore.pick).not.toHaveBeenCalled();
  });

  it('renders both "Chọn file" actions as aria-disabled with a tooltip when there is no usable key', async () => {
    mocks.keysStore.hasUsableKey = false;
    mocks.keysStore.status = 'ready';
    render(Home);

    const fileActions = screen.getAllByRole('button', { name: 'Chọn file' });
    expect(fileActions).toHaveLength(2);

    for (const action of fileActions) {
      expect(action.getAttribute('aria-disabled')).toBe('true');
      expect(action.hasAttribute('disabled')).toBe(false);
      action.focus();
      expect(document.activeElement).toBe(action);
      await fireEvent.click(action);
    }
    expect(mocks.intakeStore.pick).not.toHaveBeenCalled();
    expect(screen.getAllByText('Thêm key Gemini trước khi chọn file').length).toBeGreaterThan(0);
  });
});

describe('Home file intake (story 2.8)', () => {
  beforeEach(() => i18n.applyPreference('vi'));

  it('enables both "Chọn file" actions and calls intake.pick() once a usable key is present', async () => {
    mocks.keysStore.hasUsableKey = true;
    mocks.keysStore.status = 'ready';
    render(Home);

    const fileActions = screen.getAllByRole('button', { name: 'Chọn file' });
    expect(fileActions).toHaveLength(2);
    for (const action of fileActions) {
      expect(action.getAttribute('aria-disabled')).toBeNull();
    }

    await fireEvent.click(fileActions[0]);
    expect(mocks.intakeStore.pick).toHaveBeenCalledTimes(1);

    await fireEvent.click(fileActions[1]);
    expect(mocks.intakeStore.pick).toHaveBeenCalledTimes(2);
  });

  it('keeps the file actions disabled while the key list is still loading', () => {
    mocks.keysStore.hasUsableKey = false;
    mocks.keysStore.status = 'loading';
    render(Home);

    for (const action of screen.getAllByRole('button', { name: 'Chọn file' })) {
      expect(action.getAttribute('aria-disabled')).toBe('true');
    }
  });

  it('renders intake notices when present', () => {
    mocks.intakeStore.notices = [
      { id: 'n1', variant: 'info', title: 'a.mp4', message: 'Đã thêm vào hàng đợi transcribe.' },
    ];
    render(Home);

    expect(screen.getByText('Đã thêm vào hàng đợi transcribe.')).toBeTruthy();
  });
});
