<script lang="ts">
  // Thẻ tiến độ Job (nội dung 2.4: phút audio, Chunk, key, lần thử, chờ
  // quota, nút Huỷ) + panel log `<details>` mới ở story 2.7 (spec Tasks:
  // "JobProgress giữ nội dung 2.4 + panel log `<details>`") liệt kê diễn
  // biến tích luỹ từ `jobsStore.logFor(jobId)` — mỗi dòng dịch qua i18n từ
  // dữ liệu có cấu trúc (`JobLogEntry`), không phải một chuỗi log thô.
  import { i18n } from '../../i18n/index.svelte';
  import { jobsStore } from '../../lib/stores/jobs.svelte';
  import type { JobSnapshot } from '../../lib/bindings';

  let {
    job,
    cancelling,
    onCancel,
  }: {
    job: JobSnapshot;
    cancelling: boolean;
    onCancel: (jobId: string) => void;
  } = $props();

  function minutes(ms: number): number {
    return Math.floor(ms / 60_000);
  }

  function percent(processedMs: number, totalMs: number): number {
    if (totalMs <= 0) return 0;
    return Math.min(100, Math.round((processedMs / totalMs) * 100));
  }

  const log = $derived(jobsStore.logFor(job.jobId));

  function logLineLabel(entry: (typeof log)[number]): string {
    if (entry.kind === 'result') return i18n.t('session.job.logResult');
    if (entry.kind === 'cancelled') return i18n.t('session.job.logCancelled');
    if (entry.kind === 'error') {
      return i18n.t('session.job.logError', {
        category: entry.error.category,
        detail: entry.error.detailRedacted,
      });
    }
    const snapshot = entry.snapshot;
    const parts = [
      snapshot.state === 'queued' ? i18n.t('session.job.stateQueued') : i18n.t('session.job.stateRunning'),
      i18n.t('session.job.chunkLabel', { index: snapshot.chunkIndex, total: snapshot.chunkCount }),
    ];
    if (snapshot.keyOrdinal !== null) parts.push(i18n.t('session.job.keyLabel', { ordinal: snapshot.keyOrdinal }));
    if (snapshot.attempt !== null) parts.push(i18n.t('session.job.attemptLabel', { attempt: snapshot.attempt }));
    if (snapshot.waitingQuota) parts.push(i18n.t('session.job.waitingQuota'));
    return parts.join(' · ');
  }
</script>

<div class="job-card">
  <p class="job-state">
    {job.state === 'queued' ? i18n.t('session.job.stateQueued') : i18n.t('session.job.stateRunning')}
  </p>
  <p class="job-progress">
    {i18n.t('session.job.progressLabel', {
      processed: minutes(job.processedMs),
      total: Math.max(1, minutes(job.totalMs)),
      percent: percent(job.processedMs, job.totalMs),
    })}
  </p>
  <p class="job-detail">
    {i18n.t('session.job.chunkLabel', { index: job.chunkIndex, total: job.chunkCount })}
  </p>
  {#if job.keyOrdinal !== null}
    <p class="job-detail">{i18n.t('session.job.keyLabel', { ordinal: job.keyOrdinal })}</p>
  {/if}
  {#if job.attempt !== null}
    <p class="job-detail">{i18n.t('session.job.attemptLabel', { attempt: job.attempt })}</p>
  {/if}
  {#if job.waitingQuota}
    <p class="job-waiting" role="status">{i18n.t('session.job.waitingQuota')}</p>
  {/if}
  <button
    type="button"
    class="button button-secondary"
    disabled={cancelling}
    onclick={() => onCancel(job.jobId)}
  >
    {cancelling ? i18n.t('session.job.cancelling') : i18n.t('session.job.cancelAction')}
  </button>

  <details class="job-log">
    <summary>{i18n.t('session.job.logSummary')}</summary>
    {#if log.length === 0}
      <p class="job-log-empty">{i18n.t('session.job.logEmpty')}</p>
    {:else}
      <ul class="job-log-list">
        {#each log as entry (entry.seq)}
          <li>{logLineLabel(entry)}</li>
        {/each}
      </ul>
    {/if}
  </details>
</div>

<style>
  .job-card {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-6);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
    background: var(--color-surface);
  }

  .job-state {
    margin: 0;
    color: var(--color-text-secondary);
    font-size: var(--text-label-size);
    font-weight: 600;
  }

  .job-progress {
    margin: 0;
    font-family: var(--font-mono);
    font-size: var(--text-h2-size);
    font-variant-numeric: tabular-nums;
  }

  .job-detail {
    margin: 0;
    color: var(--color-text-secondary);
    font-size: var(--text-help-size);
  }

  .job-waiting {
    margin: 0;
    color: var(--color-warning);
    font-size: var(--text-help-size);
    font-weight: 600;
  }

  .button {
    display: inline-flex;
    width: fit-content;
    min-height: 36px;
    align-items: center;
    justify-content: center;
    margin-top: var(--space-2);
    padding: 0 14px;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: var(--text-body-size);
    font-weight: 500;
    cursor: pointer;
  }

  .button:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }

  .job-log {
    margin-top: var(--space-3);
    padding-top: var(--space-3);
    border-top: 1px solid var(--color-border);
  }

  .job-log summary {
    color: var(--color-text-secondary);
    font-size: var(--text-label-size);
    font-weight: 600;
    cursor: pointer;
  }

  .job-log-empty {
    margin: var(--space-2) 0 0;
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
  }

  .job-log-list {
    display: grid;
    gap: var(--space-1);
    margin: var(--space-2) 0 0;
    padding: 0;
    list-style: none;
    color: var(--color-text-secondary);
    font-family: var(--font-mono);
    font-size: var(--text-help-size);
  }
</style>
