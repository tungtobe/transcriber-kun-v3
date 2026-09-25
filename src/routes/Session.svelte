<script lang="ts">
  import { onDestroy } from 'svelte';
  import { link } from '@keenmate/svelte-spa-router';
  import { i18n } from '../i18n/index.svelte';
  import { commands, type RerunScope, type SessionDetail, type SessionLookup } from '../lib/bindings';
  import { registerKeymap } from '../lib/keymap';
  import { formatTranscriptCopyText } from '../lib/transcript-export';
  import { cycleTranscriptMatch, findTranscriptMatches } from '../lib/transcript-search';
  import { jobsStore } from '../lib/stores/jobs.svelte';
  import { settingsStore } from '../lib/stores/settings.svelte';
  import SessionHeader from './session/SessionHeader.svelte';
  import PartialBanner from './session/PartialBanner.svelte';
  import SegmentList from './session/SegmentList.svelte';
  import Player from './session/Player.svelte';
  import JobProgress from './session/JobProgress.svelte';
  import IntakeNotices from '../components/IntakeNotices.svelte';

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
  let searchQuery = $state('');
  let activeMatchIndex = $state(-1);
  let searchInput = $state<HTMLInputElement | null>(null);
  let exporting = $state(false);
  let transcriptActionError = $state<string | null>(null);
  let transcriptToast = $state<string | null>(null);
  let transcriptToastTimer: ReturnType<typeof setTimeout> | undefined;
  let previousSearchQuery: string | undefined;
  let previousTranscriptId: string | null | undefined;

  // Trình phát: state sống ở đây (không phải trong `Player.svelte`) vì
  // `SegmentList` (highlight/click-to-seek/Space toggle) cần đọc/điều khiển
  // nó — `playerRef` gọi thẳng các hàm `Player` export qua `bind:this`.
  let playerRef = $state<{ seek: (sec: number) => void; toggle: () => void } | null>(null);
  let currentTime = $state(0);
  let duration = $state(0);
  let playing = $state(false);

  const searchMatches = $derived.by(() => {
    const segments = view.kind === 'saved' ? (view.detail.transcript?.segments ?? []) : [];
    return findTranscriptMatches(segments, searchQuery);
  });

  $effect(() => {
    const query = searchQuery;
    const transcriptId = view.kind === 'saved' ? (view.detail.transcript?.id ?? null) : null;
    if (query === previousSearchQuery && transcriptId === previousTranscriptId) return;
    previousSearchQuery = query;
    previousTranscriptId = transcriptId;
    const segments = view.kind === 'saved' ? (view.detail.transcript?.segments ?? []) : [];
    activeMatchIndex = findTranscriptMatches(segments, query).length > 0 ? 0 : -1;
  });
  const searchCountLabel = $derived(i18n.t('session.search.count', {
    current: activeMatchIndex >= 0 && searchMatches.length > 0 ? activeMatchIndex + 1 : 0,
    total: searchMatches.length,
  }));

  function setSearchQuery(value: string): void {
    searchQuery = value;
  }

  function handleSearchInput(event: Event): void {
    setSearchQuery((event.currentTarget as HTMLInputElement).value);
  }

  function moveSearchMatch(direction: -1 | 1): void {
    activeMatchIndex = cycleTranscriptMatch(activeMatchIndex, searchMatches.length, direction);
  }

  function handleSearchKeydown(event: KeyboardEvent): void {
    if (event.key !== 'Enter') return;
    event.preventDefault();
    moveSearchMatch(event.shiftKey ? -1 : 1);
  }

  function focusTranscriptSearch(): void {
    if (view.kind !== 'saved' || !view.detail.transcript) return;
    searchInput?.focus();
    searchInput?.select();
  }

  $effect(() => {
    if (view.kind !== 'saved' || !view.detail.transcript) return;
    const removeMetaFind = registerKeymap({ id: 'session-transcript-find-meta', combo: 'Meta+F', handler: focusTranscriptSearch });
    const removeControlFind = registerKeymap({ id: 'session-transcript-find-control', combo: 'Control+F', handler: focusTranscriptSearch });
    return () => {
      removeMetaFind();
      removeControlFind();
    };
  });

  function showTranscriptToast(message: string): void {
    if (transcriptToastTimer) clearTimeout(transcriptToastTimer);
    transcriptToast = message;
    transcriptToastTimer = setTimeout(() => {
      transcriptToast = null;
      transcriptToastTimer = undefined;
    }, 4_000);
  }

  async function handleCopyTranscript(transcript: NonNullable<SessionDetail['transcript']>): Promise<void> {
    transcriptActionError = null;
    transcriptToast = null;
    try {
      const clipboard = navigator.clipboard;
      if (!clipboard?.writeText) throw new Error('clipboard unavailable');
      const text = formatTranscriptCopyText(
        transcript.segments,
        settingsStore.timestampOffsetSec,
        {
          chunkFailed: i18n.t('session.export.gapChunkFailed'),
          disconnected: i18n.t('session.export.gapDisconnected'),
          unknown: i18n.t('session.export.gapUnknown'),
        },
      );
      await clipboard.writeText(text);
      showTranscriptToast(i18n.t('session.export.copySuccess'));
    } catch {
      transcriptActionError = i18n.t('session.export.copyError');
    }
  }

  async function handleExportTranscript(
    sessionId: string,
    transcriptId: string,
    format: 'txt' | 'srt' | 'json',
  ): Promise<void> {
    if (exporting) return;
    exporting = true;
    transcriptActionError = null;
    transcriptToast = null;
    try {
      const result = await commands.libraryTranscriptExport(
        sessionId,
        transcriptId,
        format,
        settingsStore.timestampOffsetSec,
      );
      if (result.status !== 'ok') {
        transcriptActionError = i18n.t('session.export.exportError');
        return;
      }
      if (!result.data.saved) return;
      const message = format === 'srt' && result.data.hasGaps
        ? i18n.t('session.export.srtSuccessWithGaps')
        : i18n.t('session.export.success');
      showTranscriptToast(message);
    } catch {
      transcriptActionError = i18n.t('session.export.exportError');
    } finally {
      exporting = false;
    }
  }

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
    setSearchQuery('');
    transcriptActionError = null;
    transcriptToast = null;
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
    if (transcriptToastTimer) clearTimeout(transcriptToastTimer);
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
      if (outcome.kind === 'busy') {
        // Spec Always: a different in-flight rerun for this session -- treat
        // like an error notice, never navigate as if this request started
        // anything.
        rerunError = i18n.t('session.rerun.busy');
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

<div class="session-intake-notices">
  <IntakeNotices />
</div>

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

    {#if transcript}
      <div class="transcript-toolbar">
        <div class="transcript-search">
          <label for="transcript-search-input">{i18n.t('session.search.label')}</label>
          <input
            bind:this={searchInput}
            id="transcript-search-input"
            type="search"
            value={searchQuery}
            placeholder={i18n.t('session.search.placeholder')}
            oninput={handleSearchInput}
            onkeydown={handleSearchKeydown}
          />
          <span class="transcript-search-count" role="status" aria-live="polite">{searchCountLabel}</span>
          <button
            type="button"
            class="transcript-toolbar-button"
            disabled={searchMatches.length === 0}
            onclick={() => moveSearchMatch(-1)}
          >
            {i18n.t('session.search.previous')}
          </button>
          <button
            type="button"
            class="transcript-toolbar-button"
            disabled={searchMatches.length === 0}
            onclick={() => moveSearchMatch(1)}
          >
            {i18n.t('session.search.next')}
          </button>
        </div>
        <div class="transcript-export-actions">
          <button type="button" class="transcript-toolbar-button" disabled={exporting} onclick={() => handleExportTranscript(sessionId, transcript.id, 'txt')}>
            {i18n.t('session.export.txt')}
          </button>
          <button type="button" class="transcript-toolbar-button" disabled={exporting} onclick={() => handleExportTranscript(sessionId, transcript.id, 'srt')}>
            {i18n.t('session.export.srt')}
          </button>
          <button type="button" class="transcript-toolbar-button" disabled={exporting} onclick={() => handleExportTranscript(sessionId, transcript.id, 'json')}>
            {i18n.t('session.export.json')}
          </button>
          <button type="button" class="transcript-toolbar-button transcript-copy-button" onclick={() => handleCopyTranscript(transcript)}>
            {i18n.t('session.export.copy')}
          </button>
        </div>
        {#if transcriptActionError}
          <p class="transcript-action-error" role="alert">{transcriptActionError}</p>
        {/if}
      </div>
      {#if transcriptToast}
        <p class="transcript-toast" role="status" aria-live="polite">{transcriptToast}</p>
      {/if}
    {/if}

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
          matches={searchMatches}
          {activeMatchIndex}
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
  .session-intake-notices:not(:empty) {
    padding: var(--space-4) var(--space-6) 0;
  }

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

  .transcript-toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--color-border);
    background: var(--color-surface);
  }

  .transcript-search,
  .transcript-export-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .transcript-search label {
    color: var(--color-text-secondary);
    font-size: var(--text-label-size);
    font-weight: 600;
  }

  .transcript-search input {
    min-width: 180px;
    min-height: 34px;
    flex: 1;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
  }

  .transcript-search input:focus-visible,
  .transcript-toolbar-button:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }

  .transcript-search-count {
    min-width: 42px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: var(--text-help-size);
    font-variant-numeric: tabular-nums;
    text-align: center;
  }

  .transcript-toolbar-button {
    min-height: 34px;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-label-size);
    cursor: pointer;
  }

  .transcript-toolbar-button:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }

  .transcript-copy-button {
    border-color: var(--color-accent-border);
    background: var(--color-accent-soft);
  }

  .transcript-action-error {
    flex-basis: 100%;
    margin: 0;
    color: var(--color-danger);
    font-size: var(--text-help-size);
  }

  .transcript-toast {
    margin: 0;
    padding: var(--space-2) var(--space-4);
    background: var(--color-info-soft);
    color: var(--color-text-secondary);
    font-size: var(--text-help-size);
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
