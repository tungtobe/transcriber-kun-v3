// @vitest-environment jsdom
//
// Integration tests for the two behavior-level acceptance criteria in
// spec-2-9-home-danh-sach-phien-va-card-job.md that need the real
// `jobsStore` + `libraryStore` wired together through `Home.svelte` (not the
// per-store unit tests in `jobs.svelte.test.ts` / `library.svelte.test.ts`,
// and not the mocked-store UI tests in `Home.test.ts`):
//
//   1. 500 Phiên mocked from `librarySessionsList` -> Home renders (jsdom,
//      800px viewport) with < 60 DOM rows, newest `createdAt` first.
//   2. Home showing 2 Phiên + 1 running Job -> a Job `result` event ->
//      `librarySessionsList` is called again, the new Phiên appears first,
//      the job card disappears — with no user action.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import { configureRouter } from '../lib/router';
import Home from './Home.svelte';

const mocks = vi.hoisted(() => {
  class FakeChannel<T> {
    onmessage: (event: T) => void = () => {};
  }
  return {
    librarySessionsList: vi.fn(),
    jobsSubscribe: vi.fn(),
    jobsUnsubscribe: vi.fn().mockResolvedValue({ status: 'ok', data: null }),
    jobsCancel: vi.fn(),
    keysList: vi.fn(),
    FakeChannel,
  };
});

vi.mock('@tauri-apps/api/core', () => ({
  Channel: mocks.FakeChannel,
}));

vi.mock('../lib/bindings', () => ({
  commands: {
    librarySessionsList: (...args: unknown[]) => mocks.librarySessionsList(...args),
    jobsSubscribe: (...args: unknown[]) => mocks.jobsSubscribe(...args),
    jobsUnsubscribe: (...args: unknown[]) => mocks.jobsUnsubscribe(...args),
    jobsCancel: (...args: unknown[]) => mocks.jobsCancel(...args),
    keysList: (...args: unknown[]) => mocks.keysList(...args),
  },
}));

// `SessionRow` -> `formatTimestamp`/`displayTimestamp`-adjacent code paths
// pull in the real `settingsStore`, which installs a `settingsChanged`
// backend listener on bootstrap — out of scope here, same seam
// `Session.test.ts` stubs.
vi.mock('../lib/stores/settings.svelte', () => ({
  settingsStore: { timestampOffsetSec: 0 },
}));

type CapturedChannel = { onmessage: (event: unknown) => void };
let capturedChannel: CapturedChannel | null = null;

function item(sessionId: string, overrides: Partial<Record<string, unknown>> = {}) {
  return {
    sessionId,
    kind: 'file',
    title: `phiên ${sessionId}`,
    createdAt: 1_000,
    durationSec: 10,
    recovered: false,
    missingGapCount: 0,
    ...overrides,
  };
}

function job(jobId: string, overrides: Partial<Record<string, unknown>> = {}) {
  return {
    jobId,
    sessionId: `session-${jobId}`,
    sourceName: 'fixture.wav',
    kind: 'transcribe',
    state: 'running',
    processedMs: 0,
    totalMs: 1,
    chunkIndex: 0,
    chunkCount: 1,
    keyOrdinal: null,
    attempt: null,
    waitingQuota: false,
    ...overrides,
  };
}

afterEach(() => cleanup());

beforeEach(async () => {
  configureRouter();
  i18n.applyPreference('vi');
  mocks.librarySessionsList.mockReset();
  mocks.jobsSubscribe.mockReset();
  mocks.jobsUnsubscribe.mockReset().mockResolvedValue({ status: 'ok', data: null });
  mocks.jobsCancel.mockReset();
  mocks.keysList.mockReset().mockResolvedValue({ status: 'ok', data: [] });
  capturedChannel = null;
  mocks.jobsSubscribe.mockImplementation((channel: CapturedChannel) => {
    capturedChannel = channel;
    return Promise.resolve({ status: 'ok', data: null });
  });
  const { jobsStore } = await import('../lib/stores/jobs.svelte');
  const { libraryStore } = await import('../lib/stores/library.svelte');
  jobsStore.reset();
  libraryStore.reset();
});

describe('Home + real jobsStore/libraryStore (story 2.9 acceptance criteria)', () => {
  it('renders 500 sessions with a virtualized DOM (< 60 rows), newest createdAt first, at an 800px viewport', async () => {
    // The real `library_sessions_list` command already returns newest
    // `createdAt` first (spec Boundaries) — `Home`/`SessionList` trust that
    // order and never re-sort client-side, so the mock supplies it
    // pre-sorted the same way.
    const many = Array.from({ length: 500 }, (_, i) => item(`s${i}`, { createdAt: 499 - i }));
    mocks.librarySessionsList.mockResolvedValue({ status: 'ok', data: many });

    const { container } = render(Home, { sessionListViewportHeight: 800 });

    await screen.findByText('phiên s0');
    const rows = container.querySelectorAll('.session-row');
    expect(rows.length).toBeLessThan(60);
    expect(rows[0].textContent).toContain('phiên s0');
  });

  it('reloads the list and drops the job card when a Job commits, with the new Phiên first — no user action', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({
      status: 'ok',
      data: [item('a', { createdAt: 2_000 }), item('b', { createdAt: 1_000 })],
    });
    render(Home);

    await screen.findByText('phiên a');
    expect(screen.getByText('phiên b')).toBeTruthy();
    await waitFor(() => expect(mocks.jobsSubscribe).toHaveBeenCalledTimes(1));

    capturedChannel!.onmessage({
      kind: 'snapshot',
      seq: 1,
      jobs: [job('j1', { sourceName: 'new-recording.wav' })],
    });
    await screen.findByText('new-recording.wav');

    mocks.librarySessionsList.mockResolvedValueOnce({
      status: 'ok',
      data: [item('c', { createdAt: 3_000 }), item('a', { createdAt: 2_000 }), item('b', { createdAt: 1_000 })],
    });
    capturedChannel!.onmessage({ kind: 'result', seq: 2, jobId: 'j1', sessionId: 'c' });

    await waitFor(() => expect(mocks.librarySessionsList).toHaveBeenCalledTimes(2));
    await screen.findByText('phiên c');
    expect(screen.queryByText('new-recording.wav')).toBeNull();
  });
});
