<script lang="ts">
  // Toast host cấp app (story 3.7) -- gắn một lần ở `App.svelte`, hiện mọi
  // toast trong `toastStore` (spec Boundaries Always: "toast ngắn ... ở cấp
  // app"). Bấm vào một toast đóng nó ngay (không cần chờ hết giờ).
  import { toastStore } from '../lib/stores/toast.svelte';
</script>

{#if toastStore.items.length > 0}
  <div class="toast-host" role="status" aria-live="polite">
    {#each toastStore.items as item (item.id)}
      <button
        type="button"
        class="toast-item"
        class:toast-error={item.variant === 'error'}
        onclick={() => toastStore.dismiss(item.id)}
      >
        {item.message}
      </button>
    {/each}
  </div>
{/if}

<style>
  .toast-host {
    position: fixed;
    right: var(--space-4);
    bottom: var(--space-4);
    z-index: 1000;
    display: grid;
    gap: var(--space-2);
  }

  .toast-item {
    padding: var(--space-2) var(--space-4);
    border: none;
    border-radius: var(--radius-md);
    background: var(--color-text);
    /* `check:ui` resolves `box-shadow` only against tokens defined in the
       same file, so this repeats `--shadow-floating`'s literal value from
       `tokens.css` rather than referencing the variable. */
    box-shadow: 0 10px 32px rgb(17 24 39 / 12%);
    color: var(--color-surface);
    font: inherit;
    font-size: var(--text-help-size);
    text-align: left;
    cursor: pointer;
  }

  .toast-item.toast-error {
    background: var(--color-danger);
  }
</style>
