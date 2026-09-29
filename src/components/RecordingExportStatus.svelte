<script lang="ts">
  import { i18n } from '../i18n/index.svelte';
  import { recordingExportStore } from '../lib/stores/recordingExport.svelte';

  const current = $derived(recordingExportStore.active);
  const choice = $derived(recordingExportStore.formatDialog);
  let wavInput = $state<HTMLInputElement | null>(null);

  $effect(() => {
    if (choice) wavInput?.focus();
  });
  const percent = $derived.by(() => {
    const progress = current?.progress;
    if (!progress) return 0;
    const totalSeconds = progress.totalSeconds ?? 0;
    const processedSeconds = progress.processedSeconds ?? 0;
    if (totalSeconds <= 0) return 100;
    return Math.min(100, Math.round((processedSeconds / totalSeconds) * 100));
  });

  function seconds(value: number): string {
    return Math.max(0, value).toFixed(1);
  }

  function handleFormatKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      recordingExportStore.cancelChoice();
    }
  }
</script>

{#if choice}
  <div class="recording-export-backdrop">
    <dialog
      open
      class="recording-export-dialog"
      aria-modal="true"
      aria-labelledby="recording-export-title"
      onkeydown={handleFormatKeydown}
    >
      <h2 id="recording-export-title">{i18n.t('recordingExport.format.title')}</h2>
      <p>{i18n.t('recordingExport.format.help')}</p>
      <fieldset>
        <legend>{i18n.t('recordingExport.format.title')}</legend>
        <label>
          <input
            type="radio"
            name="recording-export-format"
            value="wav"
            bind:this={wavInput}
            checked={choice.format === 'wav'}
            onchange={() => recordingExportStore.chooseFormat('wav')}
          />
          <span>{i18n.t('recordingExport.format.wav')}</span>
        </label>
        <label>
          <input
            type="radio"
            name="recording-export-format"
            value="flac"
            checked={choice.format === 'flac'}
            onchange={() => recordingExportStore.chooseFormat('flac')}
          />
          <span>{i18n.t('recordingExport.format.flac')}</span>
        </label>
      </fieldset>
      <footer>
        <button type="button" class="recording-export-secondary" onclick={recordingExportStore.cancelChoice}>
          {i18n.t('recordingExport.action.close')}
        </button>
        <button
          type="button"
          onclick={() => void recordingExportStore.start(choice.sessionId, choice.format)}
        >
          {i18n.t('recordingExport.action.continue')}
        </button>
      </footer>
    </dialog>
  </div>
{/if}

{#if current}
  <section class="recording-export-status" role="status" aria-live="polite">
    {#if current.progress}
      {@const processedSeconds = current.progress.processedSeconds ?? 0}
      {@const totalSeconds = current.progress.totalSeconds ?? 0}
      <p>
        {i18n.t('recordingExport.status.progress', {
          percent,
          processed: seconds(processedSeconds),
          total: seconds(totalSeconds),
        })}
      </p>
      <div
        class="recording-export-progress"
        role="progressbar"
        aria-label={i18n.t('sessionMenu.item.downloadRecording')}
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={percent}
      >
        <span style={`width: ${percent}%`}></span>
      </div>
      <button type="button" disabled={current.cancelling} onclick={() => void recordingExportStore.cancel()}>
        {current.cancelling ? i18n.t('recordingExport.action.cancelling') : i18n.t('recordingExport.action.cancel')}
      </button>
    {:else}
      <p>{i18n.t('recordingExport.status.preparing')}</p>
    {/if}
  </section>
{/if}

<style>
  .recording-export-backdrop {
    position: fixed;
    inset: 0;
    z-index: 1001;
    display: grid;
    place-items: center;
    padding: var(--space-4);
    background: var(--color-overlay);
  }

  .recording-export-dialog {
    position: relative;
    display: grid;
    width: min(420px, 100%);
    margin: 0;
    gap: var(--space-3);
    padding: var(--space-5);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    box-shadow: 0 10px 32px rgb(17 24 39 / 12%);
    color: var(--color-text);
  }

  .recording-export-dialog h2,
  .recording-export-dialog p {
    margin: 0;
  }

  .recording-export-dialog h2 {
    font-size: var(--text-h2-size);
  }

  .recording-export-dialog p,
  .recording-export-dialog legend {
    color: var(--color-text-secondary);
    font-size: var(--text-body-size);
  }

  .recording-export-dialog fieldset {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }

  .recording-export-dialog label {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 32px;
    cursor: pointer;
  }

  .recording-export-dialog footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }

  .recording-export-dialog footer button {
    min-height: 36px;
    padding: 0 var(--space-4);
    border: 1px solid var(--color-accent);
    border-radius: var(--radius-md);
    background: var(--color-accent);
    color: var(--color-on-primary);
    font: inherit;
    cursor: pointer;
  }

  .recording-export-dialog footer button.recording-export-secondary {
    border-color: var(--color-border);
    background: var(--color-surface);
    color: var(--color-text);
  }

  .recording-export-status {
    position: fixed;
    right: var(--space-4);
    bottom: calc(var(--space-4) + 52px);
    z-index: 999;
    display: grid;
    width: min(360px, calc(100vw - var(--space-8)));
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    box-shadow: 0 10px 32px rgb(17 24 39 / 12%);
  }

  .recording-export-status p {
    margin: 0;
    color: var(--color-text);
    font-size: var(--text-help-size);
  }

  .recording-export-progress {
    height: 8px;
    overflow: hidden;
    border-radius: var(--radius-full);
    background: var(--color-surface-sunken);
  }

  .recording-export-progress span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--color-accent);
    transition: width 120ms linear;
  }

  .recording-export-status button {
    justify-self: end;
    min-height: 32px;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-help-size);
    cursor: pointer;
  }

  .recording-export-status button:disabled {
    cursor: wait;
  }
</style>
