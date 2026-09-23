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
  type TranscribeStartOutcome,
} from '../bindings';

export type JobsStatus = 'idle' | 'subscribed' | 'error';

export function createJobsStore() {
  let jobs = $state<Map<string, JobSnapshot>>(new Map());
  let status = $state<JobsStatus>('idle');
  let error = $state<AppError | null>(null);
  // `true` once the current subscription's first event (always a Snapshot)
  // has been applied — before that, an empty `jobs` map means "no data yet",
  // not "no Job running" (a consumer must not read absence as truth until
  // this flips true).
  let synced = $state(false);

  let lastSeq: number | null = null;
  let generation = 0;

  function applyEvent(event: JobEvent, forGeneration: number): void {
    // A message from a Channel created by an earlier subscribe() call that
    // has since been superseded (unsubscribe/resubscribe) — ignore it.
    if (forGeneration !== generation) return;

    if (event.kind === 'snapshot') {
      const next = new Map<string, JobSnapshot>();
      for (const job of event.jobs) next.set(job.jobId, job);
      jobs = next;
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
      return;
    }
    // 'result' | 'error' | 'cancelled': the Job left the registry.
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

  /** Call from a route's mount. Safe to call again (e.g. after an error). */
  function subscribe(): Promise<void> {
    return doSubscribe();
  }

  /** Call from a route's unmount — drops the Channel and clears local state
   * so a later `subscribe()` starts from a clean snapshot. */
  function unsubscribe(): void {
    generation += 1;
    jobs = new Map();
    lastSeq = null;
    synced = false;
    status = 'idle';
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
    status = 'idle';
    error = null;
    synced = false;
    lastSeq = null;
  }

  return {
    get jobs() {
      return jobs;
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
    subscribe,
    unsubscribe,
    start,
    cancel,
    reset,
  };
}

export const jobsStore = createJobsStore();
