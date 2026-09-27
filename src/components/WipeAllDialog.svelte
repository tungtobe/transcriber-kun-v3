<script lang="ts">
  // Dialog "Xoá toàn bộ dữ liệu" (story 3.4, spec Boundaries Always: "Hai
  // bước xác nhận nằm trong cùng một dialog (không chồng modal); nút xác
  // nhận cuối `button-danger-soft`; Esc/huỷ đóng và trả focus về nút mở").
  // Tái dùng đúng khuôn portal/focus/Esc của `ConfirmDialog.svelte` nhưng
  // thêm một bước giữa: bước 1 liệt kê chính xác hai danh sách xoá/giữ (spec
  // Decision OQ9), bước 2 mới là nút nguy hiểm thật sự. Esc hay "Huỷ" ở bất
  // kỳ bước nào đều đóng hẳn dialog (không lùi một bước) -- `onCancel` luôn
  // là "đóng", không phải "quay lại".
  import { i18n } from '../i18n/index.svelte';

  let {
    confirming = false,
    onConfirm,
    onCancel,
  }: {
    confirming?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  let step = $state<1 | 2>(1);
  let cancelEl = $state<HTMLButtonElement | null>(null);

  $effect(() => {
    // Đọc `step` để effect chạy lại mỗi lần bước đổi -- nút "Huỷ" là một
    // phần tử DOM mới ở mỗi bước (nhánh `{#if}` khác nhau), nên phải focus
    // lại, không chỉ một lần lúc mount.
    step;
    cancelEl?.focus();
  });

  // Dời backdrop lên `document.body` -- cùng lý do `ConfirmDialog.svelte`
  // (một tổ tiên `transform` sẽ bẻ `position: fixed`).
  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return {
      destroy() {
        node.remove();
      },
    };
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      onCancel();
    }
  }

  function advance(): void {
    step = 2;
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="confirm-dialog-backdrop" role="presentation" use:portal onkeydown={handleKeydown}>
  <div
    class="confirm-dialog"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="wipe-all-dialog-title"
  >
    {#if step === 1}
      <h2 id="wipe-all-dialog-title">{i18n.t('wipeAll.step1.title')}</h2>
      <p>{i18n.t('wipeAll.step1.body')}</p>
      <dl class="wipe-all-lists">
        <dt>{i18n.t('wipeAll.step1.deleteTitle')}</dt>
        <dd>{i18n.t('wipeAll.step1.deleteList')}</dd>
        <dt>{i18n.t('wipeAll.step1.keepTitle')}</dt>
        <dd>{i18n.t('wipeAll.step1.keepList')}</dd>
      </dl>
      <div class="confirm-dialog-actions">
        <button
          type="button"
          class="button button-secondary"
          bind:this={cancelEl}
          onclick={onCancel}
        >
          {i18n.t('wipeAll.dialog.cancel')}
        </button>
        <button type="button" class="button button-primary" onclick={advance}>
          {i18n.t('wipeAll.dialog.continue')}
        </button>
      </div>
    {:else}
      <h2 id="wipe-all-dialog-title">{i18n.t('wipeAll.step2.title')}</h2>
      <p>{i18n.t('wipeAll.step2.body')}</p>
      <div class="confirm-dialog-actions">
        <button
          type="button"
          class="button button-secondary"
          bind:this={cancelEl}
          disabled={confirming}
          onclick={onCancel}
        >
          {i18n.t('wipeAll.dialog.cancel')}
        </button>
        <button type="button" class="button button-danger-soft" disabled={confirming} onclick={onConfirm}>
          {confirming ? i18n.t('wipeAll.dialog.confirming') : i18n.t('wipeAll.dialog.confirm')}
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .confirm-dialog-backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: grid;
    place-items: center;
    background: var(--color-overlay);
  }

  .confirm-dialog {
    display: grid;
    gap: var(--space-3);
    width: min(480px, calc(100vw - var(--space-8)));
    padding: var(--space-6);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-3xl);
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

  .wipe-all-lists {
    display: grid;
    gap: var(--space-1);
    margin: 0;
  }

  .wipe-all-lists dt {
    color: var(--color-text-secondary);
    font-size: var(--text-label-size);
    font-weight: 600;
  }

  .wipe-all-lists dd {
    margin: 0 0 var(--space-2);
    color: var(--color-text-secondary);
    font-size: var(--text-body-size);
  }

  .confirm-dialog-actions {
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
    font-family: inherit;
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

  .button-primary {
    background: var(--color-primary-action);
    color: var(--color-on-primary);
  }

  .button-danger-soft {
    border-color: var(--color-danger-border);
    background: var(--color-surface);
    color: var(--color-danger-strong);
  }
</style>
