<script lang="ts">
  import { onDestroy } from 'svelte';
  import { link } from '@keenmate/svelte-spa-router';
  import { i18n } from '../i18n/index.svelte';
  import { commands, type SessionLookup } from '../lib/bindings';
  import { jobsStore } from '../lib/stores/jobs.svelte';

  type RouteParams = { id?: string };
  let { routeParams = {} }: { routeParams?: RouteParams } = $props();

  type ViewState =
    | { kind: 'loading' }
    | { kind: 'job'; jobId: string; sessionId: string }
    | { kind: 'saved'; sessionId: string; title: string; durationSec: number | null; sessionStatus: string }
    | { kind: 'notFound' }
    | { kind: 'error' };

  let view = $state<ViewState>({ kind: 'loading' });
  let subscribed = false;
  let cancelling = $state(false);

  function applyLookup(lookup: SessionLookup): void {
    if (lookup.kind === 'job') {
      view = { kind: 'job', jobId: lookup.jobId, sessionId: lookup.sessionId };
    } else if (lookup.kind === 'session') {
      view = {
        kind: 'saved',
        sessionId: lookup.sessionId,
        title: lookup.title,
        durationSec: lookup.durationSec,
        sessionStatus: lookup.status,
      };
    } else {
      view = { kind: 'notFound' };
    }
  }

  async function load(id: string): Promise<void> {
    view = { kind: 'loading' };
    try {
      const result = await commands.librarySessionGet(id);
      if (result.status === 'ok') {
        applyLookup(result.data);
      } else {
        view = { kind: 'error' };
      }
    } catch {
      view = { kind: 'error' };
    }
  }

  const routeId = $derived(routeParams.id ?? null);

  $effect(() => {
    if (routeId) void load(routeId);
  });

  $effect(() => {
    if (view.kind === 'job' && !subscribed) {
      subscribed = true;
      void jobsStore.subscribe();
    }
  });

  onDestroy(() => {
    if (subscribed) jobsStore.unsubscribe();
  });

  const job = $derived(view.kind === 'job' ? (jobsStore.jobs.get(view.jobId) ?? null) : null);

  // The Job left the registry (committed, errored, or cancelled) — re-resolve
  // so the route shows the saved Phiên or "Tác vụ không còn" (spec I/O Matrix
  // "`/session/:id`"). Guarded by `synced` so the transient empty map right
  // after subscribe() never reads as "Job is gone" (spec I/O Matrix "Remount
  // UI"/store contract).
  $effect(() => {
    if (view.kind === 'job' && jobsStore.synced && job === null && routeId) {
      void load(routeId);
    }
  });

  function minutes(ms: number): number {
    return Math.floor(ms / 60_000);
  }

  function percent(processedMs: number, totalMs: number): number {
    if (totalMs <= 0) return 0;
    return Math.min(100, Math.round((processedMs / totalMs) * 100));
  }

  async function handleCancel(jobId: string): Promise<void> {
    cancelling = true;
    try {
      await jobsStore.cancel(jobId);
    } finally {
      cancelling = false;
    }
  }
</script>

<svelte:head>
  <title>{i18n.t('session.meta.title')}</title>
</svelte:head>

<section class="route-screen" aria-labelledby="session-title">
  <p class="route-kicker">{i18n.t('session.header.kicker')}</p>

  {#if view.kind === 'loading'}
    <h1 id="session-title">{i18n.t('session.state.loading')}</h1>
  {:else if view.kind === 'job'}
    <h1 id="session-title">{job?.sourceName ?? i18n.t('session.header.titleJob')}</h1>
    {#if job}
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
          onclick={() => handleCancel(job.jobId)}
        >
          {cancelling ? i18n.t('session.job.cancelling') : i18n.t('session.job.cancelAction')}
        </button>
      </div>
    {/if}
  {:else if view.kind === 'saved'}
    <h1 id="session-title">{view.title}</h1>
    <div class="job-card">
      <p class="job-detail">{i18n.t('session.saved.statusLabel', { status: view.sessionStatus })}</p>
      {#if view.durationSec !== null}
        <p class="job-detail">
          {i18n.t('session.saved.durationLabel', { minutes: Math.round(view.durationSec / 60) })}
        </p>
      {/if}
    </div>
  {:else}
    <h1 id="session-title">{i18n.t('session.header.titleNotFound')}</h1>
    <p class="job-detail">
      {view.kind === 'error' ? i18n.t('session.state.errorBody') : i18n.t('session.state.notFoundBody')}
    </p>
    <a class="button button-secondary" href="/home" use:link>{i18n.t('session.state.backHome')}</a>
  {/if}
</section>

<style>
  .route-screen {
    max-width: 720px;
    margin: 0 auto;
    padding: var(--space-8);
  }

  .route-kicker {
    margin: 0 0 var(--space-1);
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  h1 {
    margin: 0 0 var(--space-5);
    font-size: var(--text-screen-title-size);
    line-height: 1.35;
  }

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
    text-decoration: none;
    cursor: pointer;
  }

  .button:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }
</style>
