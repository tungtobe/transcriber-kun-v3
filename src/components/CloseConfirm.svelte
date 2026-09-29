<script lang="ts">
  // Window, menu, and Cmd+Q close requests arrive through one typed event.
  // An idle close flushes notes and continues without an extra prompt; Live or
  // Job activity opens one dialog, and any storage error keeps it open.
  import { appStore } from '../lib/stores/app.svelte';
  import { notesStore } from '../lib/stores/notes.svelte';
  import { i18n } from '../i18n/index.svelte';

  let dialogVisible = $state(false);
  let closing = $state(false);
  let liveBusy = $state(false);
  let jobBusy = $state(false);
  let closeError = $state<string | null>(null);
  // Set when saving notes failed: the user may still quit without them.
  let notesFlushFailed = $state(false);

  async function handleCloseRequested(request: { liveBusy: boolean; jobBusy: boolean }): Promise<void> {
    liveBusy = request.liveBusy;
    jobBusy = request.jobBusy;
    closeError = null;
    notesFlushFailed = false;
    if (liveBusy || jobBusy) {
      dialogVisible = true;
      return;
    }
    await flushAndConfirm();
  }

  async function flushAndConfirm(skipNotes = false): Promise<void> {
    closing = true;
    try {
      if (!skipNotes && !(await notesStore.flushAll())) {
        notesFlushFailed = true;
        closeError = i18n.t('closeConfirm.flushError.body');
        dialogVisible = true;
        return;
      }
      notesFlushFailed = false;
      const error = await appStore.confirmClose();
      if (error) {
        closeError = i18n.t('closeConfirm.storageError.body');
        dialogVisible = true;
      } else {
        dialogVisible = false;
      }
    } catch {
      closeError = i18n.t('closeConfirm.storageError.body');
      dialogVisible = true;
    } finally {
      closing = false;
    }
  }

  $effect(() => {
    let active = true;
    let unlisten: (() => void) | null = null;
    void appStore
      .listenForCloseRequested((busy) => {
        void handleCloseRequested(busy);
      })
      .then((fn) => {
        if (!active) {
          fn();
          return;
        }
        unlisten = fn;
      });
    return () => {
      active = false;
      unlisten?.();
    };
  });

  function stayOpen(): void {
    dialogVisible = false;
    closeError = null;
    notesFlushFailed = false;
    void appStore.stayOpen();
  }

  function dialogTitle(): string {
    if (closeError) {
      return notesFlushFailed ? i18n.t('closeConfirm.flushError.title') : i18n.t('closeConfirm.storageError.title');
    }
    if (liveBusy) return i18n.t('closeConfirm.dialog.liveTitle');
    return i18n.t('closeConfirm.dialog.title');
  }

  function dialogBody(): string {
    if (closeError) return closeError;
    if (liveBusy && jobBusy) return i18n.t('closeConfirm.dialog.liveAndJobBody');
    if (liveBusy) return i18n.t('closeConfirm.dialog.liveBody');
    return i18n.t('closeConfirm.dialog.jobBody');
  }

  function confirmLabel(): string {
    if (closeError) return i18n.t('closeConfirm.storageError.retry');
    if (liveBusy && jobBusy) return i18n.t('closeConfirm.dialog.liveAndJobConfirm');
    if (liveBusy) return i18n.t('closeConfirm.dialog.liveConfirm');
    return i18n.t('closeConfirm.dialog.confirm');
  }
</script>

{#if dialogVisible}
  <div class="close-confirm-backdrop" role="presentation">
    <div class="close-confirm" role="alertdialog" aria-modal="true" aria-labelledby="close-confirm-title">
      <h2 id="close-confirm-title">{dialogTitle()}</h2>
      <p>{dialogBody()}</p>
      <div class="close-confirm-actions">
        <button type="button" class="button button-secondary" disabled={closing} onclick={stayOpen}>
          {i18n.t('closeConfirm.dialog.stay')}
        </button>
        {#if notesFlushFailed}
          <button type="button" class="button button-secondary" disabled={closing} onclick={() => void flushAndConfirm(true)}>
            {i18n.t('closeConfirm.flushError.exit')}
          </button>
        {/if}
        <button type="button" class="button button-danger" disabled={closing} onclick={() => void flushAndConfirm()}>
          {closing ? i18n.t('closeConfirm.dialog.closing') : confirmLabel()}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .close-confirm-backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: grid;
    place-items: center;
    background: rgb(17 24 39 / 45%);
  }

  .close-confirm {
    display: grid;
    gap: var(--space-3);
    width: min(420px, calc(100vw - var(--space-8)));
    padding: var(--space-6);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
    background: var(--color-surface);
    box-shadow: 0 10px 32px rgb(17 24 39 / 12%);
  }

  h2 {
    margin: 0;
    font-size: var(--text-h2-size);
  }

  p {
    margin: 0;
    color: var(--color-text-secondary);
    font-size: var(--text-body-size);
  }

  .close-confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-3);
    margin-top: var(--space-2);
  }

  .button {
    display: inline-flex;
    min-height: 36px;
    align-items: center;
    justify-content: center;
    padding: 0 14px;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    font-size: var(--text-body-size);
    font-weight: 500;
    cursor: pointer;
  }

  .button:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }

  .button-secondary {
    border-color: var(--color-border-strong);
    background: var(--color-surface);
    color: var(--color-text);
  }

  .button-danger {
    background: var(--color-danger);
    color: var(--color-on-primary);
  }
</style>
