<script lang="ts">
  // Card job dùng chung hai nơi (spec Boundaries/Code Map): biến thể `full`
  // ở đầu danh sách Home (một Job cụ thể được truyền vào — Job đang chạy,
  // hoặc Job đầu hàng nếu chưa có Job chạy) và biến thể `compact` ở sidebar
  // (tự đọc `jobsStore` để đếm/tìm Job đang chạy, nhìn thấy từ mọi màn — spec
  // Always: "Sidebar: thay placeholder bằng card thu gọn hiện trên mọi
  // màn"). Số liệu/nhãn tái dùng đúng khoá i18n `session.job.*` của
  // `JobProgress.svelte` (Code Map: "tách helper chung nếu cần thay vì
  // chép") thay vì định nghĩa lại.
  import { link } from '@keenmate/svelte-spa-router';
  import { i18n } from '../i18n/index.svelte';
  import { jobsStore } from '../lib/stores/jobs.svelte';
  import type { JobSnapshot } from '../lib/bindings';

  let {
    variant,
    job = null,
    cancelling = false,
    onCancel,
  }: {
    variant: 'full' | 'compact';
    /** Bắt buộc cho `full` — Home tự chọn Job đang chạy (hoặc Job đầu hàng)
     * để truyền vào. Bỏ qua ở `compact` (đọc thẳng `jobsStore`). */
    job?: JobSnapshot | null;
    cancelling?: boolean;
    onCancel?: (jobId: string) => void;
  } = $props();

  function minutes(ms: number): number {
    return Math.floor(ms / 60_000);
  }

  function percent(processedMs: number, totalMs: number): number {
    if (totalMs <= 0) return 0;
    return Math.min(100, Math.round((processedMs / totalMs) * 100));
  }

  function displayName(snapshot: JobSnapshot): string {
    return snapshot.kind === 'rerun'
      ? i18n.t('session.job.kindRerun')
      : (snapshot.sourceName ?? i18n.t('session.header.titleJob'));
  }

  // `compact`: đếm/tìm trên toàn bộ `jobsStore.jobs` (không chỉ Job này màn
  // đang xem) — sidebar nhìn thấy từ mọi màn nên luôn phản ánh registry đầy
  // đủ (spec Always).
  const allJobs = $derived(Array.from(jobsStore.jobs.values()));
  const runningCount = $derived(allJobs.filter((j) => j.state === 'running').length);
  const queuedCount = $derived(allJobs.filter((j) => j.state === 'queued').length);
  const totalCount = $derived(runningCount + queuedCount);
  const primaryRunning = $derived(allJobs.find((j) => j.state === 'running') ?? null);

  // "N file đang chờ" (story 2.8 Always, tái dùng ở `full`) — mọi Job
  // Transcribe `queued` trong registry, không chỉ Job đang hiện trên card,
  // nhưng loại trừ chính Job đang hiện: khi Job đầu hàng (chưa có Job chạy)
  // hiện trên card này, nó cũng nằm trong `allJobs` với `state === 'queued'`
  // — không loại nó ra sẽ tự đếm chính mình là "một file khác đang chờ"
  // (spec Boundaries: "excludes the job shown on the card").
  const queuedTranscribeCount = $derived(
    allJobs.filter((j) => j.kind === 'transcribe' && j.state === 'queued' && j.jobId !== job?.jobId).length,
  );
</script>

