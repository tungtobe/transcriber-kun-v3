<script lang="ts">
  // Menu ⋯ dùng chung cho dòng phiên ở Home và header Transcript detail
  // (story 3.1, spec Code Map): một `button` có `aria-label`, mở/đóng bằng
  // bàn phím, Esc đóng và trả focus về chính nút này (spec Boundaries
  // Always). Chỉ hai mục ở story này (Đổi tên · Xoá) — `Không tạo bảng
  // tag/notes/memo/recording ở story này` (spec Never) nên không có mục nào
  // khác dù UX-DR29 liệt kê thêm cho các story sau.
  import { i18n } from '../i18n/index.svelte';
  import { EllipsisVerticalIcon } from './icons';

  let {
    onRename,
    onDelete,
  }: {
    onRename: () => void;
    onDelete: () => void;
  } = $props();

  let open = $state(false);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let rootEl = $state<HTMLDivElement | null>(null);
  let itemEls: (HTMLButtonElement | null)[] = [];

  const items = $derived([
    { label: i18n.t('sessionMenu.item.rename'), run: onRename },
    { label: i18n.t('sessionMenu.item.delete'), run: onDelete, danger: true },
  ]);

  /** Exposed for the parent's `bind:this` (spec Boundaries: "Esc đóng và trả
   * focus" — a rename/delete opened from this menu must also be able to give
   * focus back here once it's cancelled). */
  export function focusTrigger(): void {
    triggerEl?.focus();
  }

  function close(): void {
    open = false;
  }

  function closeAndFocusTrigger(): void {
    close();
    triggerEl?.focus();
  }

  function toggle(): void {
    open = !open;
  }

  function focusItem(index: number): void {
    const count = itemEls.length;
    if (count === 0) return;
    const next = ((index % count) + count) % count;
    itemEls[next]?.focus();
  }

  $effect(() => {
    if (!open) return;
    focusItem(0);
  });

  function handleTriggerKeydown(event: KeyboardEvent): void {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      open = true;
    }
  }

  function handleItemKeydown(event: KeyboardEvent, index: number): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      closeAndFocusTrigger();
    } else if (event.key === 'ArrowDown') {
      event.preventDefault();
      focusItem(index + 1);
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      focusItem(index - 1);
    } else if (event.key === 'Tab') {
      close();
    }
  }

  function handleWindowPointerDown(event: PointerEvent): void {
    if (!open) return;
    const target = event.target as Node | null;
    if (rootEl && target && !rootEl.contains(target)) close();
  }
</script>

<svelte:window onpointerdown={handleWindowPointerDown} />

<div class="session-menu" bind:this={rootEl}>
  <button
    type="button"
    class="session-menu-trigger"
    aria-label={i18n.t('sessionMenu.trigger.label')}
    aria-haspopup="menu"
    aria-expanded={open}
    bind:this={triggerEl}
    onclick={toggle}
    onkeydown={handleTriggerKeydown}
  >
    <EllipsisVerticalIcon size={18} strokeWidth={1.75} aria-hidden="true" />
  </button>
  {#if open}
    <div class="session-menu-list" role="menu">
      {#each items as item, index (item.label)}
        <button
          type="button"
          role="menuitem"
          class="session-menu-item"
          class:session-menu-item-danger={item.danger}
          bind:this={itemEls[index]}
          onclick={() => {
            close();
            item.run();
          }}
          onkeydown={(event) => handleItemKeydown(event, index)}
        >
          {item.label}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .session-menu {
    position: relative;
    display: inline-flex;
    flex: 0 0 auto;
  }

  .session-menu-trigger {
    display: grid;
    width: 32px;
    height: 32px;
    place-items: center;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-text-secondary);
    cursor: pointer;
  }

  .session-menu-trigger:hover,
  .session-menu-trigger[aria-expanded='true'] {
    border-color: var(--color-border-strong);
    background: var(--color-surface-sunken);
    color: var(--color-text);
  }

  .session-menu-trigger:focus-visible,
  .session-menu-item:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }

  .session-menu-list {
    position: absolute;
    top: calc(100% + var(--space-1));
    right: 0;
    z-index: 20;
    display: grid;
    min-width: 160px;
    padding: var(--space-1);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    box-shadow: 0 1px 2px rgb(17 24 39 / 8%);
  }

  .session-menu-item {
    display: flex;
    min-height: 34px;
    align-items: center;
    padding: 0 var(--space-3);
    border: none;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-text);
    font-family: inherit;
    font-size: var(--text-body-size);
    text-align: left;
    cursor: pointer;
  }

  .session-menu-item:hover {
    background: var(--color-surface-sunken);
  }

  .session-menu-item-danger {
    color: var(--color-danger-strong);
  }
</style>
