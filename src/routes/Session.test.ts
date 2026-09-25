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
    librarySessionDetail: vi.fn(),
    libraryProxyRelink: vi.fn(),
    libraryTranscriptExport: vi.fn(),
    jobsSubscribe: vi.fn(),
    jobsUnsubscribe: vi.fn().mockResolvedValue({ status: 'ok', data: null }),
    jobsCancel: vi.fn(),
    transcribeRerun: vi.fn(),
    clipboardWriteText: vi.fn(),
    settingsStore: { timestampOffsetSec: 0 },
    FakeChannel,
    intakeStore: {
      notices: [] as Array<{ id: string; variant: string; title: string; message: string }>,
      dismiss: vi.fn(),
    },
  };
});

// story 2.8: `<IntakeNotices>` mounts on Session too (spec Code Map:
// "notices render trên Home và Session") — mocked the same way Home.test.ts
// mocks it, so this suite stays about Session's own routing/job/rerun logic.
vi.mock('../lib/stores/intake.svelte', () => ({ intakeStore: mocks.intakeStore }));

vi.mock('@tauri-apps/api/core', () => ({
  Channel: mocks.FakeChannel,
  convertFileSrc: (path: string) => `asset://localhost/${path}`,
}));

vi.mock('../lib/bindings', () => ({
  commands: {
    librarySessionGet: (...args: unknown[]) => mocks.librarySessionGet(...args),
    librarySessionDetail: (...args: unknown[]) => mocks.librarySessionDetail(...args),
    libraryProxyRelink: (...args: unknown[]) => mocks.libraryProxyRelink(...args),
    libraryTranscriptExport: (...args: unknown[]) => mocks.libraryTranscriptExport(...args),
    jobsSubscribe: (...args: unknown[]) => mocks.jobsSubscribe(...args),
    jobsUnsubscribe: (...args: unknown[]) => mocks.jobsUnsubscribe(...args),
    jobsCancel: (...args: unknown[]) => mocks.jobsCancel(...args),
    transcribeRerun: (...args: unknown[]) => mocks.transcribeRerun(...args),
  },
}));

// `Player.svelte` reads `displayTimestamp` -> `settingsStore.timestampOffsetSec`;
// keep it a plain fake so this suite never boots the real store (which needs
// its own `events`/`commands.settingsGet` wiring — out of scope here).
vi.mock('../lib/stores/settings.svelte', () => ({
  settingsStore: mocks.settingsStore,
}));

type CapturedChannel = { onmessage: (event: unknown) => void };
let capturedChannel: CapturedChannel | null = null;

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

beforeEach(async () => {
  i18n.applyPreference('vi');
  mocks.librarySessionGet.mockReset();
  mocks.librarySessionDetail.mockReset();
  mocks.libraryProxyRelink.mockReset();
  mocks.libraryTranscriptExport.mockReset();
  mocks.clipboardWriteText.mockReset();
  mocks.settingsStore.timestampOffsetSec = 0;
  mocks.libraryTranscriptExport.mockResolvedValue({ status: 'ok', data: { saved: false, hasGaps: false } });
  mocks.clipboardWriteText.mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { writeText: (...args: unknown[]) => mocks.clipboardWriteText(...args) },
  });
  mocks.jobsSubscribe.mockReset();
  mocks.jobsUnsubscribe.mockReset().mockResolvedValue({ status: 'ok', data: null });
  mocks.jobsCancel.mockReset();
  mocks.transcribeRerun.mockReset();
  mocks.intakeStore.notices = [];
  mocks.intakeStore.dismiss.mockReset();
  capturedChannel = null;
  mocks.jobsSubscribe.mockImplementation((channel: CapturedChannel) => {
    capturedChannel = channel;
    return Promise.resolve({ status: 'ok', data: null });
  });
  Element.prototype.scrollIntoView = vi.fn();
  const { jobsStore } = await import('../lib/stores/jobs.svelte');
  jobsStore.reset();
});

