// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import Session from './Session.svelte';

const mocks = vi.hoisted(() => {
  class FakeChannel<T> {
    onmessage: (event: T) => void = () => {};
  }
  return {
    librarySessionGet: vi.fn(),
    jobsSubscribe: vi.fn(),
    jobsCancel: vi.fn(),
    FakeChannel,
  };
});

vi.mock('@tauri-apps/api/core', () => ({ Channel: mocks.FakeChannel }));

vi.mock('../lib/bindings', () => ({
  commands: {
    librarySessionGet: (...args: unknown[]) => mocks.librarySessionGet(...args),
    jobsSubscribe: (...args: unknown[]) => mocks.jobsSubscribe(...args),
    jobsCancel: (...args: unknown[]) => mocks.jobsCancel(...args),
  },
}));

type CapturedChannel = { onmessage: (event: unknown) => void };
let capturedChannel: CapturedChannel | null = null;

afterEach(() => cleanup());

beforeEach(async () => {
  i18n.applyPreference('vi');
  mocks.librarySessionGet.mockReset();
  mocks.jobsSubscribe.mockReset();
  mocks.jobsCancel.mockReset();
  capturedChannel = null;
  mocks.jobsSubscribe.mockImplementation((channel: CapturedChannel) => {
    capturedChannel = channel;
    return Promise.resolve({ status: 'ok', data: null });
  });
  const { jobsStore } = await import('../lib/stores/jobs.svelte');
  jobsStore.reset();
});

function job(overrides: Record<string, unknown> = {}) {
  return {
    jobId: 'job-1',
    sessionId: 'session-1',
    sourceName: 'meeting.wav',
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

describe('Session route', () => {
  it('shows the saved Phiên summary when the id matches a committed session', async () => {
    mocks.librarySessionGet.mockResolvedValue({
      status: 'ok',
      data: { kind: 'session', sessionId: 's1', title: 'cuộc họp', durationSec: 120, status: 'complete' },
    });
    render(Session, { routeParams: { id: 's1' } });

    expect(await screen.findByRole('heading', { name: 'cuộc họp' })).toBeTruthy();
    expect(mocks.jobsSubscribe).not.toHaveBeenCalled();
  });

  it('shows "not found" with a link home when the id matches nothing', async () => {
    mocks.librarySessionGet.mockResolvedValue({ status: 'ok', data: { kind: 'notFound' } });
    render(Session, { routeParams: { id: 'missing' } });

    expect(await screen.findByText('Tác vụ không còn')).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Về trang chủ' })).toBeTruthy();
  });

  it('subscribes to the job registry and renders live progress, then cancels on click', async () => {
    mocks.librarySessionGet.mockResolvedValue({
      status: 'ok',
      data: { kind: 'job', jobId: 'job-1', sessionId: 'session-1' },
    });
    mocks.jobsCancel.mockResolvedValue({ status: 'ok', data: 'cancelling' });
    render(Session, { routeParams: { id: 'session-1' } });

    await waitFor(() => expect(mocks.jobsSubscribe).toHaveBeenCalledTimes(1));
    capturedChannel!.onmessage({ kind: 'snapshot', seq: 1, jobs: [job()] });

    expect(await screen.findByText('meeting.wav')).toBeTruthy();
    expect(screen.getByText('30 / 90 phút · 33 %')).toBeTruthy();
    expect(screen.getByText('Đoạn 6 / 18')).toBeTruthy();
    expect(screen.getByText('Key thứ 2')).toBeTruthy();
    expect(screen.getByText('Lần thử 1')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Huỷ' }));
    expect(mocks.jobsCancel).toHaveBeenCalledWith('job-1');
  });

  it('re-resolves to the saved Phiên once the Job leaves the registry', async () => {
    mocks.librarySessionGet
      .mockResolvedValueOnce({
        status: 'ok',
        data: { kind: 'job', jobId: 'job-1', sessionId: 'session-1' },
      })
      .mockResolvedValueOnce({
        status: 'ok',
        data: { kind: 'session', sessionId: 'session-1', title: 'cuộc họp xong', durationSec: 60, status: 'complete' },
      });
    render(Session, { routeParams: { id: 'session-1' } });

    await waitFor(() => expect(mocks.jobsSubscribe).toHaveBeenCalledTimes(1));
    capturedChannel!.onmessage({ kind: 'snapshot', seq: 1, jobs: [job()] });
    expect(await screen.findByText('meeting.wav')).toBeTruthy();

    capturedChannel!.onmessage({ kind: 'result', seq: 2, jobId: 'job-1', sessionId: 'session-1' });

    expect(await screen.findByRole('heading', { name: 'cuộc họp xong' })).toBeTruthy();
    expect(mocks.librarySessionGet).toHaveBeenCalledTimes(2);
  });
});
