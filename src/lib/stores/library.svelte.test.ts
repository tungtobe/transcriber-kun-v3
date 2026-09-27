// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync } from 'svelte';

const mocks = vi.hoisted(() => ({
  librarySessionsList: vi.fn(),
  librarySessionRename: vi.fn(),
  librarySessionDelete: vi.fn(),
  jobsSubscribe: vi.fn(),
}));

class FakeChannel<T> {
  onmessage: (event: T) => void = () => {};
}

vi.mock('@tauri-apps/api/core', () => ({
  Channel: FakeChannel,
}));

vi.mock('../bindings', () => ({
  commands: {
    librarySessionsList: (...args: unknown[]) => mocks.librarySessionsList(...args),
    librarySessionRename: (...args: unknown[]) => mocks.librarySessionRename(...args),
    librarySessionDelete: (...args: unknown[]) => mocks.librarySessionDelete(...args),
    jobsSubscribe: (...args: unknown[]) => mocks.jobsSubscribe(...args),
  },
}));

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

describe('libraryStore', () => {
  let capturedChannel: FakeChannel<unknown> | null = null;

  beforeEach(() => {
    vi.resetModules();
    mocks.librarySessionsList.mockReset();
    mocks.librarySessionRename.mockReset();
    mocks.librarySessionDelete.mockReset();
    mocks.jobsSubscribe.mockReset();
    capturedChannel = null;
    mocks.jobsSubscribe.mockImplementation((channel: FakeChannel<unknown>) => {
      capturedChannel = channel;
      return Promise.resolve({ status: 'ok', data: null });
    });
  });

  it('loads sessions and flips status loading -> ready', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({
      status: 'ok',
      data: [item('a'), item('b')],
    });
    const { libraryStore: store } = await import('./library.svelte');

    expect(store.status).toBe('loading');
    await store.load();

    expect(store.status).toBe('ready');
    expect(store.sessions).toHaveLength(2);
  });

  it('surfaces a typed error and keeps sessions empty on the first load failing', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'db unavailable' },
    });
    const { libraryStore: store } = await import('./library.svelte');

    await store.load();

    expect(store.status).toBe('error');
    expect(store.error?.category).toBe('storage');
    expect(store.sessions).toEqual([]);
  });

  it('surfaces a safe error when the IPC bridge rejects on the first load', async () => {
    mocks.librarySessionsList.mockRejectedValueOnce(new Error('bridge down'));
    const { libraryStore: store } = await import('./library.svelte');

    await store.load();

    expect(store.status).toBe('error');
    expect(store.error).not.toBeNull();
  });

  it('runs one follow-up load when load() is requested while an earlier load is in flight', async () => {
    let resolveFirst: (value: unknown) => void = () => {};
    mocks.librarySessionsList
      .mockImplementationOnce(() => new Promise((resolve) => { resolveFirst = resolve; }))
      .mockResolvedValueOnce({ status: 'ok', data: [item('new'), item('old')] });
    const { libraryStore: store } = await import('./library.svelte');

    const first = store.load();
    const second = store.load();
    const third = store.load();
    resolveFirst({ status: 'ok', data: [item('old')] });
    await Promise.all([first, second, third]);

    expect(mocks.librarySessionsList).toHaveBeenCalledTimes(2);
    expect(store.sessions.map((session) => session.sessionId)).toEqual(['new', 'old']);
  });

  it('retry after an error re-runs load() and can succeed', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'db unavailable' },
    });
    const { libraryStore: store } = await import('./library.svelte');
    await store.load();
    expect(store.status).toBe('error');

    mocks.librarySessionsList.mockResolvedValueOnce({ status: 'ok', data: [item('a')] });
    await store.load();

    expect(store.status).toBe('ready');
    expect(store.sessions).toHaveLength(1);
  });

  it('does not run overlapping loads — a second call while one is in flight reuses the same request', async () => {
    let resolveFirst: (value: unknown) => void = () => {};
    mocks.librarySessionsList.mockImplementationOnce(
      () => new Promise((resolve) => { resolveFirst = resolve; }),
    );
    const { libraryStore: store } = await import('./library.svelte');

    const first = store.load();
    const second = store.load();
    expect(mocks.librarySessionsList).toHaveBeenCalledTimes(1);

    resolveFirst({ status: 'ok', data: [item('a')] });
    await first;
    await second;
    expect(store.sessions).toHaveLength(1);
  });

  it('auto-reloads when jobsStore.resultSeq changes, keeping the old list and reporting reloadError on failure', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({
      status: 'ok',
      data: [item('a')],
    });
    const { libraryStore: store } = await import('./library.svelte');
    const { jobsStore } = await import('./jobs.svelte');
    await store.load();
    expect(store.sessions).toHaveLength(1);
    expect(mocks.librarySessionsList).toHaveBeenCalledTimes(1);

    // A Job commits — `resultSeq` increments, the store must reload without
    // anyone calling `load()` directly.
    mocks.librarySessionsList.mockResolvedValueOnce({
      status: 'ok',
      data: [item('b'), item('a')],
    });
    await jobsStore.subscribe();
    capturedChannel!.onmessage({ kind: 'snapshot', seq: 1, jobs: [{ jobId: 'j1', sessionId: 'b', sourceName: 'f.wav', kind: 'transcribe', state: 'running', processedMs: 0, totalMs: 1, chunkIndex: 0, chunkCount: 1, keyOrdinal: null, attempt: null, waitingQuota: false }] });
    capturedChannel!.onmessage({ kind: 'result', seq: 2, jobId: 'j1', sessionId: 'b' });
    flushSync();
    await Promise.resolve();
    await Promise.resolve();

    expect(mocks.librarySessionsList).toHaveBeenCalledTimes(2);
    expect(store.sessions.map((s) => s.sessionId)).toEqual(['b', 'a']);

    // A later reload failing must NOT blank the list or flip status to
    // error — only `reloadError` reports it (spec I/O Matrix "Commit").
    mocks.librarySessionsList.mockResolvedValueOnce({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'reload failed' },
    });
    capturedChannel!.onmessage({ kind: 'result', seq: 3, jobId: 'j2', sessionId: 'c' });
    flushSync();
    await Promise.resolve();
    await Promise.resolve();

    expect(store.status).toBe('ready');
    expect(store.sessions.map((s) => s.sessionId)).toEqual(['b', 'a']);
    expect(store.reloadError).toBe(true);
  });

  // Story 3.1: `rename`/`remove`.

  it('rename() updates the matching session in place on success', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({
      status: 'ok',
      data: [item('a', { title: 'cũ' }), item('b')],
    });
    const { libraryStore: store } = await import('./library.svelte');
    await store.load();

    mocks.librarySessionRename.mockResolvedValueOnce({ status: 'ok', data: 'mới' });
    const result = await store.rename('a', '  mới  ');

    expect(mocks.librarySessionRename).toHaveBeenCalledWith('a', '  mới  ');
    expect(result).toEqual({ status: 'ok', title: 'mới' });
    expect(store.sessions.find((s) => s.sessionId === 'a')?.title).toBe('mới');
    expect(store.sessions.find((s) => s.sessionId === 'b')?.title).toBe('phiên b');
  });

  it('rename() leaves the list untouched when Rust reports the session no longer exists', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({
      status: 'ok',
      data: [item('a', { title: 'cũ' })],
    });
    const { libraryStore: store } = await import('./library.svelte');
    await store.load();

    mocks.librarySessionRename.mockResolvedValueOnce({ status: 'ok', data: null });
    const result = await store.rename('a', 'mới');

    expect(result).toEqual({ status: 'ok', title: null });
    expect(store.sessions.find((s) => s.sessionId === 'a')?.title).toBe('cũ');
  });

  it('rename() surfaces a typed error and does not touch the list', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({
      status: 'ok',
      data: [item('a', { title: 'cũ' })],
    });
    const { libraryStore: store } = await import('./library.svelte');
    await store.load();

    mocks.librarySessionRename.mockResolvedValueOnce({
      status: 'error',
      error: { category: 'model', code: 'request', detailRedacted: 'empty title' },
    });
    const result = await store.rename('a', '');

    expect(result).toEqual({
      status: 'error',
      error: { category: 'model', code: 'request', detailRedacted: 'empty title' },
    });
    expect(store.sessions.find((s) => s.sessionId === 'a')?.title).toBe('cũ');
  });

  it('remove() reloads the list when the outcome is Deleted', async () => {
    mocks.librarySessionsList
      .mockResolvedValueOnce({ status: 'ok', data: [item('a'), item('b')] })
      .mockResolvedValueOnce({ status: 'ok', data: [item('b')] });
    const { libraryStore: store } = await import('./library.svelte');
    await store.load();

    mocks.librarySessionDelete.mockResolvedValueOnce({ status: 'ok', data: 'deleted' });
    const result = await store.remove('a');
    await Promise.resolve();
    await Promise.resolve();

    expect(mocks.librarySessionDelete).toHaveBeenCalledWith('a');
    expect(result).toEqual({ status: 'ok', outcome: 'deleted' });
    expect(mocks.librarySessionsList).toHaveBeenCalledTimes(2);
    expect(store.sessions.map((s) => s.sessionId)).toEqual(['b']);
  });

  it('remove() does not reload the list on a Busy outcome', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({ status: 'ok', data: [item('a')] });
    const { libraryStore: store } = await import('./library.svelte');
    await store.load();

    mocks.librarySessionDelete.mockResolvedValueOnce({ status: 'ok', data: 'busy' });
    const result = await store.remove('a');

    expect(result).toEqual({ status: 'ok', outcome: 'busy' });
    expect(mocks.librarySessionsList).toHaveBeenCalledTimes(1);
    expect(store.sessions.map((s) => s.sessionId)).toEqual(['a']);
  });

  it('remove() surfaces a typed error without reloading', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({ status: 'ok', data: [item('a')] });
    const { libraryStore: store } = await import('./library.svelte');
    await store.load();

    mocks.librarySessionDelete.mockResolvedValueOnce({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'fs failed' },
    });
    const result = await store.remove('a');

    expect(result).toEqual({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'fs failed' },
    });
    expect(mocks.librarySessionsList).toHaveBeenCalledTimes(1);
  });
});
