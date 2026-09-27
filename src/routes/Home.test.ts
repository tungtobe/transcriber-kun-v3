// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import { configureRouter } from '../lib/router';
import Home from './Home.svelte';

function job(jobId: string, overrides: Partial<Record<string, unknown>> = {}) {
  return {
    jobId,
    sessionId: `session-${jobId}`,
    sourceName: 'fixture.wav',
    kind: 'transcribe',
    state: 'running',
    processedMs: 30 * 60_000,
    totalMs: 90 * 60_000,
    chunkIndex: 6,
    chunkCount: 18,
    keyOrdinal: 2,
    attempt: 1,
    waitingQuota: false,
    ...overrides,
  };
}

function item(sessionId: string, overrides: Partial<Record<string, unknown>> = {}) {
  return {
    sessionId,
    kind: 'file',
    title: `phiên ${sessionId}`,
    createdAt: 1_000,
    durationSec: 65,
    recovered: false,
    missingGapCount: 0,
    tagIds: [],
    ...overrides,
  };
}

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
  jobsStore: {
    jobs: new Map<string, unknown>(),
    resultSeq: 0,
    synced: true,
    status: 'subscribed' as 'idle' | 'subscribed' | 'error',
    subscribe: vi.fn(() => Promise.resolve()),
    unsubscribe: vi.fn(),
    cancel: vi.fn<() => Promise<'cancelling' | 'alreadyFinished' | null>>(() => Promise.resolve('cancelling')),
  },
  libraryStore: {
    sessions: [] as unknown[],
    status: 'ready' as 'loading' | 'ready' | 'error',
    error: null as null | { category: string; code: string; detailRedacted: string },
    reloadError: false,
    load: vi.fn(() => Promise.resolve()),
    rename: vi.fn(() => Promise.resolve({ status: 'ok', title: 'renamed' })),
    remove: vi.fn(() => Promise.resolve({ status: 'ok', outcome: 'deleted' })),
    // Story 3.2: đọc-only trong test này (không có test nào ở đây lọc theo
    // tag) -- `filteredSessions` trả thẳng `sessions` hiện tại.
    get filteredSessions() {
      return this.sessions;
    },
    tags: [] as unknown[],
    tagFilter: { tagIds: [] as string[], untagged: false },
    loadTags: vi.fn(() => Promise.resolve()),
    createTag: vi.fn(),
    attachTag: vi.fn(),
    detachTag: vi.fn(),
    deleteTagGlobally: vi.fn(),
    toggleFilterTag: vi.fn(),
    toggleUntaggedFilter: vi.fn(),
    clearTagFilter: vi.fn(),
  },
}));

vi.mock('../lib/stores/keys.svelte', () => ({ keysStore: mocks.keysStore }));
vi.mock('../lib/stores/intake.svelte', () => ({ intakeStore: mocks.intakeStore }));
vi.mock('../lib/stores/jobs.svelte', () => ({ jobsStore: mocks.jobsStore }));
vi.mock('../lib/stores/library.svelte', () => ({ libraryStore: mocks.libraryStore }));

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
  mocks.jobsStore.jobs = new Map();
  mocks.jobsStore.synced = true;
  mocks.jobsStore.status = 'subscribed';
  mocks.jobsStore.subscribe.mockReset().mockResolvedValue(undefined);
  mocks.jobsStore.unsubscribe.mockReset();
  mocks.jobsStore.cancel.mockReset().mockResolvedValue('cancelling');
  mocks.libraryStore.sessions = [];
  mocks.libraryStore.status = 'ready';
  mocks.libraryStore.error = null;
  mocks.libraryStore.reloadError = false;
  mocks.libraryStore.load.mockReset().mockResolvedValue(undefined);
  mocks.libraryStore.rename.mockReset().mockResolvedValue({ status: 'ok', title: 'renamed' });
  mocks.libraryStore.remove.mockReset().mockResolvedValue({ status: 'ok', outcome: 'deleted' });
  mocks.libraryStore.tags = [];
  mocks.libraryStore.tagFilter = { tagIds: [], untagged: false };
  mocks.libraryStore.loadTags.mockReset().mockResolvedValue(undefined);
  mocks.libraryStore.createTag.mockReset();
  mocks.libraryStore.attachTag.mockReset();
  mocks.libraryStore.detachTag.mockReset();
  mocks.libraryStore.deleteTagGlobally.mockReset();
  mocks.libraryStore.toggleFilterTag.mockReset();
  mocks.libraryStore.toggleUntaggedFilter.mockReset();
  mocks.libraryStore.clearTagFilter.mockReset();
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

describe('Home mount/unmount subscribes and unsubscribes jobsStore (story 2.9)', () => {
  beforeEach(() => i18n.applyPreference('vi'));

  it('subscribes jobsStore on mount and unsubscribes on unmount, and loads the library once', () => {
    const view = render(Home);
    expect(mocks.jobsStore.subscribe).toHaveBeenCalledTimes(1);
    expect(mocks.libraryStore.load).toHaveBeenCalledTimes(1);

    view.unmount();
    expect(mocks.jobsStore.unsubscribe).toHaveBeenCalledTimes(1);
  });
});

