// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { i18n } from '../../i18n/index.svelte';

const mocks = vi.hoisted(() => ({
  transcribePickFiles: vi.fn(),
  jobsStart: vi.fn(),
  push: vi.fn(),
}));

vi.mock('../bindings', () => ({
  commands: {
    transcribePickFiles: (...args: unknown[]) => mocks.transcribePickFiles(...args),
  },
}));

vi.mock('./jobs.svelte', () => ({
  jobsStore: {
    start: (...args: unknown[]) => mocks.jobsStart(...args),
  },
}));

vi.mock('@keenmate/svelte-spa-router', () => ({
  push: (...args: unknown[]) => mocks.push(...args),
}));

afterEach(() => {
  vi.resetModules();
});

beforeEach(() => {
  i18n.applyPreference('vi');
  mocks.transcribePickFiles.mockReset();
  mocks.jobsStart.mockReset();
  mocks.push.mockReset().mockResolvedValue(undefined);
});

describe('intakeStore.basename', () => {
  it('strips both / and \\ path separators', async () => {
    const { basename } = await import('./intake.svelte');
    expect(basename('/home/user/recording.mp4')).toBe('recording.mp4');
    expect(basename('C:\\Users\\jp\\meeting.m4a')).toBe('meeting.m4a');
    expect(basename('plain.mp3')).toBe('plain.mp3');
  });
});

describe('intakeStore.pick', () => {
  it('does nothing when the dialog is cancelled (empty Vec)', async () => {
    mocks.transcribePickFiles.mockResolvedValue({ status: 'ok', data: [] });
    const { createIntakeStore } = await import('./intake.svelte');
    const store = createIntakeStore();

    await store.pick();

    expect(mocks.jobsStart).not.toHaveBeenCalled();
    expect(store.notices).toEqual([]);
  });

  it('submits every picked path in order', async () => {
    mocks.transcribePickFiles.mockResolvedValue({
      status: 'ok',
      data: ['/a.mp4', '/b.mp4'],
    });
    mocks.jobsStart.mockImplementation((path: string) =>
      Promise.resolve({ kind: 'job', jobId: `job-${path}`, sessionId: `session-${path}` }),
    );
    const { createIntakeStore } = await import('./intake.svelte');
    const store = createIntakeStore();

    await store.pick();

    expect(mocks.jobsStart.mock.calls.map((call) => call[0])).toEqual(['/a.mp4', '/b.mp4']);
  });
});

describe('intakeStore.submit — outcome to notice mapping', () => {
  it('maps a duplicate session to an info notice with the exact required copy', async () => {
    mocks.jobsStart.mockResolvedValue({ kind: 'existing', sessionId: 'session-1' });
    const { createIntakeStore } = await import('./intake.svelte');
    const store = createIntakeStore();

    await store.submit(['/dir/existing.mp4']);

    expect(store.notices).toHaveLength(1);
    expect(store.notices[0]).toMatchObject({
      variant: 'info',
      title: 'existing.mp4',
      message: 'Mở lại phiên có sẵn — không tốn token',
    });
    expect(mocks.push).toHaveBeenCalledWith('/session/session-1');
  });

  it('maps ExistingJob to an info notice and still navigates to it', async () => {
    mocks.jobsStart.mockResolvedValue({
      kind: 'existingJob',
      jobId: 'job-1',
      sessionId: 'session-1',
    });
    const { createIntakeStore } = await import('./intake.svelte');
    const store = createIntakeStore();

    await store.submit(['/dir/running.mp4']);

    expect(store.notices[0].variant).toBe('info');
    expect(mocks.push).toHaveBeenCalledWith('/session/session-1');
  });

  it('maps a new Job to an info "queued" notice and navigates to it', async () => {
    mocks.jobsStart.mockResolvedValue({ kind: 'job', jobId: 'job-1', sessionId: 'session-1' });
    const { createIntakeStore } = await import('./intake.svelte');
    const store = createIntakeStore();

    await store.submit(['/dir/new.mp4']);

    expect(store.notices[0]).toMatchObject({ variant: 'info', title: 'new.mp4' });
    expect(mocks.push).toHaveBeenCalledWith('/session/session-1');
  });

  it('maps a wrong-format error to a danger notice with the exact required copy, and never navigates', async () => {
    mocks.jobsStart.mockResolvedValue({
      error: { category: 'format', code: 'format', detailRedacted: 'bad extension' },
    });
    const { createIntakeStore } = await import('./intake.svelte');
    const store = createIntakeStore();

    await store.submit(['/dir/video.wmv']);

    expect(store.notices[0]).toMatchObject({
      variant: 'danger',
      title: 'video.wmv',
      message: 'Định dạng không hỗ trợ. Hãy chuyển sang mp4, m4a hoặc mp3.',
    });
    expect(mocks.push).not.toHaveBeenCalled();
  });

  it('maps a no-audio error to its own danger notice, distinct from wrong-format', async () => {
    mocks.jobsStart.mockResolvedValue({
      error: { category: 'format', code: 'noaudio', detailRedacted: 'no track' },
    });
    const { createIntakeStore } = await import('./intake.svelte');
    const store = createIntakeStore();

    await store.submit(['/dir/silent.mp4']);

    expect(store.notices[0]).toMatchObject({
      variant: 'danger',
      message: 'File không có audio phát được.',
    });
  });

  it('maps a missing-key error to a warning notice with a shortcut to /settings/gemini', async () => {
    mocks.jobsStart.mockResolvedValue({
      error: { category: 'auth', code: 'auth', detailRedacted: 'no usable key' },
    });
    const { createIntakeStore } = await import('./intake.svelte');
    const store = createIntakeStore();

    await store.submit(['/dir/new.mp4']);

    expect(store.notices[0]).toMatchObject({ variant: 'warning', actionHref: '/settings/gemini' });
  });

  it('falls back to the generic category title/hint for any other error', async () => {
    mocks.jobsStart.mockResolvedValue({
      error: { category: 'network', code: 'network', detailRedacted: 'timed out' },
    });
    const { createIntakeStore } = await import('./intake.svelte');
    const store = createIntakeStore();

    await store.submit(['/dir/new.mp4']);

    expect(store.notices[0].variant).toBe('danger');
    expect(store.notices[0].message.length).toBeGreaterThan(0);
  });
});

