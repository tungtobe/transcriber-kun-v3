// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import type { JobSnapshot } from '../../lib/bindings';

const mocks = vi.hoisted(() => ({
  logFor: vi.fn(() => [] as unknown[]),
  jobs: new Map<string, JobSnapshot>(),
}));

vi.mock('../../lib/stores/jobs.svelte', () => ({
  jobsStore: {
    logFor: mocks.logFor,
    get jobs() {
      return mocks.jobs;
    },
  },
}));

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  mocks.logFor.mockReset().mockReturnValue([]);
  mocks.jobs.clear();
});

function job(overrides: Partial<JobSnapshot> = {}): JobSnapshot {
  return {
    jobId: 'job-1',
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
    ...overrides,
  };
}

describe('JobProgress', () => {
  it('renders progress, chunk, key, attempt, and calls onCancel', async () => {
    const onCancel = vi.fn();
    const { default: JobProgress } = await import('./JobProgress.svelte');
    render(JobProgress, { job: job(), cancelling: false, onCancel });

    expect(screen.getByText('30 / 90 phút · 33 %')).toBeTruthy();
    expect(screen.getByText('Đoạn 6 / 18')).toBeTruthy();
    expect(screen.getByText('Key thứ 2')).toBeTruthy();
    expect(screen.getByText('Lần thử 1')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Huỷ' }));
    expect(onCancel).toHaveBeenCalledWith('job-1');
  });

  it('shows the empty log state, then structured lines once events accumulate', async () => {
    mocks.logFor.mockReturnValue([
      { seq: 1, kind: 'updated', snapshot: job({ state: 'queued', keyOrdinal: null, attempt: null }) },
      { seq: 2, kind: 'updated', snapshot: job() },
      { seq: 3, kind: 'error', error: { category: 'network', code: 'network', detailRedacted: 'timed out' } },
      { seq: 4, kind: 'cancelled' },
    ]);
    const { default: JobProgress } = await import('./JobProgress.svelte');
    render(JobProgress, { job: job(), cancelling: false, onCancel: vi.fn() });

    const summary = screen.getByText('Nhật ký diễn biến');
    await fireEvent.click(summary);

    expect(screen.getByText(/Đang chờ trong hàng/)).toBeTruthy();
    expect(screen.getByText(/Lỗi \(network\): timed out/)).toBeTruthy();
    expect(screen.getByText('Đã huỷ')).toBeTruthy();
  });

  it('shows the empty-log message when there are no entries yet', async () => {
    const { default: JobProgress } = await import('./JobProgress.svelte');
    render(JobProgress, { job: job(), cancelling: false, onCancel: vi.fn() });

    expect(screen.getByText('Chưa có diễn biến nào')).toBeTruthy();
  });

  it('shows "N file đang chờ" when another Transcribe Job is queued in the registry', async () => {
    mocks.jobs.set('job-1', job());
    mocks.jobs.set('job-2', job({ jobId: 'job-2', state: 'queued' }));
    mocks.jobs.set('job-3', job({ jobId: 'job-3', state: 'queued' }));
    const { default: JobProgress } = await import('./JobProgress.svelte');
    render(JobProgress, { job: job(), cancelling: false, onCancel: vi.fn() });

    expect(screen.getByText('2 file đang chờ')).toBeTruthy();
  });

  it('does not show the queued count when no Transcribe Job is queued', async () => {
    mocks.jobs.set('job-1', job());
    const { default: JobProgress } = await import('./JobProgress.svelte');
    render(JobProgress, { job: job(), cancelling: false, onCancel: vi.fn() });

    expect(screen.queryByText(/file đang chờ/)).toBeNull();
  });

  it('ignores queued Chạy lại Jobs when counting "N file đang chờ"', async () => {
    mocks.jobs.set('job-1', job());
    mocks.jobs.set('job-2', job({ jobId: 'job-2', kind: 'rerun', state: 'queued' }));
    const { default: JobProgress } = await import('./JobProgress.svelte');
    render(JobProgress, { job: job(), cancelling: false, onCancel: vi.fn() });

    expect(screen.queryByText(/file đang chờ/)).toBeNull();
  });
});