describe('Home session list (story 2.9)', () => {
  beforeEach(() => i18n.applyPreference('vi'));

  it('shows the thin drop-zone and session rows, newest first, instead of the two big empty cards', () => {
    mocks.libraryStore.sessions = [item('a', { title: 'cuộc họp A' }), item('b', { title: 'cuộc họp B' })];
    render(Home);

    expect(screen.getByText('Kéo file vào đây hoặc')).toBeTruthy();
    expect(screen.queryByText('Bắt đầu Live')).toBeNull();
    expect(screen.getByText('cuộc họp A')).toBeTruthy();
    expect(screen.getByText('cuộc họp B')).toBeTruthy();
  });

  it('shows the "Thiếu N khoảng" badge only when missingGapCount > 0, and "Phục hồi" when recovered', () => {
    mocks.libraryStore.sessions = [
      item('a', { title: 'còn thiếu', missingGapCount: 2 }),
      item('b', { title: 'đã phục hồi', recovered: true }),
      item('c', { title: 'bình thường' }),
    ];
    render(Home);

    expect(screen.getByText('Thiếu 2 khoảng')).toBeTruthy();
    expect(screen.getByText('Phục hồi')).toBeTruthy();
  });

  it('renders each row as a link to /session/:id', () => {
    mocks.libraryStore.sessions = [item('abc', { title: 'phiên abc' })];
    render(Home);

    const rowLink = screen.getByRole('link', { name: /phiên abc/ });
    expect(rowLink.getAttribute('href')).toBe('/session/abc');
  });

  it('shows a skeleton while the library is loading', () => {
    mocks.libraryStore.status = 'loading';
    render(Home);

    expect(screen.getAllByRole('status').length).toBeGreaterThan(0);
    expect(screen.queryByText('Kéo file vào đây')).toBeNull();
  });

  it('shows a load error with a Retry button that calls load() again', async () => {
    mocks.libraryStore.status = 'error';
    render(Home);

    expect(screen.getByText('Không tải được danh sách phiên.')).toBeTruthy();
    mocks.libraryStore.load.mockClear();
    await fireEvent.click(screen.getByRole('button', { name: 'Thử lại' }));
    expect(mocks.libraryStore.load).toHaveBeenCalledTimes(1);
  });

  it('shows a transient reload-error notice while keeping the old list visible', () => {
    mocks.libraryStore.sessions = [item('a')];
    mocks.libraryStore.reloadError = true;
    render(Home);

    expect(screen.getByText('Không tải lại được danh sách mới nhất.')).toBeTruthy();
    expect(screen.getByText('phiên a')).toBeTruthy();
  });
});

describe('Home job card (story 2.9)', () => {
  beforeEach(() => i18n.applyPreference('vi'));

  it('shows the running job at the top with progress, chunk/key/attempt detail, Mở and Huỷ', async () => {
    mocks.jobsStore.jobs = new Map([['j1', job('j1')]]);
    render(Home);

    expect(screen.getByText('fixture.wav')).toBeTruthy();
    expect(screen.getByText('30 / 90 phút · 33 %')).toBeTruthy();
    expect(screen.getByText(/Đoạn 6 \/ 18.*Key thứ 2.*Lần thử 1/)).toBeTruthy();
    const openLink = screen.getByRole('link', { name: 'Mở' });
    expect(openLink.getAttribute('href')).toBe('/session/session-j1');

    await fireEvent.click(screen.getByRole('button', { name: 'Huỷ' }));
    expect(mocks.jobsStore.cancel).toHaveBeenCalledWith('j1');
  });

  it('keeps the job card and re-enables Huỷ when cancelling fails', async () => {
    mocks.jobsStore.jobs = new Map([['j1', job('j1')]]);
    mocks.jobsStore.cancel.mockResolvedValueOnce(null);
    render(Home);

    await fireEvent.click(screen.getByRole('button', { name: 'Huỷ' }));
    await Promise.resolve();

    const cancel = await screen.findByRole('button', { name: 'Huỷ' });
    expect((cancel as HTMLButtonElement).disabled).toBe(false);
    expect(screen.getByText('fixture.wav')).toBeTruthy();
  });

  it('shows the queued job (Job đầu hàng) when nothing is running yet', () => {
    mocks.jobsStore.jobs = new Map([['j1', job('j1', { state: 'queued', sourceName: 'queued.wav' })]]);
    render(Home);

    expect(screen.getByText('queued.wav')).toBeTruthy();
  });

  it('shows "Đang chờ quota…" and "N file đang chờ" when applicable', () => {
    mocks.jobsStore.jobs = new Map([
      ['j1', job('j1', { waitingQuota: true })],
      ['j2', job('j2', { state: 'queued' })],
    ]);
    render(Home);

    expect(screen.getByText('Đang chờ quota…')).toBeTruthy();
    expect(screen.getByText('1 file đang chờ')).toBeTruthy();
  });

  it('uses the "Đang chạy lại" label for a rerun job with no sourceName', () => {
    mocks.jobsStore.jobs = new Map([['j1', job('j1', { kind: 'rerun', sourceName: null })]]);
    render(Home);

    expect(screen.getByText('Đang chạy lại')).toBeTruthy();
  });

  it('still shows the job card in the otherwise-empty state (a Job but no Phiên yet)', () => {
    mocks.jobsStore.jobs = new Map([['j1', job('j1')]]);
    mocks.libraryStore.sessions = [];
    render(Home);

    expect(screen.getByText('fixture.wav')).toBeTruthy();
    expect(screen.queryByText('Bắt đầu Live')).toBeNull();
  });

  it('renders the true empty state (two big cards) only with no Phiên and no Job', () => {
    render(Home);

    expect(screen.getByRole('heading', { name: 'Kéo file vào đây' })).toBeTruthy();
    expect(screen.getByRole('heading', { name: 'Bắt đầu Live' })).toBeTruthy();
  });

  it('does not flash the two big empty cards before the first Job snapshot arrives', () => {
    mocks.jobsStore.synced = false;
    mocks.jobsStore.status = 'idle';
    render(Home);

    expect(screen.queryByRole('heading', { name: 'Bắt đầu Live' })).toBeNull();
  });
});
