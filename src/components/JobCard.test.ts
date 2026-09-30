// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import { configureRouter } from '../lib/router';
import JobCard from './JobCard.svelte';
import type { JobSnapshot } from '../lib/bindings';

const mocks = vi.hoisted(() => ({
  jobsStore: {
    jobs: new Map<string, unknown>(),
  },
}));

vi.mock('../lib/stores/jobs.svelte', () => ({ jobsStore: mocks.jobsStore }));

function job(jobId: string, overrides: Partial<JobSnapshot> = {}): JobSnapshot {
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

afterEach(() => cleanup());

beforeEach(() => {
  configureRouter();
  i18n.applyPreference('vi');
  mocks.jobsStore.jobs = new Map();
});

describe('JobCard variant=full', () => {
  it('renders name, progress, chunk/key/attempt detail, and Mở/Huỷ links to /session/:sessionId', async () => {
    const onCancel = vi.fn();
    render(JobCard, { variant: 'full', job: job('j1'), cancelling: false, onCancel });

    expect(screen.getByText('fixture.wav')).toBeTruthy();
    expect(screen.getByText('30 / 90 phút · 33 %')).toBeTruthy();
    expect(screen.getByText(/Đoạn 6 \/ 18/)).toBeTruthy();
    expect(screen.getByText(/Key thứ 2/)).toBeTruthy();
    expect(screen.getByText(/Lần thử 1/)).toBeTruthy();

    const links = screen.getAllByRole('link', { name: /fixture\.wav|Mở/ });
    for (const link of links) {
      expect(link.getAttribute('href')).toBe('/session/session-j1');
    }

    await fireEvent.click(screen.getByRole('button', { name: 'Huỷ' }));
    expect(onCancel).toHaveBeenCalledWith('j1');
  });

  it('shows "Đang chờ quota…" only when waitingQuota is true', () => {
    render(JobCard, { variant: 'full', job: job('j1', { waitingQuota: true }) });
    expect(screen.getByText('Đang chờ quota…')).toBeTruthy();
  });

  it('does not show the waiting-quota line when false', () => {
    render(JobCard, { variant: 'full', job: job('j1', { waitingQuota: false }) });
    expect(screen.queryByText('Đang chờ quota…')).toBeNull();
  });

  it('shows "N file đang chờ" counting only queued transcribe Jobs elsewhere in the registry', () => {
    mocks.jobsStore.jobs = new Map([
      ['j1', job('j1')],
      ['j2', job('j2', { state: 'queued' })],
      ['j3', job('j3', { state: 'queued', kind: 'rerun' })],
    ]);
    render(JobCard, { variant: 'full', job: job('j1') });

    expect(screen.getByText('1 file đang chờ')).toBeTruthy();
  });

  // spec I/O Matrix "Single queued job": the card itself can show a queued
  // Job (Job đầu hàng, nothing running yet) — that Job is also `queued` in
  // the registry, so it must not count itself as "another" waiting file.
  it('does not show "N file đang chờ" when the only queued transcribe Job is the one shown on the card', () => {
    mocks.jobsStore.jobs = new Map([['j1', job('j1', { state: 'queued' })]]);
    render(JobCard, { variant: 'full', job: job('j1', { state: 'queued' }) });

    expect(screen.queryByText(/file đang chờ/)).toBeNull();
  });

  // spec I/O Matrix "Queued + another": once a second queued transcribe Job
  // exists elsewhere, it counts — even while the card itself shows a queued
  // Job that must stay excluded.
  it('shows "1 file đang chờ" when another queued transcribe Job exists besides the one shown on the card', () => {
    mocks.jobsStore.jobs = new Map([
      ['j1', job('j1', { state: 'queued' })],
      ['j2', job('j2', { state: 'queued' })],
    ]);
    render(JobCard, { variant: 'full', job: job('j1', { state: 'queued' }) });

    expect(screen.getByText('1 file đang chờ')).toBeTruthy();
  });

  it('uses "Đang chạy lại" for a rerun job even with no sourceName', () => {
    render(JobCard, { variant: 'full', job: job('j1', { kind: 'rerun', sourceName: null }) });
    expect(screen.getByText('Đang chạy lại')).toBeTruthy();
  });

  it('shows the cancelling label and disables the button while cancelling', () => {
    render(JobCard, { variant: 'full', job: job('j1'), cancelling: true });
    const button = screen.getByRole('button', { name: 'Đang huỷ…' });
    expect(button.hasAttribute('disabled')).toBe(true);
  });

  it('renders nothing when job is null', () => {
    const { container } = render(JobCard, { variant: 'full', job: null });
    expect(container.textContent?.trim()).toBe('');
  });
});

describe('JobCard variant=compact', () => {
  it('is hidden when the registry has no jobs', () => {
    render(JobCard, { variant: 'compact' });
    expect(screen.queryByText('Không có job nào đang chạy.')).toBeNull();
    expect(screen.queryByText('0')).toBeNull();
  });

  it('shows the running Job name + percent linking to /session/:sessionId, and the total running+queued count', () => {
    mocks.jobsStore.jobs = new Map([
      ['j1', job('j1')],
      ['j2', job('j2', { state: 'queued' })],
    ]);
    render(JobCard, { variant: 'compact' });

    expect(screen.getByText('2')).toBeTruthy();
    expect(screen.getByText('fixture.wav')).toBeTruthy();
    expect(screen.getByText('33 %')).toBeTruthy();
    const link = screen.getByRole('link', { name: /fixture\.wav/ });
    expect(link.getAttribute('href')).toBe('/session/session-j1');
  });

  it('shows only the count (no name/percent, no "no jobs" fallback) when jobs are queued but none is running yet', () => {
    mocks.jobsStore.jobs = new Map([['j1', job('j1', { state: 'queued' })]]);
    render(JobCard, { variant: 'compact' });

    expect(screen.getByText('1')).toBeTruthy();
    expect(screen.queryByText('Không có job nào đang chạy.')).toBeNull();
  });
});
