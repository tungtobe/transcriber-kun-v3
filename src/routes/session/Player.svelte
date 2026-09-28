<script lang="ts">
  // Trình phát 64 px (spec Design Notes/DESIGN.md `player`): nút play/pause
  // tròn 40 px, thời gian `HH:MM:SS / HH:MM:SS` mono tabular, thanh seek
  // `role="slider"`, tốc độ, âm lượng. `preload="metadata"` — không bao giờ
  // tự phát (spec Always). Proxy thiếu/hỏng (`proxyPath` rỗng hoặc sự kiện
  // `error`) thay cả thanh phát bằng "Không có audio · Chọn lại file nguồn"
  // (spec I/O Matrix "Proxy thiếu"/"Proxy hỏng" — cùng một UI).
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { i18n } from '../../i18n/index.svelte';
  import { displayTimestamp } from '../../lib/time';
  import { PlayIcon, PauseIcon, Volume2Icon, FolderOpenIcon } from '../../components/icons';

  const SPEEDS = [0.75, 1, 1.25, 1.5, 2];
  const SEEK_STEP_SEC = 5;

  let {
    proxyPath,
    allowRelink = true,
    onRelinkRequest,
    relinking,
    relinkMessage,
    currentTime = $bindable(0),
    duration = $bindable(0),
    playing = $bindable(false),
  }: {
    proxyPath: string | null;
    allowRelink?: boolean;
    onRelinkRequest: () => void;
    relinking: boolean;
    relinkMessage: string | null;
    currentTime?: number;
    duration?: number;
    playing?: boolean;
  } = $props();

  let audioEl = $state<HTMLAudioElement | null>(null);
  let playbackRate = $state(1);
  let volume = $state(1);
  let audioError = $state(false);

  const src = $derived(proxyPath ? convertFileSrc(proxyPath) : null);
  const hasAudio = $derived(src !== null && !audioError);

  $effect(() => {
    // A new `proxyPath` (typically right after a successful "Chọn lại file
    // nguồn") deserves a fresh chance — don't stay stuck on a stale error.
    void proxyPath;
    audioError = false;
  });

  function handleError(): void {
    audioError = true;
    playing = false;
  }

  function handleLoadedMetadata(): void {
    if (audioEl) duration = Number.isFinite(audioEl.duration) ? audioEl.duration : 0;
  }

  function handleTimeUpdate(): void {
    if (audioEl) currentTime = audioEl.currentTime;
  }

  function handlePlay(): void {
    playing = true;
  }

  function handlePause(): void {
    playing = false;
  }

  /** Exposed to the parent via `bind:this` so `SegmentList`'s click-to-seek
   * and Space-to-toggle can drive this `<audio>` without either component
   * reaching into the other's internals. */
  export function play(): void {
    void audioEl?.play();
  }

  export function pause(): void {
    audioEl?.pause();
  }

  export function toggle(): void {
    if (!audioEl || !hasAudio) return;
    if (audioEl.paused) void audioEl.play();
    else audioEl.pause();
  }

  export function seek(sec: number): void {
    const clamped = Math.min(Math.max(sec, 0), duration > 0 ? duration : Math.max(sec, 0));
    currentTime = clamped;
    if (audioEl) audioEl.currentTime = clamped;
  }

  function handleSliderInput(event: Event): void {
    seek(Number((event.target as HTMLInputElement).value));
  }

  function handleSliderKeydown(event: KeyboardEvent): void {
    if (event.key === 'ArrowRight') {
      event.preventDefault();
      seek(currentTime + SEEK_STEP_SEC);
    } else if (event.key === 'ArrowLeft') {
      event.preventDefault();
      seek(Math.max(0, currentTime - SEEK_STEP_SEC));
    }
  }

  function handleSpeedChange(event: Event): void {
    const value = Number((event.target as HTMLSelectElement).value);
    playbackRate = value;
    if (audioEl) audioEl.playbackRate = value;
  }

  function handleVolumeInput(event: Event): void {
    const value = Number((event.target as HTMLInputElement).value);
    volume = value;
    if (audioEl) audioEl.volume = value;
  }
