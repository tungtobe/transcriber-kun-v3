<script lang="ts">
  import { onDestroy } from 'svelte';
  import { link } from '@keenmate/svelte-spa-router';
  import { i18n } from '../i18n/index.svelte';
  import { commands, type RerunScope, type SessionDetail, type SessionLookup } from '../lib/bindings';
  import { jobsStore } from '../lib/stores/jobs.svelte';
  import SessionHeader from './session/SessionHeader.svelte';
  import PartialBanner from './session/PartialBanner.svelte';
  import SegmentList from './session/SegmentList.svelte';
  import Player from './session/Player.svelte';
  import JobProgress from './session/JobProgress.svelte';

  type RouteParams = { id?: string };
  let { routeParams = {} }: { routeParams?: RouteParams } = $props();

  type ViewState =
    | { kind: 'loading' }
    | { kind: 'job'; jobId: string; sessionId: string }
    | { kind: 'saved'; sessionId: string; detail: SessionDetail }
    | { kind: 'notFound' }
    | { kind: 'error' };

  let view = $state<ViewState>({ kind: 'loading' });
  let subscribed = false;
  let cancelling = $state(false);
  let rerunStarting = $state(false);
  let rerunError = $state<string | null>(null);
  let relinking = $state(false);
  let relinkMessage = $state<string | null>(null);

  // Trình phát: state sống ở đây (không phải trong `Player.svelte`) vì
  // `SegmentList` (highlight/click-to-seek/Space toggle) cần đọc/điều khiển
  // nó — `playerRef` gọi thẳng các hàm `Player` export qua `bind:this`.
  let playerRef = $state<{ seek: (sec: number) => void; toggle: () => void } | null>(null);
  let currentTime = $state(0);
  let duration = $state(0);
  let playing = $state(false);

  async function loadDetail(sessionId: string): Promise<void> {
    try {
      const result = await commands.librarySessionDetail(sessionId);
      if (result.status !== 'ok') {
        view = { kind: 'error' };
        return;
      }
      if (result.data === null) {
        view = { kind: 'notFound' };
        return;
      }
      view = { kind: 'saved', sessionId, detail: result.data };
    } catch {
      view = { kind: 'error' };
    }
  }

  function applyLookup(lookup: SessionLookup): void {
    if (lookup.kind === 'job') {
      view = { kind: 'job', jobId: lookup.jobId, sessionId: lookup.sessionId };
    } else if (lookup.kind === 'session') {
      // `SessionLookup::Session` chỉ đủ để phân biệt Job/Phiên/không-còn
      // (spec Design Notes: "detail tải sau khi biết là Phiên") — chi tiết
      // đầy đủ (segments, proxy, transcript) tới từ `librarySessionDetail`.
      void loadDetail(lookup.sessionId);
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

  async function handleCancel(jobId: string): Promise<void> {
    cancelling = true;
    try {
      await jobsStore.cancel(jobId);
    } finally {
      cancelling = false;
    }
  }

  // Chạy lại: dùng chung cho cả banner partial ("phần thiếu"/"toàn bộ") lẫn
  // nút "Chạy lại khoảng này" trên một dòng gap (spec Tasks Acceptance:
  // "bấm 'Chạy lại khoảng này' trên dòng gap idx 3 -> `transcribe_rerun`
  // được gọi với `{ kind: 'gap', gapId: 3 }`").
  async function handleRerun(sessionId: string, transcriptId: string | undefined, scope: RerunScope): Promise<void> {
    if (!transcriptId || rerunStarting) return;
    rerunStarting = true;
    rerunError = null;
    try {
      const outcome = await jobsStore.rerun(sessionId, transcriptId, scope);
      if ('error' in outcome) {
        rerunError = i18n.t('session.rerun.error');
        return;
      }
      if (outcome.kind === 'nothingToRerun') {
        rerunError = i18n.t('session.rerun.nothingToRerun');
        return;
      }
      view = { kind: 'job', jobId: outcome.jobId, sessionId };
    } finally {
      rerunStarting = false;
    }
  }

  async function handleRelinkRequest(): Promise<void> {
    if (view.kind !== 'saved' || relinking) return;
    const sessionId = view.sessionId;
    relinking = true;
    relinkMessage = null;
    try {
      const result = await commands.libraryProxyRelink(sessionId);
      if (result.status !== 'ok') {
        relinkMessage = i18n.t('session.player.relinkError');
        return;
      }
      switch (result.data) {
        case 'relinked':
          await loadDetail(sessionId);
          break;
        case 'hashMismatch':
          relinkMessage = i18n.t('session.player.relinkHashMismatch');
          break;
        case 'liveUnsupported':
          relinkMessage = i18n.t('session.player.relinkLiveUnsupported');
          break;
        case 'cancelled':
          // Huỷ dialog -> im lặng, không đổi gì (spec I/O Matrix "Chọn lại sai").
          break;
      }
    } catch {
      relinkMessage = i18n.t('session.player.relinkError');
    } finally {
      relinking = false;
    }
  }
</script>

<svelte:head>
  <title>{i18n.t('session.meta.title')}</title>
</svelte:head>

{#if view.kind === 'saved'}
  {@const sessionId = view.sessionId}
  {@const detail = view.detail}
  {@const transcript = detail.transcript}
  {@const gapRanges = (transcript?.segments ?? [])
    .filter((s) => s.kind === 'gap' && s.gapReason === 'chunk_failed')
    .map((s) => ({ startSec: s.startSec ?? 0, endSec: s.endSec ?? 0 }))}
  {@const segmentTextCount = (transcript?.segments ?? []).filter((s) => s.kind === 'text').length}
  <section class="session-shell" aria-labelledby="session-title">
    <SessionHeader
      title={detail.title}
      kind={detail.kind}
      createdAtMs={detail.createdAt}
      durationSec={detail.durationSec}
      segmentTextCount={segmentTextCount}
      recovered={detail.recovered}
      partial={transcript?.status === 'partial'}
    />

    <div class="session-body">
      <div class="session-main">
        {#if transcript?.status === 'partial'}
          <div class="session-partial-banner">
            <PartialBanner
              {gapRanges}
              starting={rerunStarting}
              errorMessage={rerunError}
              onRerun={(scope) => handleRerun(sessionId, transcript?.id, scope)}
            />
          </div>
        {/if}
        <SegmentList
          segments={transcript?.segments ?? []}
          {currentTime}
          {playing}
          rerunStarting={rerunStarting}
          onSeek={(sec) => playerRef?.seek(sec)}
          onTogglePlay={() => playerRef?.toggle()}
          onRerun={(scope) => handleRerun(sessionId, transcript?.id, scope)}
        />
      </div>
      <aside class="session-aside">
        <h2>{i18n.t('session.aside.title')}</h2>
        <dl>
          <dt>{i18n.t('session.aside.source')}</dt>
          <dd>{detail.sourceName ?? i18n.t('session.aside.unknown')}</dd>
          <dt>{i18n.t('session.aside.model')}</dt>
          <dd>{transcript?.model ?? i18n.t('session.aside.unknown')}</dd>
          <dt>{i18n.t('session.aside.language')}</dt>
          <dd>{transcript?.language ?? i18n.t('session.aside.languageAuto')}</dd>
        </dl>
      </aside>
    </div>

    <Player
      bind:this={playerRef}
      bind:currentTime
      bind:duration
      bind:playing
      proxyPath={detail.proxyPath}
      onRelinkRequest={handleRelinkRequest}
      {relinking}
      {relinkMessage}
    />
  </section>
{:else}
  <section class="route-screen" aria-labelledby="session-title">
    <p class="route-kicker">{i18n.t('session.header.kicker')}</p>

    {#if view.kind === 'loading'}
      <h1 id="session-title">{i18n.t('session.state.loading')}</h1>
    {:else if view.kind === 'job'}
      <h1 id="session-title">
        {job?.kind === 'rerun' ? i18n.t('session.job.kindRerun') : (job?.sourceName ?? i18n.t('session.header.titleJob'))}
      </h1>
      {#if job}
        <JobProgress {job} {cancelling} onCancel={handleCancel} />
      {/if}
    {:else}
      <h1 id="session-title">{i18n.t('session.header.titleNotFound')}</h1>
      <p class="job-detail">
        {view.kind === 'error' ? i18n.t('session.state.errorBody') : i18n.t('session.state.notFoundBody')}
      </p>
      <a class="button button-secondary" href="/home" use:link>{i18n.t('session.state.backHome')}</a>
    {/if}
  </section>
{/if}

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

  .job-detail {
    margin: 0;
    color: var(--color-text-secondary);
    font-size: var(--text-help-size);
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

  .session-shell {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .session-body {
    display: grid;
    grid-template-columns: minmax(0, 1fr) var(--panel-detail-width);
    flex: 1;
    min-height: 0;
  }

  .session-main {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .session-partial-banner {
    padding: var(--space-4) var(--space-4) 0;
  }

  .session-aside {
    overflow-y: auto;
    padding: var(--space-4);
    border-left: 1px solid var(--color-border);
    background: var(--color-surface);
  }

  .session-aside h2 {
    margin: 0 0 var(--space-3);
    font-size: var(--text-label-size);
    color: var(--color-text-secondary);
  }

  .session-aside dl {
    display: grid;
    gap: var(--space-1);
    margin: 0;
  }

  .session-aside dt {
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
  }

  .session-aside dd {
    margin: 0 0 var(--space-3);
    color: var(--color-text);
    font-size: var(--text-body-size);
  }

  @media (max-width: 1279px) {
    .session-body {
      grid-template-columns: 1fr;
    }

    .session-aside {
      display: none;
    }
  }
</style>
