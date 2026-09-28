<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { link, push } from '@keenmate/svelte-spa-router';
  import NotesPanel from '../components/NotesPanel.svelte';
  import TagPicker, {
    type TagPickerAction,
    type TagPickerCreateResult,
  } from '../components/TagPicker.svelte';
  import { AlertTriangleIcon, RadioIcon, RefreshCwIcon, TagIcon } from '../components/icons';
  import { i18n } from '../i18n/index.svelte';
  import { formatTimestamp } from '../lib/time';
  import { registerKeymap } from '../lib/keymap';
  import { commands, type PermissionState, type TranscribeLanguage } from '../lib/bindings';
  import { liveStore, type LiveLine } from '../lib/stores/live.svelte';
  import { keysStore } from '../lib/stores/keys.svelte';
  import { libraryStore } from '../lib/stores/library.svelte';
  import { settingsStore } from '../lib/stores/settings.svelte';

  type SourceMode = 'mixed' | 'system' | 'microphone';

  let sourceMode = $state<SourceMode>('mixed');
  let microphone = $state('');
  let language = $state<TranscribeLanguage>(settingsStore.transcribeLanguage);
  let selectedTagIds = $state<string[]>([]);
  let tagPickerOpen = $state(false);
  let tagButton = $state<HTMLButtonElement | null>(null);
  let startPending = $state(false);
  let stopPending = $state(false);
  let notesFlushError = $state(false);
  let pendingFinalSessionId = $state<string | null>(null);
  let finalNavigationAttemptId: string | null = null;
  let recordingOnlyPending = $state(false);
  let sourcePending = $state(false);
  let autoScroll = $state(true);
  let elapsedSeconds = $state(0);
  let wallClockMs = $state(Date.now());
  let transcriptEl = $state<HTMLDivElement | null>(null);
  let notesPanelRef = $state<{
    flush: () => Promise<boolean>;
    retry: () => Promise<boolean>;
  } | null>(null);
  let previousScrollTop = 0;
  let focusListener: (() => void) | null = null;

  const isRunning = $derived(liveStore.snapshot.sessionId !== null && liveStore.snapshot.recording === 'active');
  const hasSession = $derived(liveStore.snapshot.sessionId !== null);
  const hasKey = $derived(keysStore.status === 'ready' && keysStore.hasUsableKey);
  const sourceReady = $derived(liveStore.sourceStatus === 'ready' && liveStore.sources !== null);
  const microphones = $derived(liveStore.sources?.microphones ?? []);

  const sourceValue = $derived.by(() => {
    if (sourceMode === 'system') return 'system';
    if (!microphone) return '';
    const micId = microphone.replace(/^mic:/, '');
    return sourceMode === 'mixed' ? `mixed:${micId}` : microphone;
  });

  function selectedPermission(): PermissionState | null {
    if (liveStore.deniedSource === sourceValue) return 'denied';
    if (!liveStore.sources) return null;
    if (sourceMode === 'system') return liveStore.sources.systemPermission;
    if (sourceMode === 'microphone') return liveStore.sources.microphonePermission;
    if (liveStore.sources.systemPermission === 'denied') return 'denied';
    return liveStore.sources.microphonePermission;
  }

  const permissionDenied = $derived(selectedPermission() === 'denied');
  const sourceUnavailable = $derived(
    !sourceReady
    || !sourceValue
    || (sourceMode === 'system' && liveStore.sources?.systemAvailable === false)
    || ((sourceMode === 'mixed' || sourceMode === 'microphone') && microphones.length === 0),
  );
  const canStart = $derived(
    !isRunning && !startPending && hasKey && !sourceUnavailable && !permissionDenied,
  );

  onMount(() => {
    void keysStore.load();
    void libraryStore.loadTags();
    void liveStore.loadSources(true);
    void liveStore.subscribe();
    focusListener = () => {
      liveStore.clearPermissionDenial();
      void liveStore.loadSources(true);
    };
    window.addEventListener('focus', focusListener);
  });

  onDestroy(() => {
    liveStore.unsubscribe();
    if (focusListener) window.removeEventListener('focus', focusListener);
  });

  $effect(() => {
    if (sourceMode === 'system') return;
    if (microphone || microphones.length === 0) return;
    microphone = liveStore.sources?.defaultMicrophone ?? microphones[0].source;
  });

  $effect(() => {
    if (!isRunning) return;
    if (!autoScroll) return;
    if (transcriptEl && typeof transcriptEl.scrollTo === 'function') {
      transcriptEl.scrollTo({ top: transcriptEl.scrollHeight, behavior: 'smooth' });
    }
  });

  $effect(() => {
    const sessionId = liveStore.snapshot.sessionId;
    const recording = liveStore.snapshot.recording;
    const durationSec = liveStore.snapshot.durationSec ?? 0;
    if (!sessionId) {
      elapsedSeconds = 0;
      return;
    }
    if (recording !== 'active') {
      elapsedSeconds = Math.max(0, Math.floor(durationSec));
      return;
    }

    const startedAt = Date.now() - Math.max(0, durationSec) * 1000;
    const update = () => {
      elapsedSeconds = Math.max(0, Math.floor((Date.now() - startedAt) / 1000));
      wallClockMs = Date.now();
    };
    update();
    const timer = window.setInterval(update, 250);
    return () => window.clearInterval(timer);
  });

  function startOrStop(): void {
    if (isRunning) {
      void stop();
    } else if (canStart) {
      void start();
    }
  }

  $effect(() => {
    const removeMeta = registerKeymap({
      id: 'live-toggle-meta',
      combo: 'Meta+Shift+L',
      handler: startOrStop,
    });
    const removeControl = registerKeymap({
      id: 'live-toggle-control',
      combo: 'Control+Shift+L',
      handler: startOrStop,
    });
    return () => {
      removeMeta();
      removeControl();
    };
  });

  async function start(): Promise<void> {
    if (!canStart) return;
    startPending = true;
    autoScroll = true;
    const error = await liveStore.start(sourceValue, language, i18n.locale, selectedTagIds);
    startPending = false;
    if (!error) notesFlushError = false;
    if (error?.category === 'permission') {
      await liveStore.loadSources(true);
    }
  }

  async function stop(): Promise<void> {
    if (!isRunning || stopPending) return;
    stopPending = true;
    notesFlushError = false;
    try {
      const notesSaved = await (notesPanelRef?.flush() ?? Promise.resolve(true));
      if (!notesSaved) {
        notesFlushError = true;
        return;
      }

      const result = await liveStore.stop();
      if (result.error || !result.sessionId) return;
      finalNavigationAttemptId = result.sessionId;
      pendingFinalSessionId = null;
      await push(`/session/${result.sessionId}`);
    } finally {
      stopPending = false;
    }
  }

  async function finishAutomaticStop(sessionId: string): Promise<void> {
    if (finalNavigationAttemptId === sessionId || stopPending) return;
    finalNavigationAttemptId = sessionId;
    stopPending = true;
    notesFlushError = false;
    try {
      const notesSaved = await (notesPanelRef?.flush() ?? Promise.resolve(true));
      if (!notesSaved) {
        pendingFinalSessionId = sessionId;
        notesFlushError = true;
        return;
      }
      await push(`/session/${sessionId}`);
    } finally {
      stopPending = false;
    }
  }

  async function retryNotesAndContinue(): Promise<void> {
    if (stopPending) return;
    stopPending = true;
    try {
      const saved = await (notesPanelRef?.retry() ?? Promise.resolve(true));
      if (!saved) return;
      notesFlushError = false;
      if (pendingFinalSessionId) {
        const sessionId = pendingFinalSessionId;
        pendingFinalSessionId = null;
        await push(`/session/${sessionId}`);
      } else {
        if (!isRunning) return;
        const result = await liveStore.stop();
        if (result.error || !result.sessionId) return;
        finalNavigationAttemptId = result.sessionId;
        await push(`/session/${result.sessionId}`);
      }
    } finally {
      stopPending = false;
    }
  }

  $effect(() => {
    const sessionId = liveStore.finalizedSessionId;
    if (!sessionId || stopPending || finalNavigationAttemptId === sessionId) return;
    void finishAutomaticStop(sessionId);
  });

  async function continueRecordingOnly(): Promise<void> {
    if (recordingOnlyPending) return;
    recordingOnlyPending = true;
    await liveStore.continueRecordingOnly();
    recordingOnlyPending = false;
  }

  async function changeSource(event: Event): Promise<void> {
    const previous = sourceMode;
    const value = (event.currentTarget as HTMLSelectElement).value as SourceMode;
    sourceMode = value;
    if (!isRunning) return;
    sourcePending = true;
    const error = await liveStore.setSource(sourceValue);
    sourcePending = false;
    if (error) {
      sourceMode = previous;
      if (error.category === 'permission') await liveStore.loadSources(true);
    }
  }

  async function changeMicrophone(event: Event): Promise<void> {
    const previous = microphone;
    microphone = (event.currentTarget as HTMLSelectElement).value;
    if (!isRunning) return;
    sourcePending = true;
    const error = await liveStore.setSource(sourceValue);
    sourcePending = false;
    if (error) {
      microphone = previous;
      if (error.category === 'permission') await liveStore.loadSources(true);
    }
  }

  function toggleTag(tagId: string): Promise<TagPickerAction> {
    selectedTagIds = selectedTagIds.includes(tagId)
      ? selectedTagIds.filter((id) => id !== tagId)
      : [...selectedTagIds, tagId];
    return Promise.resolve({ status: 'ok' });
  }

  async function createTag(name: string): Promise<TagPickerCreateResult> {
    const result = await libraryStore.createTag(name);
    return result.status === 'ok'
      ? { status: 'ok', id: result.tag.id, name: result.tag.name }
      : { status: 'error', message: i18n.t('live.tag.createError') };
  }

  async function deleteTag(tagId: string): Promise<TagPickerAction> {
    const result = await libraryStore.deleteTagGlobally(tagId);
    if (result.status === 'ok') {
      selectedTagIds = selectedTagIds.filter((id) => id !== tagId);
      return { status: 'ok' };
    }
    return { status: 'error', message: i18n.t('live.tag.deleteError') };
  }

  async function openPermissionSettings(): Promise<void> {
    try {
      const systemDenied = liveStore.sources?.systemPermission === 'denied';
      const failedSource = liveStore.deniedSource ?? sourceValue;
      const opensSystemSettings = failedSource === 'system' || (failedSource.startsWith('mixed:') && systemDenied);
      await commands.liveOpenPermissionSettings(opensSystemSettings);
    } catch {
      // Keep the setup usable if the OS settings handoff is unavailable.
    }
  }

  async function refreshSources(): Promise<void> {
    liveStore.clearPermissionDenial();
    await liveStore.loadSources(true);
  }

  async function recheckPermissions(): Promise<void> {
    liveStore.clearPermissionDenial();
    await liveStore.loadSources(true);
  }

  function handleTranscriptScroll(): void {
    const nextTop = transcriptEl?.scrollTop ?? 0;
    if (nextTop < previousScrollTop) autoScroll = false;
    previousScrollTop = nextTop;
  }

  function scrollToLatest(): void {
    autoScroll = true;
    if (transcriptEl && typeof transcriptEl.scrollTo === 'function') {
      transcriptEl.scrollTo({ top: transcriptEl.scrollHeight, behavior: 'smooth' });
    }
    previousScrollTop = transcriptEl?.scrollTop ?? previousScrollTop;
  }

  function lineTime(line: LiveLine): string {
    const sec = line.kind === 'segment' ? line.segment.startSec : line.startSec;
    return sec === null ? '—' : formatTimestamp(sec);
  }

  function gapRange(line: Extract<LiveLine, { kind: 'gap' }>): string {
    const start = line.startSec === null ? '—' : formatTimestamp(line.startSec);
    const end = line.endSec === null ? '—' : formatTimestamp(line.endSec);
    return i18n.t('live.transcript.gapRange', { start, end });
  }

  function formatElapsed(totalSeconds: number): string {
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    const seconds = totalSeconds % 60;
    return hours > 0
      ? `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`
      : `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
  }

  function formatReconnectElapsed(totalSeconds: number): string {
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
  }
</script>

<svelte:head>
  <title>{i18n.t('live.header.title')} · trans-kun</title>
</svelte:head>

<section class="live-screen" aria-labelledby="live-title" aria-busy={stopPending} data-ad-slot-hidden="true">
  <div class="live-heading">
    <div>
      <p class="route-kicker">{i18n.t('live.header.kicker')}</p>
      <h1 id="live-title">{i18n.t('live.header.title')}</h1>
    </div>
    {#if hasSession}
      <div class="status-pills" aria-label={i18n.t('live.status.label')}>
        <span class:status-active={liveStore.snapshot.recording === 'active'} class:status-recording={liveStore.snapshot.recording === 'active'} class="status-pill recording-pill">
          <span class="status-dot" aria-hidden="true"></span>
          <span>{i18n.t(`live.recording.${liveStore.snapshot.recording}`)}</span>
          {#if isRunning}
            <time class="mono recording-timer" aria-label={`${i18n.t('live.recording.active')} ${formatElapsed(elapsedSeconds)}`}>{formatElapsed(elapsedSeconds)}</time>
          {/if}
        </span>
        <span class="status-pill connection-pill">
          <RadioIcon size={14} strokeWidth={1.75} aria-hidden="true" />
          {#if liveStore.snapshot.transcription === 'recordingOnly' || liveStore.snapshot.transcription === 'setupRejected'}
            {i18n.t('live.connection.transcriptStopped')}
          {:else if liveStore.snapshot.connection.type === 'reconnecting'}
            {i18n.t('live.connection.reconnectingElapsed', {
              elapsed: formatReconnectElapsed(Math.floor(Math.max(0, wallClockMs - liveStore.snapshot.connection.sinceMs) / 1000)),
            })}
          {:else}
            {i18n.t(`live.connection.${liveStore.snapshot.connection.type}`)}
          {/if}
        </span>
      </div>
    {/if}
  </div>

  {#if liveStore.snapshot.connection.type === 'reconnecting'}
    <div class="live-status-banner reconnect-banner" role="status">
      <RadioIcon size={18} strokeWidth={1.75} aria-hidden="true" />
      <p>{i18n.t('live.connection.reconnectingStatus', {
        elapsed: formatReconnectElapsed(Math.floor(Math.max(0, wallClockMs - liveStore.snapshot.connection.sinceMs) / 1000)),
      })}</p>
    </div>
  {/if}

  {#if liveStore.snapshot.transcription === 'setupRejected'}
    <div class="live-error setup-rejected-banner" role="alert">
      <AlertTriangleIcon size={18} strokeWidth={1.75} aria-hidden="true" />
      <div>
        <strong>{i18n.t('live.setupRejected.title')}</strong>
        <p>{i18n.t('live.setupRejected.description')}</p>
      </div>
      <div class="permission-actions">
        <button class="button button-secondary" type="button" disabled={recordingOnlyPending || stopPending} onclick={() => void continueRecordingOnly()}>
          {i18n.t('live.setupRejected.continueRecordingOnly')}
        </button>
        <button class="button button-danger-soft" type="button" disabled={recordingOnlyPending || stopPending} onclick={() => void stop()}>
          {i18n.t('live.setupRejected.stop')}
        </button>
        <a class="settings-link" href="/settings/gemini" use:link>{i18n.t('live.category.openSettings')}</a>
      </div>
    </div>
  {:else if liveStore.snapshot.transcription === 'recordingOnly'}
    <div class="live-status-banner recording-only-banner" role="status">
      <RadioIcon size={18} strokeWidth={1.75} aria-hidden="true" />
      <div>
        <strong>{i18n.t('live.recordingOnly.title')}</strong>
        <p>{i18n.t('live.recordingOnly.description')}</p>
      </div>
    </div>
  {/if}

  {#if notesFlushError}
    <div class="live-error notes-flush-error" role="alert">
      <AlertTriangleIcon size={18} strokeWidth={1.75} aria-hidden="true" />
      <div><p>{i18n.t('live.notes.saveError')}</p></div>
      <button class="button button-secondary" type="button" disabled={stopPending} onclick={() => void retryNotesAndContinue()}>
        {i18n.t('live.notes.retryAndContinue')}
      </button>
    </div>
  {/if}

  {#if liveStore.error && liveStore.snapshot.transcription !== 'setupRejected'}
    <div class="live-error" role="status">
      <AlertTriangleIcon size={18} strokeWidth={1.75} aria-hidden="true" />
      <div>
        <strong>{i18n.t('live.error.title')}</strong>
        <p>{i18n.t(`live.category.${liveStore.error}`)}</p>
      </div>
      {#if liveStore.error === 'permission'}
        <div class="permission-actions">
          <button type="button" class="text-button" onclick={() => void openPermissionSettings()}>
            {i18n.t('live.permission.openSettings')}
          </button>
          <button type="button" class="text-button" onclick={() => void recheckPermissions()}>
            {i18n.t('live.permission.recheck')}
          </button>
        </div>
      {:else if liveStore.error === 'model' || liveStore.error === 'quota' || liveStore.error === 'auth'}
        <a class="settings-link" href="/settings/gemini" use:link>{i18n.t('live.category.openSettings')}</a>
      {/if}
    </div>
  {/if}

  {#if !hasSession}
    <div class="setup-card">
      <div class="setup-intro">
        <span class="setup-icon" aria-hidden="true"><RadioIcon size={20} strokeWidth={1.75} /></span>
        <div>
          <h2>{i18n.t('live.config.title')}</h2>
          <p>{i18n.t('live.config.description')}</p>
        </div>
      </div>

      <fieldset class="source-fieldset">
        <legend>{i18n.t('live.config.source')}</legend>
        <p class="setup-help">{i18n.t('live.config.permissionNotice')}</p>
        <div class="source-options">
          <label class:source-card-selected={sourceMode === 'mixed'} class="source-card">
            <input type="radio" name="sourceMode" value="mixed" bind:group={sourceMode} onchange={changeSource} />
            <span class="source-card-content">
              <span class="source-card-title">{i18n.t('live.source.mixed')}</span>
              <span class="source-card-description">{i18n.t('live.source.mixedDescription')}</span>
            </span>
            <span class="source-card-badge">{i18n.t('live.source.recommended')}</span>
          </label>
          <label class:source-card-selected={sourceMode === 'system'} class="source-card">
            <input type="radio" name="sourceMode" value="system" bind:group={sourceMode} onchange={changeSource} />
            <span class="source-card-content">
              <span class="source-card-title">{i18n.t('live.source.system')}</span>
              <span class="source-card-description">{i18n.t('live.source.systemDescription')}</span>
            </span>
          </label>
          <label class:source-card-selected={sourceMode === 'microphone'} class="source-card">
            <input type="radio" name="sourceMode" value="microphone" bind:group={sourceMode} onchange={changeSource} />
            <span class="source-card-content">
              <span class="source-card-title">{i18n.t('live.source.microphone')}</span>
              <span class="source-card-description">{i18n.t('live.source.microphoneDescription')}</span>
            </span>
          </label>
        </div>
      </fieldset>

      <div class="setup-grid">
        {#if sourceMode !== 'system'}
          <label class="field">
            <span>{i18n.t('live.config.microphone')}</span>
            <div class="select-with-action">
              <select value={microphone} onchange={changeMicrophone} disabled={microphones.length === 0}>
                {#each microphones as mic (mic.source)}
                  <option value={mic.source}>{mic.name}{mic.isDefault ? ` · ${i18n.t('live.source.default')}` : ''}</option>
                {/each}
              </select>
              <button type="button" class="icon-button" aria-label={i18n.t('live.config.refresh')} title={i18n.t('live.config.refresh')} onclick={() => void refreshSources()}>
                <RefreshCwIcon size={16} strokeWidth={1.75} aria-hidden="true" />
              </button>
            </div>
          </label>
        {/if}

        <label class="field">
          <span>{i18n.t('live.config.language')}</span>
          <select bind:value={language}>
            <option value="auto">{i18n.t('live.language.auto')}</option>
            <option value="ja">{i18n.t('live.language.ja')}</option>
            <option value="vi">{i18n.t('live.language.vi')}</option>
            <option value="en">{i18n.t('live.language.en')}</option>
          </select>
        </label>

        <div class="field tag-field">
          <span>{i18n.t('live.config.tags')}</span>
          <div class="tag-picker-anchor">
            <button bind:this={tagButton} type="button" class="tag-trigger" aria-expanded={tagPickerOpen} onclick={() => (tagPickerOpen = !tagPickerOpen)}>
              <TagIcon size={16} strokeWidth={1.75} aria-hidden="true" />
              {selectedTagIds.length > 0
                ? i18n.t('live.tag.selected', { count: selectedTagIds.length })
                : i18n.t('live.tag.choose')}
            </button>
            {#if tagPickerOpen}
              <TagPicker
                mode="assign"
                anchor={tagButton}
                dialogLabel={i18n.t('live.tag.dialog')}
                tags={libraryStore.tags}
                selectedIds={selectedTagIds}
                onToggle={toggleTag}
                onCreate={createTag}
                onDeleteTag={deleteTag}
                onClose={() => (tagPickerOpen = false)}
              />
            {/if}
          </div>
        </div>
      </div>

      {#if liveStore.sourceStatus === 'loading'}
        <p class="setup-help" role="status">{i18n.t('live.config.loadingSources')}</p>
      {:else if liveStore.sourceStatus === 'error'}
        <p class="setup-help error-text" role="status">{i18n.t('live.config.sourcesError')}</p>
      {:else if permissionDenied}
        <div class="permission-callout" role="status">
          <p>{i18n.t('live.permission.denied')}</p>
          <div class="permission-actions">
            <button type="button" class="text-button" onclick={() => void openPermissionSettings()}>
              {i18n.t('live.permission.openSettings')}
            </button>
            <button type="button" class="text-button" onclick={() => void recheckPermissions()}>
              {i18n.t('live.permission.recheck')}
            </button>
          </div>
        </div>
      {:else if sourceUnavailable}
        <p class="setup-help">{i18n.t('live.config.sourceUnavailable')}</p>
      {/if}

      <div class="setup-actions">
        <a class="settings-link" href="/settings/gemini" use:link>{i18n.t('live.config.keySettings')}</a>
        {#if canStart}
          <button class="button button-primary button-lg" type="button" disabled={sourcePending} onclick={() => void start()}>
            {i18n.t('live.config.start')}
          </button>
        {:else}
          <button class="button button-primary button-lg" type="button" disabled aria-disabled="true" title={!hasKey ? i18n.t('live.config.missingKey') : permissionDenied ? i18n.t('live.permission.denied') : i18n.t('live.config.sourceUnavailable')}>
            {startPending ? i18n.t('live.config.starting') : i18n.t('live.config.start')}
          </button>
        {/if}
      </div>
      {#if !hasKey}
        <p class="key-hint" role="status">{i18n.t('live.config.missingKey')} <a href="/settings/gemini" use:link>{i18n.t('live.config.keySettings')}</a></p>
      {/if}
    </div>
  {:else}
    <div class="live-layout">
      <section class="transcript-card" aria-label={i18n.t('live.transcript.label')}>
        <header class="transcript-header">
          <h2>{i18n.t('live.transcript.title')}</h2>
          <div class="transcript-controls">
            <label class="compact-source">
              <span class="sr-only">{i18n.t('live.config.source')}</span>
              <select value={sourceMode} onchange={changeSource} disabled={sourcePending}>
                <option value="mixed">{i18n.t('live.source.mixed')}</option>
                <option value="system">{i18n.t('live.source.system')}</option>
                <option value="microphone">{i18n.t('live.source.microphone')}</option>
              </select>
            </label>
            {#if sourceMode !== 'system'}
              <label class="compact-source">
                <span class="sr-only">{i18n.t('live.config.microphone')}</span>
                <select value={microphone} onchange={changeMicrophone} disabled={sourcePending || microphones.length === 0}>
                  {#each microphones as mic (mic.source)}
                    <option value={mic.source}>{mic.name}</option>
                  {/each}
                </select>
              </label>
            {/if}
            {#if isRunning}
              <button class="button button-danger-soft" type="button" disabled={stopPending} onclick={() => void stop()}>
                {stopPending ? i18n.t('live.recording.stopping') : i18n.t('live.recording.stop')}
              </button>
            {:else}
              <button class="button button-secondary" type="button" onclick={() => liveStore.reset()}>
                {i18n.t('live.config.newSession')}
              </button>
            {/if}
          </div>
        </header>

        <div
          class="transcript-list"
          role="region"
          aria-label={i18n.t('live.transcript.label')}
          bind:this={transcriptEl}
          onscroll={handleTranscriptScroll}
          aria-live="polite"
          aria-relevant="additions text"
        >
          {#each liveStore.lines as line (line.seq)}
            <article class:gap-line={line.kind === 'gap'} class="transcript-line">
              {#if line.kind === 'segment'}
                <time class="mono">{lineTime(line)}</time>
                <p>{line.segment.text}</p>
              {:else}
                <p>{gapRange(line)}</p>
              {/if}
            </article>
          {/each}
          {#if liveStore.draft}
            <article class="transcript-line interim-line">
              <time class="mono">{i18n.t('live.transcript.now')}</time>
              <p>{liveStore.draft}<span class="live-caret" aria-hidden="true">▍</span></p>
            </article>
          {:else if liveStore.snapshot.transcription !== 'active' && liveStore.lines.length === 0}
            <p class="listening-empty">{i18n.t('live.transcript.stopped')}</p>
          {:else if isRunning && liveStore.lines.length === 0}
            <p class="listening-empty">{i18n.t('live.transcript.listening')}<span class="live-caret" aria-hidden="true">▍</span></p>
          {/if}
          {#if !isRunning && liveStore.lines.length === 0 && !liveStore.draft}
            <p class="listening-empty">{i18n.t('live.transcript.empty')}</p>
          {/if}
        </div>
        {#if !autoScroll && isRunning}
          <button type="button" class="scroll-latest" onclick={scrollToLatest}>{i18n.t('live.transcript.scrollLatest')}</button>
        {/if}
      </section>

      {#if liveStore.snapshot.sessionId}
        <details class="notes-card" open>
          <summary>{i18n.t('live.notes.title')}</summary>
          <div class="notes-content"><NotesPanel bind:this={notesPanelRef} sessionId={liveStore.snapshot.sessionId} /></div>
        </details>
      {/if}
    </div>
  {/if}

  {#if stopPending}
    <div class="save-overlay" role="status" aria-live="polite">
      <div class="save-overlay-card">
        <span class="save-spinner"><RefreshCwIcon size={20} strokeWidth={1.75} aria-hidden="true" /></span>
        <span>{i18n.t('live.overlay.saving')}<small>{i18n.t('live.overlay.steps')}</small></span>
      </div>
    </div>
  {/if}
</section>

<style>
  .live-screen { max-width: 1120px; margin: 0 auto; padding: var(--space-8); }
  .save-overlay { position:fixed; inset:0; z-index:1000; display:grid; place-items:center; padding:var(--space-4); background:rgb(15 23 42 / 38%); }
  .save-overlay-card { display:flex; align-items:center; gap:var(--space-3); padding:var(--space-4) var(--space-5); border:1px solid var(--color-border); border-radius:var(--radius-lg); background:var(--color-surface); color:var(--color-text); box-shadow:0 10px 32px rgb(17 24 39 / 12%); font-weight:600; }
  .save-spinner { display:inline-flex; color:var(--color-accent); animation:save-spin 1s linear infinite; }
  .save-overlay-card small { display:block; margin-top:var(--space-1); color:var(--color-text-muted); font-size:var(--text-help-size); font-weight:400; }
  .notes-flush-error { align-items:center; }
  .notes-flush-error p { margin:0; }
  .live-heading { display:flex; align-items:flex-start; justify-content:space-between; gap:var(--space-4); margin-bottom:var(--space-6); }
  .route-kicker { margin:0 0 var(--space-1); color:var(--color-text-muted); font-size:var(--text-help-size); font-weight:600; letter-spacing:.04em; text-transform:uppercase; }
  h1, h2, p { margin-top:0; }
  h1 { margin-bottom:0; font-size:var(--text-screen-title-size); }
  h2 { margin-bottom:var(--space-2); font-size:var(--text-section-title-size); }
  .setup-card, .transcript-card, .notes-card { border:1px solid var(--color-border); border-radius:var(--radius-xl); background:var(--color-surface); box-shadow:0 1px 2px rgb(17 24 39 / 8%); }
  .setup-card { max-width:680px; margin:0 auto; padding:var(--space-6); }
  .setup-intro { display:flex; align-items:flex-start; gap:var(--space-3); margin-bottom:var(--space-6); }
  .setup-intro p { margin-bottom:0; color:var(--color-text-secondary); }
  .setup-icon { display:grid; width:40px; height:40px; flex:none; place-items:center; border-radius:var(--radius-lg); background:var(--color-accent-soft); color:var(--color-accent); }
  .source-fieldset { min-width:0; margin:0 0 var(--space-5); padding:0; border:0; }
  .source-fieldset legend { margin-bottom:var(--space-2); color:var(--color-text-secondary); font-size:var(--text-label-size); font-weight:600; }
  .source-options { display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:var(--space-2); }
  .source-card { position:relative; display:flex; min-width:0; min-height:112px; flex-direction:column; justify-content:space-between; gap:var(--space-2); padding:var(--space-3); border:1px solid var(--color-border-strong); border-radius:var(--radius-lg); background:var(--color-bg); color:var(--color-text); cursor:pointer; }
  .source-card-selected { border-color:var(--color-accent); background:var(--color-accent-soft); }
  .source-card input { position:absolute; width:1px; height:1px; overflow:hidden; opacity:0; }
  .source-card:focus-within { outline:2px solid var(--color-accent); outline-offset:2px; }
  .source-card-content { display:flex; flex-direction:column; gap:var(--space-1); }
  .source-card-title { font-weight:600; line-height:1.35; }
  .source-card-description { color:var(--color-text-muted); font-size:var(--text-help-size); font-weight:400; line-height:1.4; }
  .source-card-badge { align-self:flex-start; padding:2px var(--space-2); border-radius:var(--radius-full); background:var(--color-accent); color:var(--color-on-accent); font-size:var(--text-help-size); font-weight:600; }
  .setup-grid { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:var(--space-4); }
  .field { display:flex; min-width:0; flex-direction:column; gap:var(--space-2); color:var(--color-text-secondary); font-size:var(--text-label-size); font-weight:600; }
  .field select, .compact-source select { width:100%; min-width:0; min-height:40px; padding:0 var(--space-3); border:1px solid var(--color-border-strong); border-radius:var(--radius-md); background:var(--color-bg); color:var(--color-text); font:inherit; font-weight:400; }
  .select-with-action { display:flex; gap:var(--space-2); }
  .icon-button, .tag-trigger { display:inline-flex; min-height:40px; align-items:center; justify-content:center; gap:var(--space-2); padding:0 var(--space-3); border:1px solid var(--color-border-strong); border-radius:var(--radius-md); background:var(--color-bg); color:var(--color-text-secondary); cursor:pointer; }
  .icon-button { width:40px; flex:none; padding:0; }
  .tag-picker-anchor { position:relative; }
  .tag-trigger { width:100%; justify-content:flex-start; }
  .setup-help, .key-hint { margin:var(--space-4) 0 0; color:var(--color-text-muted); font-size:var(--text-help-size); }
  .error-text, .permission-callout { color:var(--color-danger); }
  .permission-callout { display:flex; align-items:center; justify-content:space-between; gap:var(--space-3); margin-top:var(--space-4); padding:var(--space-3); border-radius:var(--radius-md); background:var(--color-danger-soft); }
  .permission-callout p { margin:0; }
  .permission-actions { display:flex; flex-wrap:wrap; justify-content:flex-end; gap:var(--space-3); }
  .setup-actions { display:flex; align-items:center; justify-content:space-between; gap:var(--space-3); margin-top:var(--space-6); }
  .button-lg { min-height:48px; padding-inline:var(--space-5); }
  .settings-link, .text-button { color:var(--color-accent); font:inherit; text-decoration:underline; cursor:pointer; }
  .text-button { padding:0; border:0; background:none; }
  .key-hint { color:var(--color-danger); }
  .key-hint a { color:inherit; }
  .status-pills { display:flex; flex-wrap:wrap; gap:var(--space-2); }
  .status-pill { display:inline-flex; min-height:30px; align-items:center; gap:var(--space-2); padding:0 var(--space-3); border:1px solid var(--color-border); border-radius:var(--radius-full); background:var(--color-surface); color:var(--color-text-secondary); font-size:var(--text-help-size); }
  .status-dot { width:7px; height:7px; border-radius:50%; background:var(--color-text-muted); }
  .status-active { border-color:var(--color-success); color:var(--color-success); }
  .status-recording { border-color:var(--color-danger); background:var(--color-danger-soft); color:var(--color-danger); }
  .status-recording .status-dot { background:var(--color-danger); animation:recording-pulse 1.4s ease-out infinite; }
  .recording-timer { font-size:var(--text-help-size); font-variant-numeric:tabular-nums; }
  .connection-pill { color:var(--color-text-secondary); }
  .live-error { display:flex; align-items:flex-start; gap:var(--space-3); margin-bottom:var(--space-4); padding:var(--space-3) var(--space-4); border:1px solid var(--color-danger); border-radius:var(--radius-lg); color:var(--color-danger); }
  .live-error div { flex:1; }
  .live-error p { margin:var(--space-1) 0 0; }
  .live-status-banner { display:flex; align-items:flex-start; gap:var(--space-3); margin-bottom:var(--space-4); padding:var(--space-3) var(--space-4); border:1px solid var(--color-border-strong); border-radius:var(--radius-lg); color:var(--color-text-secondary); }
  .live-status-banner p { margin:0; }
  .live-status-banner strong { color:var(--color-text); }
  .reconnect-banner { border-color:var(--color-warning); background:var(--color-warning-soft); }
  .recording-only-banner { border-color:var(--color-accent); background:var(--color-accent-soft); }
  .setup-rejected-banner { align-items:center; }
  .live-layout { display:grid; grid-template-columns:minmax(0,1.7fr) minmax(260px,.8fr); align-items:stretch; gap:var(--space-4); min-height:540px; }
  .transcript-card { position:relative; display:flex; min-height:500px; flex-direction:column; overflow:hidden; }
  .transcript-header { display:flex; flex-wrap:wrap; align-items:center; justify-content:space-between; gap:var(--space-3); padding:var(--space-4) var(--space-5); border-bottom:1px solid var(--color-border); }
  .transcript-header h2 { margin:0; }
  .transcript-controls { display:flex; flex-wrap:wrap; align-items:center; gap:var(--space-2); }
  .compact-source select { min-height:34px; max-width:170px; padding-inline:var(--space-2); font-size:var(--text-help-size); }
  .transcript-list { flex:1; min-height:0; overflow:auto; padding:var(--space-4) var(--space-5); scroll-behavior:smooth; }
  .transcript-line { display:grid; grid-template-columns:64px minmax(0,1fr); gap:var(--space-3); padding:var(--space-3) 0; border-bottom:1px solid var(--color-border); }
  .transcript-line time { padding-top:3px; color:var(--color-text-muted); font-size:var(--text-help-size); }
  .transcript-line p { margin:0; white-space:pre-wrap; line-height:1.65; }
  .gap-line { display:block; margin-block:var(--space-2); padding:var(--space-3); border-radius:var(--radius-md); background:var(--color-surface-sunken); color:var(--color-text-muted); font-style:italic; }
  .interim-line { color:var(--color-text-secondary); }
  .listening-empty { margin:var(--space-6) 0; color:var(--color-text-muted); text-align:center; }
  .live-caret { margin-left:3px; color:var(--color-accent); animation:caret-blink 1s step-end infinite; }
  .scroll-latest { position:absolute; right:var(--space-5); bottom:var(--space-4); padding:var(--space-2) var(--space-3); border:1px solid var(--color-border-strong); border-radius:var(--radius-full); background:var(--color-surface); color:var(--color-text); box-shadow:0 1px 2px rgb(17 24 39 / 8%); cursor:pointer; }
  .notes-card { display:flex; min-width:0; flex-direction:column; overflow:hidden; }
  .notes-card summary { padding:var(--space-4) var(--space-5); border-bottom:1px solid var(--color-border); font-weight:600; cursor:pointer; }
  .notes-content { flex:1; min-height:0; padding:var(--space-4); }
  .notes-card:not([open]) .notes-content { display:none; }
  .notes-card:not([open]) { align-self:start; }
  .mono { font-family:var(--font-mono); }
  @keyframes caret-blink { 50% { opacity:0; } }
  @keyframes save-spin { to { transform:rotate(360deg); } }
  @keyframes recording-pulse { 50% { opacity:.3; transform:scale(1.5); } }
  @media (prefers-reduced-motion: reduce) { .live-caret, .status-recording .status-dot, .save-spinner { animation:none; } .transcript-list { scroll-behavior:auto; } }
  @media (max-width: 900px) { .live-layout { grid-template-columns:1fr; } .notes-card { min-height:280px; } }
  @media (max-width: 650px) { .live-screen { padding:var(--space-4); } .setup-grid, .source-options { grid-template-columns:1fr; } .source-card { min-height:84px; } .live-heading { flex-direction:column; } .setup-actions { align-items:flex-start; flex-direction:column; } }
</style>