</script>

<div class="player">
  {#if src}
    <audio
      bind:this={audioEl}
      preload="metadata"
      {src}
      onerror={handleError}
      onloadedmetadata={handleLoadedMetadata}
      ontimeupdate={handleTimeUpdate}
      onplay={handlePlay}
      onpause={handlePause}
    ></audio>
  {/if}

  {#if !hasAudio}
    <div class="player-broken">
      <span>{i18n.t(allowRelink ? 'session.player.noAudio' : 'session.player.noAudioLive')}</span>
      {#if allowRelink}
        <button type="button" disabled={relinking} onclick={onRelinkRequest}>
          <FolderOpenIcon size={16} strokeWidth={1.75} />
          {relinking ? i18n.t('session.player.relinking') : i18n.t('session.player.relink')}
        </button>
        {#if relinkMessage}
          <span class="player-relink-message" role="alert">{relinkMessage}</span>
        {/if}
      {/if}
    </div>
  {:else}
    <button
      type="button"
      class="player-toggle"
      aria-label={playing ? i18n.t('session.player.pause') : i18n.t('session.player.play')}
      onclick={toggle}
    >
      {#if playing}
        <PauseIcon size={18} strokeWidth={1.75} />
      {:else}
        <PlayIcon size={18} strokeWidth={1.75} />
      {/if}
    </button>

    <span class="player-time">{displayTimestamp(currentTime)} / {displayTimestamp(duration)}</span>

    <input
      class="player-slider"
      type="range"
      min="0"
      max={Math.max(duration, 0)}
      step="0.01"
      value={currentTime}
      aria-valuemin={0}
      aria-valuemax={Math.max(duration, 0)}
      aria-valuenow={currentTime}
      aria-valuetext={displayTimestamp(currentTime)}
      aria-label={i18n.t('session.player.seekLabel')}
      oninput={handleSliderInput}
      onkeydown={handleSliderKeydown}
    />

    <label class="player-speed">
      <span class="visually-hidden-label">{i18n.t('session.player.speedLabel')}</span>
      <select value={playbackRate} aria-label={i18n.t('session.player.speedLabel')} onchange={handleSpeedChange}>
        {#each SPEEDS as speed (speed)}
          <option value={speed}>{speed}×</option>
        {/each}
      </select>
    </label>

    <label class="player-volume">
      <Volume2Icon size={16} strokeWidth={1.75} aria-hidden="true" />
      <span class="visually-hidden-label">{i18n.t('session.player.volumeLabel')}</span>
      <input
        type="range"
        min="0"
        max="1"
        step="0.05"
        value={volume}
        aria-label={i18n.t('session.player.volumeLabel')}
        oninput={handleVolumeInput}
      />
    </label>
  {/if}
</div>

<style>
  .player {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    height: 64px;
    padding: 0 var(--space-4);
    border-top: 1px solid var(--color-border);
    background: var(--color-surface);
  }

  .player-toggle {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    border: none;
    border-radius: var(--radius-full);
    background: var(--color-primary-action);
    color: var(--color-on-primary);
    cursor: pointer;
  }

  .player-time {
    flex: 0 0 auto;
    color: var(--color-text-secondary);
    font-family: var(--font-mono);
    font-size: var(--text-label-size);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .player-slider {
    flex: 1;
    min-width: 0;
    accent-color: var(--color-accent);
  }

  .player-speed select {
    height: 32px;
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: var(--text-label-size);
  }

  .player-volume {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--color-text-secondary);
  }

  .player-volume input {
    width: 80px;
    accent-color: var(--color-accent);
  }

  .visually-hidden-label {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }

  .player-broken {
    display: flex;
    flex: 1;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
    color: var(--color-text-secondary);
    font-size: var(--text-body-size);
  }

  .player-broken button {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    min-height: 32px;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: var(--text-label-size);
    font-weight: 500;
    cursor: pointer;
  }

  .player-broken button:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }

  .player-relink-message {
    color: var(--color-danger);
    font-size: var(--text-help-size);
    font-weight: 600;
  }
</style>
