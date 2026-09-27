<script lang="ts">
  // Dialog xác nhận dùng chung (story 3.1, UX-DR16: "Dialog rộng 480, radius
  // 16, overlay `overlay`... hành động nguy hiểm tách sang phải,
  // `button-danger-soft`; modal chồng tối đa một cấp"). Style lấy từ
  // `CloseConfirm.svelte` (spec Code Map) nhưng thêm quản lý focus/Esc theo
  // Design Notes: "focus nút huỷ khi mở; Esc = huỷ" — `CloseConfirm` (viết
  // trước quy ước này) chưa làm, component mới này thì có ngay từ đầu.
  let {
    title,
    body,
    confirmLabel,
    cancelLabel,
    confirming = false,
    onConfirm,
    onCancel,
  }: {
    title: string;
    body: string;
    confirmLabel: string;
    cancelLabel: string;
    confirming?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  let cancelEl = $state<HTMLButtonElement | null>(null);

  $effect(() => {
    cancelEl?.focus();
  });

  // Dời backdrop lên `document.body`: dialog có thể được render bên trong
  // một tổ tiên có `transform` (virtual list ở Home dịch dòng bằng
  // `translateY`), mà `position: fixed` dưới tổ tiên `transform` lại bám theo
  // tổ tiên đó thay vì viewport.
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
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="confirm-dialog-backdrop" role="presentation" use:portal onkeydown={handleKeydown}>
  <div
    class="confirm-dialog"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="confirm-dialog-title"
  >
    <h2 id="confirm-dialog-title">{title}</h2>
    <p>{body}</p>
    <div class="confirm-dialog-actions">
      <button
        type="button"
        class="button button-secondary"
        bind:this={cancelEl}
        disabled={confirming}
        onclick={onCancel}
      >
        {cancelLabel}
      </button>
      <button
        type="button"
        class="button button-danger-soft"
        disabled={confirming}
        onclick={onConfirm}
      >
        {confirmLabel}
      </button>
    </div>
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

  .button-danger-soft {
    border-color: var(--color-danger-border);
    background: var(--color-surface);
    color: var(--color-danger-strong);
  }
</style>
