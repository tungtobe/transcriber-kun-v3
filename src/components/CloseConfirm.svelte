<script lang="ts">
  // Story 2.4 Design Notes: "Đóng app: luôn `prevent_close`, hỏi registry
  // async; rảnh → thoát, bận → emit `CloseRequested`". This component only
  // reacts to that one event — it never polls the job registry itself.
  import { appStore } from '../lib/stores/app.svelte';
  import { i18n } from '../i18n/index.svelte';

  let visible = $state(false);
  let closing = $state(false);

  $effect(() => {
    let active = true;
    let unlisten: (() => void) | null = null;
    void appStore
      .listenForCloseRequested(() => {
        visible = true;
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

  function stay(): void {
    visible = false;
  }

  async function confirmClose(): Promise<void> {
    closing = true;
    try {
      await appStore.confirmClose();
    } finally {
      closing = false;
      visible = false;
    }
  }
</script>

{#if visible}
  <div class="close-confirm-backdrop" role="presentation">
    <div class="close-confirm" role="alertdialog" aria-modal="true" aria-labelledby="close-confirm-title">
      <h2 id="close-confirm-title">{i18n.t('closeConfirm.dialog.title')}</h2>
      <p>{i18n.t('closeConfirm.dialog.body')}</p>
      <div class="close-confirm-actions">
        <button type="button" class="button button-secondary" disabled={closing} onclick={stay}>
          {i18n.t('closeConfirm.dialog.stay')}
        </button>
        <button type="button" class="button button-danger" disabled={closing} onclick={confirmClose}>
          {closing ? i18n.t('closeConfirm.dialog.closing') : i18n.t('closeConfirm.dialog.confirm')}
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
