// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, cleanup, render, screen } from '@testing-library/svelte';
import AppShell from './AppShell.svelte';
import { i18n } from '../i18n/index.svelte';
import { configureRouter } from '../lib/router';
import type { DragDropEvent } from '../lib/dragdrop';

const mocks = vi.hoisted(() => ({
  settingsStore: {
    theme: 'system' as const,
    error: null as null | { category: string; code: string; detailRedacted: string },
    setTheme: vi.fn(),
    load: vi.fn(),
  },
  appStore: {
    version: { status: 'ok' as const, version: '0.1.0' },
    listenForCloseRequested: vi.fn(() => Promise.resolve(() => {})),
    confirmClose: vi.fn(() => Promise.resolve()),
  },
  intakeStore: {
    submit: vi.fn(() => Promise.resolve()),
  },
  onDragDropEvent: vi.fn(),
  jobsStore: {
    jobs: new Map<string, unknown>(),
    resultSeq: 0,
    subscribe: vi.fn(() => Promise.resolve()),
    unsubscribe: vi.fn(),
    cancel: vi.fn(() => Promise.resolve('cancelling')),
  },
  liveStore: {
    snapshot: { sessionId: null, transcriptId: null, recording: 'stopped', connection: { type: 'stopped' }, transcription: 'stopped', errorCategory: null, durationSec: null },
    subscribe: vi.fn(() => Promise.resolve()),
    unsubscribe: vi.fn(),
  },
}));

vi.mock('../lib/stores/settings.svelte', () => ({ settingsStore: mocks.settingsStore }));
vi.mock('../lib/stores/app.svelte', () => ({ appStore: mocks.appStore }));
vi.mock('../lib/stores/intake.svelte', () => ({ intakeStore: mocks.intakeStore }));
vi.mock('../lib/stores/jobs.svelte', () => ({ jobsStore: mocks.jobsStore }));
vi.mock('../lib/stores/live.svelte', () => ({ liveStore: mocks.liveStore }));
vi.mock('../lib/dragdrop', () => ({
  onDragDropEvent: (...args: unknown[]) => mocks.onDragDropEvent(...args),
}));

let dragDropHandler: ((event: DragDropEvent) => void) | null = null;

afterEach(() => cleanup());