function job(overrides: Record<string, unknown> = {}) {
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

function textSegment(idx: number, startSec: number, endSec: number, text: string) {
  return { idx, startSec, endSec, kind: 'text', gapReason: null, text };
}

function gapSegment(idx: number, startSec: number, endSec: number, gapReason: string) {
  return { idx, startSec, endSec, kind: 'gap', gapReason, text: '' };
}

function detail(overrides: Record<string, unknown> = {}) {
  return {
    sessionId: 's1',
    kind: 'file',
    title: 'cuộc họp',
    createdAt: Date.UTC(2026, 0, 15),
    durationSec: 120,
    recovered: false,
    sourceName: 'meeting.wav',
    proxyPath: '/data/media/s1/proxy.flac',
    transcript: {
      id: 't1',
      variant: 'primary',
      status: 'complete',
      model: 'gemini-flash-lite-latest',
      language: null,
      segments: [textSegment(0, 0, 10, 'xin chào'), textSegment(1, 10, 20, 'các bạn')],
    },
    ...overrides,
  };
}

describe('Session route', () => {
  it('shows the saved Phiên header and Segment list when the id matches a committed session', async () => {
    mocks.librarySessionGet.mockResolvedValue({
      status: 'ok',
      data: { kind: 'session', sessionId: 's1', title: 'cuộc họp', durationSec: 120, status: 'complete', partial: false, transcriptId: 't1' },
    });
    mocks.librarySessionDetail.mockResolvedValue({ status: 'ok', data: detail() });
    render(Session, { routeParams: { id: 's1' } });

    expect(await screen.findByRole('heading', { name: 'cuộc họp' })).toBeTruthy();
    expect(screen.getByText('xin chào')).toBeTruthy();
    expect(screen.getByText('các bạn')).toBeTruthy();
    expect(mocks.jobsSubscribe).not.toHaveBeenCalled();
  });

  it('searches the selected transcript, highlights matches, and cycles forward and backward', async () => {
    mocks.librarySessionGet.mockResolvedValue({
      status: 'ok',
      data: { kind: 'session', sessionId: 's1', title: 'cuộc họp', durationSec: 120, status: 'complete', partial: false, transcriptId: 't1' },
    });
    mocks.librarySessionDetail.mockResolvedValue({
      status: 'ok',
      data: detail({
        transcript: {
          id: 't1', variant: 'primary', status: 'complete', model: 'm', language: null,
          segments: [textSegment(0, 0, 10, 'Giao diện đầu'), textSegment(1, 10, 20, 'Giao diện sau')],
        },
      }),
    });
    render(Session, { routeParams: { id: 's1' } });

    const input = await screen.findByRole('searchbox', { name: 'Tìm trong transcript' });
    await fireEvent.input(input, { target: { value: '  giao   diện  ' } });
    expect(screen.getByText('1/2')).toBeTruthy();
    expect(document.querySelectorAll('mark').length).toBe(2);

    await fireEvent.click(screen.getByRole('button', { name: 'Tiếp' }));
    expect(screen.getByText('2/2')).toBeTruthy();
    await fireEvent.keyDown(input, { key: 'Enter' });
    expect(screen.getByText('1/2')).toBeTruthy();
    await fireEvent.keyDown(input, { key: 'Enter', shiftKey: true });
    expect(screen.getByText('2/2')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Trước' }));
    expect(screen.getByText('1/2')).toBeTruthy();
    expect(Element.prototype.scrollIntoView).toHaveBeenCalled();
  });

  it('shows 0/0 for an empty or absent query, disables navigation, and does not scroll', async () => {
    mocks.librarySessionGet.mockResolvedValue({
      status: 'ok',
      data: { kind: 'session', sessionId: 's1', title: 'cuộc họp', durationSec: 120, status: 'complete', partial: false, transcriptId: 't1' },
    });
    mocks.librarySessionDetail.mockResolvedValue({ status: 'ok', data: detail() });
    render(Session, { routeParams: { id: 's1' } });

    const input = await screen.findByRole('searchbox', { name: 'Tìm trong transcript' });
    expect(screen.getByText('0/0')).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Trước' }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole('button', { name: 'Tiếp' }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.input(input, { target: { value: 'không có' } });
    expect(screen.getByText('0/0')).toBeTruthy();
    expect(Element.prototype.scrollIntoView).not.toHaveBeenCalled();
  });

  it('focuses transcript search on Meta+F and Control+F in the saved detail view', async () => {
    mocks.librarySessionGet.mockResolvedValue({
      status: 'ok',
      data: { kind: 'session', sessionId: 's1', title: 'cuộc họp', durationSec: 120, status: 'complete', partial: false, transcriptId: 't1' },
    });
    mocks.librarySessionDetail.mockResolvedValue({ status: 'ok', data: detail() });
    render(Session, { routeParams: { id: 's1' } });

    const input = await screen.findByRole('searchbox', { name: 'Tìm trong transcript' });
    const { installKeymap } = await import('../lib/keymap');
    const removeKeymap = installKeymap(document);
    (input as HTMLInputElement).blur();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', metaKey: true, bubbles: true }));
    expect(document.activeElement).toBe(input);
    (input as HTMLInputElement).blur();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', ctrlKey: true, bubbles: true }));
    expect(document.activeElement).toBe(input);
    removeKeymap();
  });

  it('copies all transcript text and gap notes, shows a polite toast, and expires it after four seconds', async () => {
    mocks.librarySessionGet.mockResolvedValue({
      status: 'ok',
      data: { kind: 'session', sessionId: 's1', title: 'cuộc họp', durationSec: 120, status: 'partial', partial: true, transcriptId: 't1' },
    });
    mocks.settingsStore.timestampOffsetSec = 3_600;
    mocks.librarySessionDetail.mockResolvedValue({
      status: 'ok',
      data: detail({
        transcript: {
          id: 't1', variant: 'primary', status: 'partial', model: 'm', language: null,
          segments: [textSegment(0, 0, 10, 'xin chào'), gapSegment(1, 10, 20, 'chunk_failed')],
        },
      }),
    });
    render(Session, { routeParams: { id: 's1' } });
    const copyButton = await screen.findByRole('button', { name: 'Copy toàn bộ' });

    vi.useFakeTimers();
    await fireEvent.click(copyButton);
    await Promise.resolve();
    await Promise.resolve();
    expect(mocks.clipboardWriteText).toHaveBeenCalledWith(
      '[01:00:00] xin chào\n[01:00:10–01:00:20] Khoảng này bị lỗi khi transcribe',
    );
    const toast = screen.getByText('Đã copy transcript.');
    expect(toast.getAttribute('aria-live')).toBe('polite');
    await vi.advanceTimersByTimeAsync(3_999);
    expect(screen.queryByText('Đã copy transcript.')).toBeTruthy();
    await vi.advanceTimersByTimeAsync(1);
    expect(screen.queryByText('Đã copy transcript.')).toBeNull();
  });

  it('reports clipboard failures inline and never shows a success toast', async () => {
    mocks.librarySessionGet.mockResolvedValue({
      status: 'ok',
      data: { kind: 'session', sessionId: 's1', title: 'cuộc họp', durationSec: 120, status: 'complete', partial: false, transcriptId: 't1' },
    });
    mocks.librarySessionDetail.mockResolvedValue({ status: 'ok', data: detail() });
    mocks.clipboardWriteText.mockRejectedValue(new Error('denied'));
    render(Session, { routeParams: { id: 's1' } });

    await fireEvent.click(await screen.findByRole('button', { name: 'Copy toàn bộ' }));
    expect((await screen.findByRole('alert')).textContent).toContain('Không thể copy transcript.');
    expect(screen.queryByText('Đã copy transcript.')).toBeNull();
  });

  it('exports the selected transcript and offset; cancellation stays silent', async () => {
    mocks.librarySessionGet.mockResolvedValue({
      status: 'ok',
      data: { kind: 'session', sessionId: 's1', title: 'cuộc họp', durationSec: 120, status: 'partial', partial: true, transcriptId: 't1' },
    });
    mocks.settingsStore.timestampOffsetSec = 42;
    mocks.librarySessionDetail.mockResolvedValue({
      status: 'ok',
      data: detail({ transcript: { ...detail().transcript, status: 'partial', segments: [gapSegment(0, 0, 10, 'chunk_failed')] } }),
    });
    render(Session, { routeParams: { id: 's1' } });

    await fireEvent.click(await screen.findByRole('button', { name: 'Xuất TXT' }));
    expect(mocks.libraryTranscriptExport).toHaveBeenCalledWith('s1', 't1', 'txt', 42);
    expect(screen.queryByText('Đã lưu transcript.')).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('reports SRT gap omission after a successful save and shows export errors inline', async () => {
    mocks.librarySessionGet.mockResolvedValue({
      status: 'ok',
      data: { kind: 'session', sessionId: 's1', title: 'cuộc họp', durationSec: 120, status: 'partial', partial: true, transcriptId: 't1' },
    });
    mocks.librarySessionDetail.mockResolvedValue({
      status: 'ok',
      data: detail({ transcript: { ...detail().transcript, status: 'partial', segments: [gapSegment(0, 0, 10, 'chunk_failed')] } }),
    });
    mocks.libraryTranscriptExport.mockResolvedValueOnce({ status: 'ok', data: { saved: true, hasGaps: true } })
      .mockResolvedValueOnce({ status: 'error', error: { category: 'storage', code: 'storage', detailRedacted: 'disk full' } });
    render(Session, { routeParams: { id: 's1' } });

    await fireEvent.click(await screen.findByRole('button', { name: 'Xuất SRT' }));
    expect(await screen.findByText('Đã lưu SRT. Các khoảng thiếu không được đưa vào phụ đề.')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Xuất JSON' }));
    expect((await screen.findByRole('alert')).textContent).toContain('Không thể xuất transcript.');
    expect(screen.queryByText('Đã lưu transcript.')).toBeNull();
  });

  it('shows a partial banner listing the chunk_failed ranges, with two rerun buttons', async () => {
    mocks.librarySessionGet.mockResolvedValue({
      status: 'ok',
      data: { kind: 'session', sessionId: 's1', title: 'cuộc họp dở', durationSec: 120, status: 'partial', partial: true, transcriptId: 't1' },
    });
    mocks.librarySessionDetail.mockResolvedValue({
      status: 'ok',
      data: detail({
        title: 'cuộc họp dở',
        transcript: {
          id: 't1',
          variant: 'primary',
          status: 'partial',
          model: 'gemini-flash-lite-latest',
          language: null,
          segments: [
            textSegment(0, 0, 10, 'xin chào'),
            gapSegment(1, 10, 20, 'chunk_failed'),
          ],
        },
      }),
    });
    render(Session, { routeParams: { id: 's1' } });

    expect(await screen.findByText('Bản ghi còn thiếu một số đoạn do lỗi khi transcribe.')).toBeTruthy();
    expect(screen.getByText('00:10–00:20')).toBeTruthy();
    expect(screen.getByRole('button', { name: /Chạy lại phần thiếu/ })).toBeTruthy();
    expect(screen.getByRole('button', { name: /Chạy lại toàn bộ/ })).toBeTruthy();
  });

  it('starts a gap rerun with the exact scope and switches to the Job view (acceptance criterion)', async () => {
    mocks.librarySessionGet.mockResolvedValue({
      status: 'ok',
      data: { kind: 'session', sessionId: 's1', title: 'cuộc họp dở', durationSec: 120, status: 'partial', partial: true, transcriptId: 't1' },
    });
    mocks.librarySessionDetail.mockResolvedValue({
      status: 'ok',
      data: detail({
        transcript: {
          id: 't1',
          variant: 'primary',
          status: 'partial',
          model: 'm',
          language: null,
          segments: [
            textSegment(0, 0, 10, 'xin chào'),
            textSegment(1, 10, 20, 'các bạn'),
            textSegment(2, 20, 30, 'khoẻ không'),
            gapSegment(3, 30, 40, 'chunk_failed'),
          ],
        },
      }),
    });
    mocks.transcribeRerun.mockResolvedValue({ status: 'ok', data: { kind: 'started', jobId: 'job-rerun' } });
    render(Session, { routeParams: { id: 's1' } });

    const rerunButton = await screen.findByRole('button', { name: /Chạy lại khoảng này/ });
    await fireEvent.click(rerunButton);

    expect(mocks.transcribeRerun).toHaveBeenCalledWith('s1', 't1', { kind: 'gap', gapId: 3 });
    await waitFor(() => expect(mocks.jobsSubscribe).toHaveBeenCalledTimes(1));
    capturedChannel!.onmessage({
      kind: 'snapshot',
      seq: 1,
      jobs: [job({ jobId: 'job-rerun', sessionId: 's1', kind: 'rerun', sourceName: null })],
    });
    expect(await screen.findByRole('heading', { name: 'Đang chạy lại' })).toBeTruthy();
  });

  it('rerun missing / all buttons call transcribeRerun with the matching scope', async () => {
    mocks.librarySessionGet.mockResolvedValue({
      status: 'ok',
      data: { kind: 'session', sessionId: 's1', title: 'cuộc họp dở', durationSec: 120, status: 'partial', partial: true, transcriptId: 't1' },
    });
    mocks.librarySessionDetail.mockResolvedValue({
      status: 'ok',
      data: detail({
        transcript: {
          id: 't1',
          variant: 'primary',
          status: 'partial',
          model: 'm',
          language: null,
          segments: [gapSegment(0, 0, 10, 'chunk_failed')],
        },
      }),
    });
    mocks.transcribeRerun.mockResolvedValue({ status: 'ok', data: { kind: 'nothingToRerun' } });
    render(Session, { routeParams: { id: 's1' } });

    const allButton = await screen.findByRole('button', { name: /Chạy lại toàn bộ/ });
    await fireEvent.click(allButton);
    expect(mocks.transcribeRerun).toHaveBeenCalledWith('s1', 't1', { kind: 'all' });
    expect(await screen.findByText('Không còn đoạn nào thiếu để chạy lại.')).toBeTruthy();
  });

  it('shows "no audio" and relinks the proxy on request, then reloads the detail', async () => {
    mocks.librarySessionGet.mockResolvedValue({
      status: 'ok',
      data: { kind: 'session', sessionId: 's1', title: 'cuộc họp', durationSec: 120, status: 'complete', partial: false, transcriptId: 't1' },
    });
    mocks.librarySessionDetail
      .mockResolvedValueOnce({ status: 'ok', data: detail({ proxyPath: null }) })
      .mockResolvedValueOnce({ status: 'ok', data: detail({ proxyPath: '/data/media/s1/proxy.flac' }) });
    mocks.libraryProxyRelink.mockResolvedValue({ status: 'ok', data: 'relinked' });
    render(Session, { routeParams: { id: 's1' } });

    expect(await screen.findByText('Không có audio · Chọn lại file nguồn')).toBeTruthy();
    const relinkButton = screen.getByRole('button', { name: 'Chọn lại file nguồn' });
    await fireEvent.click(relinkButton);

    expect(mocks.libraryProxyRelink).toHaveBeenCalledWith('s1');
    await waitFor(() => expect(mocks.librarySessionDetail).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.queryByText('Không có audio · Chọn lại file nguồn')).toBeNull());
  });

  it('shows a hash-mismatch message and keeps the Transcript unchanged', async () => {
    mocks.librarySessionGet.mockResolvedValue({
      status: 'ok',
      data: { kind: 'session', sessionId: 's1', title: 'cuộc họp', durationSec: 120, status: 'complete', partial: false, transcriptId: 't1' },
    });
    mocks.librarySessionDetail.mockResolvedValue({ status: 'ok', data: detail({ proxyPath: null }) });
    mocks.libraryProxyRelink.mockResolvedValue({ status: 'ok', data: 'hashMismatch' });
    render(Session, { routeParams: { id: 's1' } });

    const relinkButton = await screen.findByRole('button', { name: 'Chọn lại file nguồn' });
    await fireEvent.click(relinkButton);

    expect(await screen.findByText('File này không khớp với bản ghi gốc.')).toBeTruthy();
    expect(mocks.librarySessionDetail).toHaveBeenCalledTimes(1);
  });

  it('shows "not found" when the detail lookup returns null after a session lookup', async () => {
    mocks.librarySessionGet.mockResolvedValue({
      status: 'ok',
      data: { kind: 'session', sessionId: 'gone', title: 'x', durationSec: 1, status: 'complete', partial: false, transcriptId: null },
    });
    mocks.librarySessionDetail.mockResolvedValue({ status: 'ok', data: null });
    render(Session, { routeParams: { id: 'gone' } });

    expect(await screen.findByText('Tác vụ không còn')).toBeTruthy();
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

  it('shows the job event log panel and appends a line per update', async () => {
    mocks.librarySessionGet.mockResolvedValue({
      status: 'ok',
      data: { kind: 'job', jobId: 'job-1', sessionId: 'session-1' },
    });
    render(Session, { routeParams: { id: 'session-1' } });

    await waitFor(() => expect(mocks.jobsSubscribe).toHaveBeenCalledTimes(1));
    capturedChannel!.onmessage({ kind: 'snapshot', seq: 1, jobs: [job()] });
    await screen.findByText('meeting.wav');

    const summary = screen.getByText('Nhật ký diễn biến');
    await fireEvent.click(summary);
    expect(screen.getAllByText(/Đoạn 6 \/ 18/).length).toBeGreaterThan(0);
  });

  it('re-resolves to the saved Phiên once the Job leaves the registry', async () => {
    mocks.librarySessionGet
      .mockResolvedValueOnce({
        status: 'ok',
        data: { kind: 'job', jobId: 'job-1', sessionId: 'session-1' },
      })
      .mockResolvedValueOnce({
        status: 'ok',
        data: { kind: 'session', sessionId: 'session-1', title: 'cuộc họp xong', durationSec: 60, status: 'complete', partial: false, transcriptId: 't1' },
      });
    mocks.librarySessionDetail.mockResolvedValue({
      status: 'ok',
      data: detail({ sessionId: 'session-1', title: 'cuộc họp xong' }),
    });
    render(Session, { routeParams: { id: 'session-1' } });

    await waitFor(() => expect(mocks.jobsSubscribe).toHaveBeenCalledTimes(1));
    capturedChannel!.onmessage({ kind: 'snapshot', seq: 1, jobs: [job()] });
    expect(await screen.findByText('meeting.wav')).toBeTruthy();

    capturedChannel!.onmessage({ kind: 'result', seq: 2, jobId: 'job-1', sessionId: 'session-1' });

    expect(await screen.findByRole('heading', { name: 'cuộc họp xong' })).toBeTruthy();
    expect(mocks.librarySessionGet).toHaveBeenCalledTimes(2);
  });

  // story 2.8 Code Map: "notices render trên Home và Session".
  it('renders intake notices (from a file dropped while on this screen) regardless of view state', async () => {
    mocks.librarySessionGet.mockResolvedValue({
      status: 'ok',
      data: { kind: 'session', sessionId: 's1', title: 'cuộc họp', durationSec: 120, status: 'complete', partial: false, transcriptId: 't1' },
    });
    mocks.librarySessionDetail.mockResolvedValue({ status: 'ok', data: detail() });
    mocks.intakeStore.notices = [
      { id: 'n1', variant: 'info', title: 'other.mp4', message: 'Đã thêm vào hàng đợi transcribe.' },
    ];
    render(Session, { routeParams: { id: 's1' } });

    expect(await screen.findByRole('heading', { name: 'cuộc họp' })).toBeTruthy();
    expect(screen.getByText('Đã thêm vào hàng đợi transcribe.')).toBeTruthy();
  });
});
