// Store domain `jobs` (story 2.4): the only frontend owner of
// `jobsSubscribe`/`jobsCancel`/`transcribeStart`. Owns one `Channel<JobEvent>`
// subscription at a time — `subscribe()`/`unsubscribe()` are meant to be
// called from a route's mount/unmount (currently only `Session.svelte`).
//
// Snapshot + subsequent events are applied in order; a `seq` gap (a message
// this Channel never received — dropped connection, remount racing the old
// Channel) triggers an automatic re-subscribe for a fresh snapshot (spec I/O
// Matrix "Remount UI": "Hụt `seq` -> UI subscribe lại"), never a half-stale
// job list.
import { Channel } from '@tauri-apps/api/core';
import {
  commands,
  type AppError,
  type CancelOutcome,
  type JobEvent,
  type JobSnapshot,
  type RerunScope,
  type TranscribeRerunOutcome,
  type TranscribeStartOutcome,
} from '../bindings';

export type JobsStatus = 'idle' | 'subscribed' | 'error';

/** Một dòng "diễn biến" ngắn cho panel log của `JobProgress.svelte` (story
 * 2.7, spec Tasks: "lưu log diễn biến ngắn theo `jobId`"). Chỉ giữ dữ liệu có
 * cấu trúc — không bao giờ một chuỗi đã định dạng sẵn — để `JobProgress`
 * dịch qua i18n lúc hiển thị, và để không thể vô tình nhét path/transcript
 * vào một chuỗi tự do (spec Always: "log không chứa path đầy đủ hay
 * transcript"). `snapshot`/`error` chỉ mang field đã có sẵn ở kiểu tương ứng
 * (không có `path` hay nội dung transcript ở đó).
 */
export type JobLogEntry =
  | { seq: number; kind: 'updated'; snapshot: JobSnapshot }
  | { seq: number; kind: 'result' }
  | { seq: number; kind: 'error'; error: AppError }
  | { seq: number; kind: 'cancelled' };

/** Giữ tối đa ~200 dòng mỗi Job (spec Tasks) — cắt bớt dòng cũ nhất trước. */
const MAX_LOG_LINES_PER_JOB = 200;

