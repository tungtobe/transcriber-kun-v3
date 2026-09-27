// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync } from 'svelte';

const mocks = vi.hoisted(() => ({
  librarySessionsList: vi.fn(),
  librarySessionRename: vi.fn(),
  librarySessionDelete: vi.fn(),
  jobsSubscribe: vi.fn(),
  tagsList: vi.fn(),
  tagsCreate: vi.fn(),
  sessionTagsAttach: vi.fn(),
  sessionTagsDetach: vi.fn(),
  tagsDelete: vi.fn(),
  libraryWipeAll: vi.fn(),
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
    tagsList: (...args: unknown[]) => mocks.tagsList(...args),
    tagsCreate: (...args: unknown[]) => mocks.tagsCreate(...args),
    sessionTagsAttach: (...args: unknown[]) => mocks.sessionTagsAttach(...args),
    sessionTagsDetach: (...args: unknown[]) => mocks.sessionTagsDetach(...args),
    tagsDelete: (...args: unknown[]) => mocks.tagsDelete(...args),
    libraryWipeAll: (...args: unknown[]) => mocks.libraryWipeAll(...args),
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
    tagIds: [],
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
    mocks.tagsList.mockReset();
    mocks.tagsCreate.mockReset();
    mocks.sessionTagsAttach.mockReset();
    mocks.sessionTagsDetach.mockReset();
    mocks.tagsDelete.mockReset();
    mocks.libraryWipeAll.mockReset();
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

  // Story 3.4: wipeAll() reloads both sessions and tags on `wiped`.

  it('wipeAll() reloads sessions and tags when the outcome is wiped', async () => {
    mocks.librarySessionsList
      .mockResolvedValueOnce({ status: 'ok', data: [item('a')] })
      .mockResolvedValueOnce({ status: 'ok', data: [] });
    mocks.tagsList
      .mockResolvedValueOnce({ status: 'ok', data: [{ id: 't1', name: 'x', sessionCount: 1 }] })
      .mockResolvedValueOnce({ status: 'ok', data: [] });
    const { libraryStore: store } = await import('./library.svelte');
    await store.load();
    await store.loadTags();

    mocks.libraryWipeAll.mockResolvedValueOnce({ status: 'ok', data: 'wiped' });
    const result = await store.wipeAll();
    await Promise.resolve();
    await Promise.resolve();

    expect(mocks.libraryWipeAll).toHaveBeenCalledTimes(1);
    expect(result).toEqual({ status: 'ok', outcome: 'wiped' });
    expect(mocks.librarySessionsList).toHaveBeenCalledTimes(2);
    expect(mocks.tagsList).toHaveBeenCalledTimes(2);
    expect(store.sessions).toEqual([]);
    expect(store.tags).toEqual([]);
  });

  it('wipeAll() does not reload on a busy outcome', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({ status: 'ok', data: [item('a')] });
    const { libraryStore: store } = await import('./library.svelte');
    await store.load();

    mocks.libraryWipeAll.mockResolvedValueOnce({ status: 'ok', data: 'busy' });
    const result = await store.wipeAll();

    expect(result).toEqual({ status: 'ok', outcome: 'busy' });
    expect(mocks.librarySessionsList).toHaveBeenCalledTimes(1);
    expect(mocks.tagsList).not.toHaveBeenCalled();
  });

  it('wipeAll() surfaces a typed error without reloading', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({ status: 'ok', data: [item('a')] });
    const { libraryStore: store } = await import('./library.svelte');
    await store.load();

    mocks.libraryWipeAll.mockResolvedValueOnce({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'fs failed' },
    });
    const result = await store.wipeAll();

    expect(result).toEqual({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'fs failed' },
    });
    expect(mocks.librarySessionsList).toHaveBeenCalledTimes(1);
    expect(mocks.tagsList).not.toHaveBeenCalled();
  });

  // Story 3.2: tags + tagFilter + filteredSessions.

  function tag(id: string, overrides: Partial<Record<string, unknown>> = {}) {
    return { id, name: `tag-${id}`, sessionCount: 0, ...overrides };
  }

  it('loadTags() populates tags on success and keeps the old list on failure', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({ status: 'ok', data: [] });
    const { libraryStore: store } = await import('./library.svelte');
    await store.load();

    mocks.tagsList.mockResolvedValueOnce({ status: 'ok', data: [tag('a'), tag('b')] });
    await store.loadTags();
    expect(store.tags.map((t) => t.id)).toEqual(['a', 'b']);

    mocks.tagsList.mockResolvedValueOnce({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'x' },
    });
    await store.loadTags();
    expect(store.tags.map((t) => t.id)).toEqual(['a', 'b']);
  });

  it('createTag() adds a brand-new tag to the list but not a duplicate of an existing id', async () => {
    const { libraryStore: store } = await import('./library.svelte');

    mocks.tagsCreate.mockResolvedValueOnce({ status: 'ok', data: { id: 'a', name: 'A' } });
    const result = await store.createTag('  A  ');
    expect(result).toEqual({ status: 'ok', tag: { id: 'a', name: 'A' } });
    expect(store.tags).toEqual([{ id: 'a', name: 'A', sessionCount: 0 }]);

    // Trùng hoa thường -- Rust trả lại cùng id, không thêm dòng thứ hai.
    mocks.tagsCreate.mockResolvedValueOnce({ status: 'ok', data: { id: 'a', name: 'A' } });
    await store.createTag('a');
    expect(store.tags).toHaveLength(1);
  });

  it('attachTag() adds the tag to the session and bumps its count, but is a no-op the second time', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({ status: 'ok', data: [item('a')] });
    const { libraryStore: store } = await import('./library.svelte');
    await store.load();
    mocks.tagsList.mockResolvedValueOnce({ status: 'ok', data: [tag('t1')] });
    await store.loadTags();

    mocks.sessionTagsAttach.mockResolvedValueOnce({ status: 'ok', data: null });
    await store.attachTag('a', 't1');
    expect(store.sessions.find((s) => s.sessionId === 'a')?.tagIds).toEqual(['t1']);
    expect(store.tags.find((t) => t.id === 't1')?.sessionCount).toBe(1);

    // Gắn lại (Rust coi là no-op) -- không tăng đếm lần hai.
    mocks.sessionTagsAttach.mockResolvedValueOnce({ status: 'ok', data: null });
    await store.attachTag('a', 't1');
    expect(store.tags.find((t) => t.id === 't1')?.sessionCount).toBe(1);
  });

  it('attaching a tag while a filter is active never changes the filter (Acceptance Criteria)', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({ status: 'ok', data: [item('a')] });
    const { libraryStore: store } = await import('./library.svelte');
    await store.load();
    mocks.tagsList.mockResolvedValueOnce({ status: 'ok', data: [tag('t1')] });
    await store.loadTags();
    store.toggleFilterTag('t1');
    expect(store.tagFilter).toEqual({ tagIds: ['t1'], untagged: false });

    mocks.sessionTagsAttach.mockResolvedValueOnce({ status: 'ok', data: null });
    await store.attachTag('a', 't1');

    expect(store.tagFilter).toEqual({ tagIds: ['t1'], untagged: false });
  });

  it('detachTag() removes the tag from the session and decrements its count, floored at zero', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({
      status: 'ok',
      data: [item('a', { tagIds: ['t1'] })],
    });
    const { libraryStore: store } = await import('./library.svelte');
    await store.load();
    mocks.tagsList.mockResolvedValueOnce({ status: 'ok', data: [tag('t1', { sessionCount: 1 })] });
    await store.loadTags();

    mocks.sessionTagsDetach.mockResolvedValueOnce({ status: 'ok', data: null });
    await store.detachTag('a', 't1');

    expect(store.sessions.find((s) => s.sessionId === 'a')?.tagIds).toEqual([]);
    expect(store.tags.find((t) => t.id === 't1')?.sessionCount).toBe(0);
  });

  it('deleteTagGlobally() removes the tag everywhere: the list, every session, and the active filter', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({
      status: 'ok',
      data: [item('a', { tagIds: ['t1'] }), item('b', { tagIds: ['t1'] })],
    });
    const { libraryStore: store } = await import('./library.svelte');
    await store.load();
    mocks.tagsList.mockResolvedValueOnce({ status: 'ok', data: [tag('t1', { sessionCount: 2 })] });
    await store.loadTags();
    store.toggleFilterTag('t1');

    mocks.tagsDelete.mockResolvedValueOnce({ status: 'ok', data: null });
    await store.deleteTagGlobally('t1');

    expect(store.tags).toEqual([]);
    expect(store.sessions.every((s) => s.tagIds.length === 0)).toBe(true);
    expect(store.tagFilter).toEqual({ tagIds: [], untagged: false });
  });

  it('toggleFilterTag/toggleUntaggedFilter are mutually exclusive and drive filteredSessions (AND, untagged)', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({
      status: 'ok',
      data: [
        item('a', { tagIds: ['x', 'y'] }),
        item('b', { tagIds: ['x'] }),
        item('c', { tagIds: [] }),
      ],
    });
    const { libraryStore: store } = await import('./library.svelte');
    await store.load();

    store.toggleFilterTag('x');
    store.toggleFilterTag('y');
    expect(store.filteredSessions.map((s) => s.sessionId)).toEqual(['a']);

    // Bật "Chưa gắn tag" xoá tag đang lọc.
    store.toggleUntaggedFilter();
    expect(store.tagFilter).toEqual({ tagIds: [], untagged: true });
    expect(store.filteredSessions.map((s) => s.sessionId)).toEqual(['c']);

    // Chọn lại một tag cụ thể tắt "Chưa gắn tag".
    store.toggleFilterTag('x');
    expect(store.tagFilter).toEqual({ tagIds: ['x'], untagged: false });

    store.clearTagFilter();
    expect(store.filteredSessions.map((s) => s.sessionId)).toEqual(['a', 'b', 'c']);
  });

  // Story 3.3: nameQuery + tagFilter AND, clear riêng/clear tất cả.

  it('setNameQuery() ANDs the name query with the current tag filter in filteredSessions', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({
      status: 'ok',
      data: [
        item('a', { title: 'họp sprint', tagIds: ['x'] }),
        item('b', { title: 'review code', tagIds: ['x'] }),
        item('c', { title: 'họp sprint khác', tagIds: [] }),
      ],
    });
    const { libraryStore: store } = await import('./library.svelte');
    await store.load();

    store.toggleFilterTag('x');
    store.setNameQuery('  HỌP ');
    expect(store.nameQuery).toBe('  HỌP ');
    expect(store.filteredSessions.map((s) => s.sessionId)).toEqual(['a']);
  });

  it('clearNameQuery() clears only the query, leaving the tag filter untouched', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({
      status: 'ok',
      data: [item('a', { title: 'họp sprint', tagIds: ['x'] }), item('b', { title: 'review code', tagIds: ['x'] })],
    });
    const { libraryStore: store } = await import('./library.svelte');
    await store.load();

    store.toggleFilterTag('x');
    store.setNameQuery('họp');
    expect(store.filteredSessions.map((s) => s.sessionId)).toEqual(['a']);

    store.clearNameQuery();
    expect(store.nameQuery).toBe('');
    expect(store.tagFilter).toEqual({ tagIds: ['x'], untagged: false });
    expect(store.filteredSessions.map((s) => s.sessionId)).toEqual(['a', 'b']);
  });

  it('clearAllFilters() clears both the query and the tag filter', async () => {
    mocks.librarySessionsList.mockResolvedValueOnce({
      status: 'ok',
      data: [item('a', { title: 'họp sprint', tagIds: ['x'] }), item('b', { title: 'review code', tagIds: [] })],
    });
    const { libraryStore: store } = await import('./library.svelte');
    await store.load();

    store.toggleFilterTag('x');
    store.setNameQuery('họp');
    expect(store.filteredSessions.map((s) => s.sessionId)).toEqual(['a']);

    store.clearAllFilters();
    expect(store.nameQuery).toBe('');
    expect(store.tagFilter).toEqual({ tagIds: [], untagged: false });
    expect(store.filteredSessions.map((s) => s.sessionId)).toEqual(['a', 'b']);
  });
});
