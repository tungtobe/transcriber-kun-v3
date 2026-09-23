<script lang="ts">
  // Overlay toàn cửa sổ khi đang kéo file vào (story 2.8 Design Notes: kéo
  // thả dùng sự kiện drag-drop native của webview, không phải HTML5 `drop`).
  // Thuần hiển thị — không nhận pointer event nào, việc nhận file thật sự đi
  // qua `AppShell`'s `onDragDropEvent` listener.
  import { i18n } from '../i18n/index.svelte';

  let { active }: { active: boolean } = $props();
</script>

{#if active}
  <div class="drop-overlay" role="presentation" aria-hidden="true">
    <p>{i18n.t('dropzone.overlay.message')}</p>
  </div>
{/if}

<style>
  .drop-overlay {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: grid;
    place-items: center;
    border: 1px dashed var(--color-accent-border);
    border-radius: var(--radius-2xl);
    background: var(--color-accent-soft);
    pointer-events: none;
  }

  .drop-overlay p {
    margin: 0;
    padding: var(--space-4) var(--space-6);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    color: var(--color-accent);
    font-size: var(--text-h2-size);
    font-weight: 600;
  }
</style>