export function createJobsStore() {
  let jobs = $state<Map<string, JobSnapshot>>(new Map());
  let logs = $state<Map<string, JobLogEntry[]>>(new Map());
  let status = $state<JobsStatus>('idle');
  let error = $state<AppError | null>(null);
  // `true` once the current subscription's first event (always a Snapshot)
  // has been applied — before that, an empty `jobs` map means "no data yet",
  // not "no Job running" (a consumer must not read absence as truth until
  // this flips true).
  let synced = $state(false);
  // Tăng mỗi khi một Job commit (event `result`) — story 2.9: `library`
  // store theo dõi giá trị này để tự tải lại danh sách Home mà không cần
  // người dùng thao tác (spec Code Map: "đếm `resultSeq` ... để store khác
  // phản ứng"). Chỉ tăng, không bao giờ giảm hay reset ngoài `reset()`.
  let resultSeq = $state(0);

  let lastSeq: number | null = null;
  let generation = 0;
  // Đếm tham chiếu subscribe/unsubscribe (spec Design Notes): sidebar
  // (`AppShell`, gắn suốt vòng đời app) và `/session/:id` cùng gọi
  // `subscribe()`/`unsubscribe()` độc lập mà không xoá state của nhau —
  // `subscribe()` chỉ thật sự mở Channel ở lần đầu (đếm 0 -> 1) hoặc khi
  // đang `error` (mở lại); `unsubscribe()` chỉ xoá state khi đếm về 0.
  let subscriberCount = 0;
  let pendingSubscribe: Promise<void> | null = null;
  // The most recently created Channel, kept only so `unsubscribe()` can
  // release it backend-side via `jobsUnsubscribe(channel.id)` -- never read
  // for anything else.
  let currentChannel: Channel<JobEvent> | null = null;

  function appendLog(jobId: string, entry: JobLogEntry): void {
    const nextLogs = new Map(logs);
    const existing = nextLogs.get(jobId) ?? [];
    const appended = [...existing, entry];
    const trimmed =
      appended.length > MAX_LOG_LINES_PER_JOB
        ? appended.slice(appended.length - MAX_LOG_LINES_PER_JOB)
        : appended;
    nextLogs.set(jobId, trimmed);
    logs = nextLogs;
  }

  function applyEvent(event: JobEvent, forGeneration: number): void {
    // A message from a Channel created by an earlier subscribe() call that
    // has since been superseded (unsubscribe/resubscribe) — ignore it.
    if (forGeneration !== generation) return;

    if (event.kind === 'snapshot') {
      const next = new Map<string, JobSnapshot>();
      const nextLogs = new Map<string, JobLogEntry[]>();
      for (const job of event.jobs) {
        next.set(job.jobId, job);
        nextLogs.set(job.jobId, [{ seq: event.seq, kind: 'updated', snapshot: job }]);
      }
      jobs = next;
      logs = nextLogs;
      lastSeq = event.seq;
      synced = true;
      return;
    }

    if (lastSeq !== null && event.seq !== lastSeq + 1) {
      void doSubscribe();
      return;
    }
    lastSeq = event.seq;

    if (event.kind === 'updated') {
      const next = new Map(jobs);
      next.set(event.job.jobId, event.job);
      jobs = next;
      appendLog(event.job.jobId, { seq: event.seq, kind: 'updated', snapshot: event.job });
      return;
    }
    // 'result' | 'error' | 'cancelled': the Job left the registry.
    if (event.kind === 'result') {
      appendLog(event.jobId, { seq: event.seq, kind: 'result' });
      resultSeq += 1;
    } else if (event.kind === 'error') {
      appendLog(event.jobId, { seq: event.seq, kind: 'error', error: event.error });
    } else {
      appendLog(event.jobId, { seq: event.seq, kind: 'cancelled' });
    }
    const next = new Map(jobs);
    next.delete(event.jobId);
    jobs = next;
  }

  async function doSubscribe(): Promise<void> {
    generation += 1;
    const forGeneration = generation;
    lastSeq = null;
    synced = false;
    const channel = new Channel<JobEvent>();
    channel.onmessage = (event) => applyEvent(event, forGeneration);
    currentChannel = channel;

    try {
      const result = await commands.jobsSubscribe(channel);
      if (forGeneration !== generation) return;
      if (result.status === 'ok') {
        status = 'subscribed';
        error = null;
      } else {
        status = 'error';
        error = result.error;
      }
    } catch {
      if (forGeneration !== generation) return;
      status = 'error';
      error = null;
    }
  }

  /** Call from a mount (route or shell). Reference-counted (spec Design
   * Notes): the first call opens the Channel; later calls while already
   * subscribed just bump the count and reuse the existing subscription —
   * except when the current state is `error`, which reopens it. Multiple
   * callers (sidebar + `/session/:id`) can hold a subscription at once
   * without tearing down each other's state. */
  function subscribe(): Promise<void> {
    subscriberCount += 1;
    if (subscriberCount === 1 || (status === 'error' && !pendingSubscribe)) {
      const request = doSubscribe().finally(() => {
        if (pendingSubscribe === request) pendingSubscribe = null;
      });
      pendingSubscribe = request;
      return request;
    }
    return pendingSubscribe ?? Promise.resolve();
  }

  /** Call from a mount's teardown. Reference-counted counterpart of
   * `subscribe()`: only drops the Channel and clears local state once every
   * caller has unsubscribed (count back to 0) — one caller leaving must not
   * blank the state another caller (e.g. the sidebar) still reads. */
  function unsubscribe(): void {
    if (subscriberCount === 0) return;
    subscriberCount -= 1;
    if (subscriberCount > 0) return;
    generation += 1;
    jobs = new Map();
    logs = new Map();
    lastSeq = null;
    synced = false;
    status = 'idle';
    pendingSubscribe = null;
    // Release the channel backend-side so its JobEvent subscription actually
    // stops (spec Always: "jobsStore.unsubscribe() calls it with its
    // channel's id"). Fire-and-forget: the actor's unsubscribe is idempotent
    // and this runs from teardown, which cannot usefully await it.
    if (currentChannel !== null) {
      const channelId = currentChannel.id;
      currentChannel = null;
      void commands.jobsUnsubscribe(channelId).catch(() => {});
    }
  }

  async function start(path: string): Promise<TranscribeStartOutcome | { error: AppError }> {
    try {
      const result = await commands.transcribeStart(path);
      return result.status === 'ok' ? result.data : { error: result.error };
    } catch {
      return {
        error: { category: 'network', code: 'network', detailRedacted: 'transcribe start unavailable' },
      };
    }
  }

  /** Chạy lại (2.5): scope `missing`/`all`/`gap(gap_id)` — see
   * `RerunScope` in bindings.ts. Mirrors `start()`'s try/catch-to-typed-error
   * contract so callers never need a bare `try`/`catch` of their own. */
  async function rerun(
    sessionId: string,
    transcriptId: string,
    scope: RerunScope,
  ): Promise<TranscribeRerunOutcome | { error: AppError }> {
    try {
      const result = await commands.transcribeRerun(sessionId, transcriptId, scope);
      return result.status === 'ok' ? result.data : { error: result.error };
    } catch {
      return {
        error: { category: 'network', code: 'network', detailRedacted: 'transcribe rerun unavailable' },
      };
    }
  }

  async function cancel(jobId: string): Promise<CancelOutcome | null> {
    try {
      const result = await commands.jobsCancel(jobId);
      return result.status === 'ok' ? result.data : null;
    } catch {
      return null;
    }
  }

  /** Test-only seam: resets every field without touching a live Channel. */
  function reset(): void {
    generation += 1;
    jobs = new Map();
    logs = new Map();
    status = 'idle';
    error = null;
    synced = false;
    lastSeq = null;
    resultSeq = 0;
    subscriberCount = 0;
    pendingSubscribe = null;
  }

  return {
    get jobs() {
      return jobs;
    },
    /** Log diễn biến ngắn của một Job (`JobProgress.svelte`'s panel log) —
     * `[]` khi Job chưa có dòng nào (ví dụ trước khi snapshot đầu tiên tới). */
    logFor(jobId: string): JobLogEntry[] {
      return logs.get(jobId) ?? [];
    },
    get status() {
      return status;
    },
    get error() {
      return error;
    },
    get synced() {
      return synced;
    },
    /** Tăng mỗi lần một Job commit (event `result`) — theo dõi giá trị này
     * (không phải nội dung `jobs`) để biết "có commit mới" mà không phải so
     * sánh snapshot cũ/mới (spec Code Map). */
    get resultSeq() {
      return resultSeq;
    },
    subscribe,
    unsubscribe,
    start,
    rerun,
    cancel,
    reset,
  };
}

export const jobsStore = createJobsStore();