describe('intakeStore.submit — batch semantics', () => {
  it('processes a mixed batch sequentially, one notice per file, and does not let an error stop later files', async () => {
    mocks.jobsStart
      .mockResolvedValueOnce({ error: { category: 'format', code: 'format', detailRedacted: 'x' } })
      .mockResolvedValueOnce({ kind: 'existing', sessionId: 'session-existing' })
      .mockResolvedValueOnce({ kind: 'job', jobId: 'job-1', sessionId: 'session-new' });
    const { createIntakeStore } = await import('./intake.svelte');
    const store = createIntakeStore();

    await store.submit(['/a.wmv', '/b.mp4', '/c.mp4']);

    expect(store.notices).toHaveLength(3);
    expect(mocks.jobsStart).toHaveBeenCalledTimes(3);
    // First successful result in received order wins the navigation target
    // (spec Always) — here that's the second file (the existing session).
    expect(mocks.push).toHaveBeenCalledTimes(1);
    expect(mocks.push).toHaveBeenCalledWith('/session/session-existing');
  });

  it('stays on the current screen (never navigates) when every file in the batch errors', async () => {
    mocks.jobsStart.mockResolvedValue({
      error: { category: 'format', code: 'format', detailRedacted: 'x' },
    });
    const { createIntakeStore } = await import('./intake.svelte');
    const store = createIntakeStore();

    await store.submit(['/a.wmv', '/b.wmv']);

    expect(store.notices).toHaveLength(2);
    expect(mocks.push).not.toHaveBeenCalled();
  });

  it('chains overlapping submit() calls so a later batch never interleaves with an earlier one', async () => {
    let resolveFirst!: (value: unknown) => void;
    const firstCallPromise = new Promise((resolve) => {
      resolveFirst = resolve;
    });
    mocks.jobsStart
      .mockImplementationOnce(() => firstCallPromise)
      .mockImplementationOnce(() =>
        Promise.resolve({ kind: 'job', jobId: 'job-2', sessionId: 'session-2' }),
      );
    const { createIntakeStore } = await import('./intake.svelte');
    const store = createIntakeStore();

    const firstBatch = store.submit(['/slow.mp4']);
    const secondBatch = store.submit(['/fast.mp4']);

    // The second batch's file must not have been submitted yet — it is
    // queued behind the first, still-pending batch.
    await Promise.resolve();
    await Promise.resolve();
    expect(mocks.jobsStart).toHaveBeenCalledTimes(1);

    resolveFirst({ kind: 'job', jobId: 'job-1', sessionId: 'session-1' });
    await firstBatch;
    await secondBatch;

    expect(mocks.jobsStart).toHaveBeenCalledTimes(2);
    expect(mocks.jobsStart.mock.calls.map((call) => call[0])).toEqual(['/slow.mp4', '/fast.mp4']);
  });
});

describe('intakeStore.dismiss', () => {
  it('removes exactly the notice with the given id', async () => {
    mocks.jobsStart
      .mockResolvedValueOnce({ kind: 'existing', sessionId: 's1' })
      .mockResolvedValueOnce({ kind: 'existing', sessionId: 's2' });
    const { createIntakeStore } = await import('./intake.svelte');
    const store = createIntakeStore();

    await store.submit(['/a.mp4', '/b.mp4']);
    expect(store.notices).toHaveLength(2);
    const [first, second] = store.notices;

    store.dismiss(first.id);

    expect(store.notices).toEqual([second]);
  });
});