beforeEach(() => {
  configureRouter();
  i18n.applyPreference('vi');
  window.history.replaceState({}, '', '/home');
  window.dispatchEvent(new PopStateEvent('popstate', { state: {} }));
  mocks.settingsStore.theme = 'system';
  mocks.settingsStore.error = null;
  mocks.settingsStore.setTheme.mockReset();
  mocks.settingsStore.load.mockReset();
  mocks.intakeStore.submit.mockReset().mockResolvedValue(undefined);
  mocks.jobsStore.jobs = new Map();
  mocks.jobsStore.subscribe.mockReset().mockResolvedValue(undefined);
  mocks.jobsStore.unsubscribe.mockReset();
  mocks.jobsStore.cancel.mockReset().mockResolvedValue('cancelling');
  mocks.liveStore.snapshot.recording = 'stopped';
  mocks.liveStore.subscribe.mockReset().mockResolvedValue(undefined);
  mocks.liveStore.unsubscribe.mockReset();
  dragDropHandler = null;
  mocks.onDragDropEvent.mockReset().mockImplementation((handler: (event: DragDropEvent) => void) => {
    dragDropHandler = handler;
    return Promise.resolve(() => {});
  });
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
    expect(screen.getByRole('link', { name: 'Live' }).getAttribute('href')).toBe('/live');
  });

  it('subscribes jobsStore on mount and unsubscribes on unmount (story 2.9)', () => {
    const view = render(AppShell);
    expect(mocks.jobsStore.subscribe).toHaveBeenCalledTimes(1);

    view.unmount();
    expect(mocks.jobsStore.unsubscribe).toHaveBeenCalledTimes(1);
  });

  it('subscribes Live at the shell and marks its navigation entry while recording', () => {
    mocks.liveStore.snapshot.recording = 'active';
    render(AppShell);

    expect(mocks.liveStore.subscribe).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('link', { name: /Live/ }).querySelector('.live-nav-indicator')).toBeTruthy();
  });

  it('shows the running Job name and percent in the compact sidebar card, linking to /session/:id (story 2.9)', () => {
    mocks.jobsStore.jobs = new Map([
      [
        'j1',
        {
          jobId: 'j1',
          sessionId: 'session-1',
          sourceName: 'meeting.wav',
          kind: 'transcribe',
          state: 'running',
          processedMs: 30 * 60_000,
          totalMs: 90 * 60_000,
          chunkIndex: 6,
          chunkCount: 18,
          keyOrdinal: 2,
          attempt: 1,
          waitingQuota: false,
        },
      ],
      [
        'j2',
        {
          jobId: 'j2',
          sessionId: 'session-2',
          sourceName: 'queued.wav',
          kind: 'transcribe',
          state: 'queued',
          processedMs: 0,
          totalMs: 1,
          chunkIndex: 0,
          chunkCount: 1,
          keyOrdinal: null,
          attempt: null,
          waitingQuota: false,
        },
      ],
    ]);
    render(AppShell);

    expect(screen.queryByText('Không có job nào đang chạy.')).toBeNull();
    expect(screen.getByText('meeting.wav')).toBeTruthy();
    expect(screen.getByText('33 %')).toBeTruthy();
    const link = screen.getByRole('link', { name: /meeting\.wav/ });
    expect(link.getAttribute('href')).toBe('/session/session-1');
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

  it.each([
    ['en', 'Primary navigation sidebar', 'Home', 'quiet utility'],
    ['ja', 'メインナビゲーションのサイドバー', 'ホーム', '静かなユーティリティ'],
  ] as const)('renders shell copy and landmarks in %s', (locale, navLabel, heading, caption) => {
    i18n.applyPreference(locale);
    render(AppShell);

    expect(screen.getByRole('complementary', { name: navLabel })).toBeTruthy();
    expect(screen.getByRole('heading', { name: heading })).toBeTruthy();
    expect(screen.getByText(caption)).toBeTruthy();
  });

  describe('drag-and-drop intake (story 2.8)', () => {
    it('shows the drop overlay while dragging over /home and hides it again on drop', async () => {
      render(AppShell);
      expect(dragDropHandler).not.toBeNull();

      dragDropHandler!({ payload: { type: 'enter', paths: [], position: { x: 0, y: 0 } } });
      await Promise.resolve();
      expect(screen.getByText('Thả file vào đây để bắt đầu transcribe')).toBeTruthy();

      dragDropHandler!({
        payload: { type: 'drop', paths: ['/tmp/a.mp4'], position: { x: 0, y: 0 } },
      });
      await Promise.resolve();
      expect(screen.queryByText('Thả file vào đây để bắt đầu transcribe')).toBeNull();
      expect(mocks.intakeStore.submit).toHaveBeenCalledWith(['/tmp/a.mp4']);
    });

    it('never shows the overlay or submits a drop outside /home and /session/:id', async () => {
      window.history.replaceState({}, '', '/settings/general');
      window.dispatchEvent(new PopStateEvent('popstate', { state: {} }));
      render(AppShell);
      expect(dragDropHandler).not.toBeNull();

      dragDropHandler!({ payload: { type: 'enter', paths: [], position: { x: 0, y: 0 } } });
      await Promise.resolve();
      expect(screen.queryByText('Thả file vào đây để bắt đầu transcribe')).toBeNull();

      dragDropHandler!({
        payload: { type: 'drop', paths: ['/tmp/a.mp4'], position: { x: 0, y: 0 } },
      });
      await Promise.resolve();
      expect(mocks.intakeStore.submit).not.toHaveBeenCalled();
    });

    it('accepts a drop on /session/:id', async () => {
      window.history.replaceState({}, '', '/session/abc');
      window.dispatchEvent(new PopStateEvent('popstate', { state: {} }));
      render(AppShell);
      expect(dragDropHandler).not.toBeNull();

      dragDropHandler!({
        payload: { type: 'drop', paths: ['/tmp/b.m4a'], position: { x: 0, y: 0 } },
      });
      await Promise.resolve();
      expect(mocks.intakeStore.submit).toHaveBeenCalledWith(['/tmp/b.m4a']);
    });
  });
});