{#if variant === 'compact'}
  <section class="job-card-compact" aria-label={i18n.t('app.shell.runningJobs')}>
    <div class="job-heading">
      <span>{i18n.t('app.shell.runningJobs')}</span>
      <span class="job-count">{totalCount}</span>
    </div>
    {#if primaryRunning}
      <a class="job-compact-link" href={`/session/${primaryRunning.sessionId}`} use:link>
        <span class="job-compact-name">{displayName(primaryRunning)}</span>
        <span class="job-compact-percent mono">
          {i18n.t('home.jobCard.percent', { percent: percent(primaryRunning.processedMs, primaryRunning.totalMs) })}
        </span>
      </a>
    {:else if totalCount === 0}
      <p class="job-compact-empty">{i18n.t('app.shell.noRunningJobs')}</p>
    {/if}
  </section>
{:else if job}
  <div class="job-card-full">
    <a class="job-card-title" href={`/session/${job.sessionId}`} use:link>{displayName(job)}</a>
    <p class="job-progress">
      {i18n.t('session.job.progressLabel', {
        processed: minutes(job.processedMs),
        total: Math.max(1, minutes(job.totalMs)),
        percent: percent(job.processedMs, job.totalMs),
      })}
    </p>
    <p class="job-detail">
      {i18n.t('session.job.chunkLabel', { index: job.chunkIndex, total: job.chunkCount })}
      {#if job.keyOrdinal !== null}
        · {i18n.t('session.job.keyLabel', { ordinal: job.keyOrdinal })}
      {/if}
      {#if job.attempt !== null}
        · {i18n.t('session.job.attemptLabel', { attempt: job.attempt })}
      {/if}
    </p>
    {#if job.waitingQuota}
      <p class="job-waiting" role="status">{i18n.t('session.job.waitingQuota')}</p>
    {/if}
    {#if queuedTranscribeCount > 0}
      <p class="job-detail" role="status">
        {i18n.t('session.job.queuedCount', { count: queuedTranscribeCount })}
      </p>
    {/if}
    <div class="job-actions">
      <a class="button button-secondary" href={`/session/${job.sessionId}`} use:link>
        {i18n.t('home.jobCard.open')}
      </a>
      <button
        type="button"
        class="button button-danger-soft"
        disabled={cancelling}
        onclick={() => onCancel?.(job.jobId)}
      >
        {cancelling ? i18n.t('session.job.cancelling') : i18n.t('session.job.cancelAction')}
      </button>
    </div>
  </div>
{/if}

<style>
  .job-card-full {
    display: grid;
    gap: var(--space-2);
    margin-bottom: var(--space-4);
    padding: var(--space-6);
    border: 1px solid var(--color-warning-border);
    border-radius: var(--radius-xl);
    background: var(--color-warning-soft);
  }

  .job-card-title {
    justify-self: start;
    color: var(--color-text);
    font-weight: 600;
    text-decoration: none;
  }

  .job-card-title:hover {
    text-decoration: underline;
  }

  .job-progress {
    margin: 0;
    color: var(--color-text);
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

  .job-actions {
    display: flex;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }

  .button {
    display: inline-flex;
    min-height: 36px;
    align-items: center;
    justify-content: center;
    padding: 0 14px;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font-family: inherit;
    font-size: var(--text-body-size);
    font-weight: 500;
    text-decoration: none;
    cursor: pointer;
  }

  .button-danger-soft {
    border-color: var(--color-danger-border);
    color: var(--color-danger-strong);
  }

  .button:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }

  .job-card-compact {
    padding: var(--space-3);
    border: 1px solid var(--color-warning-border);
    border-radius: var(--radius-lg);
    background: var(--color-warning-soft);
  }

  .job-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: var(--space-2);
    color: var(--color-text-secondary);
    font-size: var(--text-label-size);
    font-weight: 600;
  }

  .job-count {
    display: grid;
    min-width: 20px;
    min-height: 20px;
    padding: 0 6px;
    place-items: center;
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    font-family: var(--font-mono);
    font-size: var(--text-help-size);
    font-variant-numeric: tabular-nums;
  }

  .job-compact-empty {
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
  }

  .job-compact-link {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    color: var(--color-text);
    text-decoration: none;
  }

  .job-compact-link:hover {
    text-decoration: underline;
  }

  .job-compact-name {
    min-width: 0;
    overflow: hidden;
    font-size: var(--text-label-size);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .job-compact-percent {
    flex: 0 0 auto;
    font-size: var(--text-help-size);
    font-variant-numeric: tabular-nums;
  }
</style>
