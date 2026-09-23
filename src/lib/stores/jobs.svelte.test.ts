// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  jobsSubscribe: vi.fn(),
  jobsCancel: vi.fn(),
  transcribeStart: vi.fn(),
  transcribeRerun: vi.fn(),
}));

class FakeChannel<T> {
  onmessage: (event: T) => void = () => {};
}

vi.mock('@tauri-apps/api/core', () => ({
  Channel: FakeChannel,
}));

vi.mock('../bindings', () => ({
  commands: {
    jobsSubscribe: (...args: unknown[]) => mocks.jobsSubscribe(...args),
    jobsCancel: (...args: unknown[]) => mocks.jobsCancel(...args),
    transcribeStart: (...args: unknown[]) => mocks.transcribeStart(...args),
    transcribeRerun: (...args: unknown[]) => mocks.transcribeRerun(...args),
  },
}));

function job(jobId: string, overrides: Partial<Record<string, unknown>> = {}) {
  return {
    jobId,
    sessionId: `session-${jobId}`,
    sourceName: 'fixture.wav',
    state: 'running',
    processedMs: 0,
    totalMs: 60_000,
    chunkIndex: 0,
    chunkCount: 1,
    keyOrdinal: null,
    attempt: null,
    waitingQuota: false,
    ...overrides,
  };
}

describe('jobsStore', () => {
  let capturedChannel: FakeChannel<unknown> | null = null;

  beforeEach(() => {
    vi.resetModules();
    mocks.jobsSubscribe.mockReset();
    mocks.jobsCancel.mockReset();
    mocks.transcribeStart.mockReset();
    mocks.transcribeRerun.mockReset();
    capturedChannel = null;
    mocks.jobsSubscribe.mockImplementation((channel: FakeChannel<unknown>) => {
      capturedChannel = channel;
      return Promise.resolve({ status: 'ok', data: null });
    });
  });

  it('applies a snapshot then keeps jobs in sync as updated/result events arrive', async () => {
    const { createJobsStore } = await import('./jobs.svelte');
    const store = createJobsStore();

    await store.subscribe();
    expect(store.synced).toBe(false);
    expect(store.jobs.size).toBe(0);

    capturedChannel!.onmessage({ kind: 'snapshot', seq: 1, jobs: [job('a')] });
    expect(store.synced).toBe(true);
    expect(store.jobs.get('a')?.processedMs).toBe(0);

    capturedChannel!.onmessage({ kind: 'updated', seq: 2, job: job('a', { processedMs: 30_000 }) });
    expect(store.jobs.get('a')?.processedMs).toBe(30_000);

    capturedChannel!.onmessage({ kind: 'result', seq: 3, jobId: 'a', sessionId: 'session-a' });
    expect(store.jobs.has('a')).toBe(false);
  });

  it('re-subscribes automatically when seq skips (a missed event)', async () => {
    const { createJobsStore } = await import('./jobs.svelte');
    const store = createJobsStore();

    await store.subscribe();
    const firstChannel = capturedChannel!;
    firstChannel.onmessage({ kind: 'snapshot', seq: 1, jobs: [job('a')] });
    expect(mocks.jobsSubscribe).toHaveBeenCalledTimes(1);

    // seq jumps from 1 to 3 — a gap.
    firstChannel.onmessage({ kind: 'updated', seq: 3, job: job('a', { processedMs: 10 }) });

    // Allow the microtask from the internal re-subscribe to settle.
    await Promise.resolve();
    await Promise.resolve();

    expect(mocks.jobsSubscribe).toHaveBeenCalledTimes(2);
    // The stale first channel's later messages must not resurrect state.
    firstChannel.onmessage({ kind: 'updated', seq: 4, job: job('a', { processedMs: 999 }) });
    expect(store.jobs.get('a')?.processedMs).not.toBe(999);
  });

  it('ignores messages from a channel superseded by unsubscribe', async () => {
    const { createJobsStore } = await import('./jobs.svelte');
    const store = createJobsStore();

    await store.subscribe();
    const staleChannel = capturedChannel!;
    staleChannel.onmessage({ kind: 'snapshot', seq: 1, jobs: [job('a')] });
    store.unsubscribe();
    expect(store.jobs.size).toBe(0);
    expect(store.synced).toBe(false);

    staleChannel.onmessage({ kind: 'updated', seq: 2, job: job('a', { processedMs: 500 }) });
    expect(store.jobs.size).toBe(0);
  });

  it('surfaces a jobsSubscribe error without throwing', async () => {
    mocks.jobsSubscribe.mockResolvedValue({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'registry unavailable' },
    });
    const { createJobsStore } = await import('./jobs.svelte');
    const store = createJobsStore();

    await store.subscribe();

    expect(store.status).toBe('error');
    expect(store.error?.category).toBe('storage');
  });

  it('start returns the typed outcome on success and a safe error when IPC rejects', async () => {
    mocks.transcribeStart.mockResolvedValueOnce({
      status: 'ok',
      data: { kind: 'job', jobId: 'j1', sessionId: 's1' },
    });
    const { createJobsStore } = await import('./jobs.svelte');
    const store = createJobsStore();

    const outcome = await store.start('/tmp/a.wav');
    expect(outcome).toEqual({ kind: 'job', jobId: 'j1', sessionId: 's1' });

    mocks.transcribeStart.mockRejectedValueOnce(new Error('bridge down'));
    const failed = await store.start('/tmp/a.wav');
    expect('error' in failed).toBe(true);
  });

  it('rerun returns the typed outcome on success and a safe error when IPC rejects', async () => {
    mocks.transcribeRerun.mockResolvedValueOnce({
      status: 'ok',
      data: { kind: 'started', jobId: 'j1' },
    });
    const { createJobsStore } = await import('./jobs.svelte');
    const store = createJobsStore();

    const outcome = await store.rerun('s1', 't1', { kind: 'missing' });
    expect(outcome).toEqual({ kind: 'started', jobId: 'j1' });
    expect(mocks.transcribeRerun).toHaveBeenCalledWith('s1', 't1', { kind: 'missing' });

    mocks.transcribeRerun.mockRejectedValueOnce(new Error('bridge down'));
    const failed = await store.rerun('s1', 't1', { kind: 'missing' });
    expect('error' in failed).toBe(true);
  });

  it('cancel returns the outcome on success and null on failure', async () => {
    mocks.jobsCancel.mockResolvedValueOnce({ status: 'ok', data: 'cancelling' });
    const { createJobsStore } = await import('./jobs.svelte');
    const store = createJobsStore();

    expect(await store.cancel('j1')).toBe('cancelling');

    mocks.jobsCancel.mockRejectedValueOnce(new Error('bridge down'));
    expect(await store.cancel('j1')).toBeNull();
  });
});
