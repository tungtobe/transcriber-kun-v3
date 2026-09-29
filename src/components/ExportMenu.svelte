<script lang="ts">
  // Menu tải xuống của Transcript detail: một nút icon, mở danh sách gồm các
  // định dạng transcript (TXT/SRT/JSON) và, nếu phiên có, file audio
  // recording gốc. Cùng quy ước với SessionMenu: Esc đóng và trả focus về
  // nút, mũi tên di chuyển, danh sách nổi trên document.body (floating) để
  // không bị cắt bởi vùng cuộn hay overflow.
  import { i18n } from '../i18n/index.svelte';
  import { DownloadIcon } from './icons';
  import { floating } from '../lib/floating';

  export type ExportFormat = 'txt' | 'srt' | 'json';

  let {
    disabled = false,
    onExport,
    onDownloadRecording,
    downloadDisabledReason,
  }: {
    /** Khoá toàn bộ nút (đang xuất, hoặc chưa có transcript). */
    disabled?: boolean;
    onExport: (format: ExportFormat) => void;
    /** Có mặt chỉ khi phiên có file recording để tải. */
    onDownloadRecording?: () => void;
    downloadDisabledReason?: string;
  } = $props();

  type MenuItem = { key: string; label: string; run: () => void; disabled?: boolean; hint?: string };

  let open = $state(false);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let listEl = $state<HTMLDivElement | null>(null);
  let itemEls = $state<(HTMLButtonElement | null)[]>([]);

  const items = $derived<MenuItem[]>([
    { key: 'txt', label: i18n.t('session.export.txt'), run: () => onExport('txt') },
    { key: 'srt', label: i18n.t('session.export.srt'), run: () => onExport('srt') },
    { key: 'json', label: i18n.t('session.export.json'), run: () => onExport('json') },
    ...(onDownloadRecording
      ? [{
          key: 'audio',
          label: i18n.t('session.export.audio'),
          run: onDownloadRecording,
          disabled: Boolean(downloadDisabledReason),
          hint: downloadDisabledReason,
        }]
      : []),
  ]);

  function close(): void {
    open = false;
  }

  function closeAndFocusTrigger(): void {
    close();
    triggerEl?.focus();
  }

  function focusItem(index: number, direction = 1): void {
    const count = itemEls.length;
    for (let offset = 0; offset < count; offset += 1) {
      const next = (((index + offset * direction) % count) + count) % count;
      if (itemEls[next] && !itemEls[next]?.disabled) {
        itemEls[next]?.focus();
        return;
      }
    }
  }

  $effect(() => {
    if (open) focusItem(0);
  });

  $effect(() => {
    if (disabled) open = false;
  });

  function handleTriggerKeydown(event: KeyboardEvent): void {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      if (!disabled) open = true;
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
      focusItem(index - 1, -1);
    } else if (event.key === 'Tab') {
      close();
    }
  }

  function handleWindowPointerDown(event: PointerEvent): void {
    if (!open) return;
    const target = event.target as Node | null;
    if (!target || triggerEl?.contains(target) || listEl?.contains(target)) return;
    close();
  }
</script>

<svelte:window onpointerdown={handleWindowPointerDown} />

<span class="export-menu">
  <button
    type="button"
    class="export-menu-trigger"
    aria-label={i18n.t('session.export.menuLabel')}
    title={i18n.t('session.export.menuLabel')}
    aria-haspopup="menu"
    aria-expanded={open}
    {disabled}
    bind:this={triggerEl}
    onclick={() => (open = !open)}
    onkeydown={handleTriggerKeydown}
  >
    <DownloadIcon size={18} strokeWidth={1.75} aria-hidden="true" />
  </button>
  {#if open}
    <div class="export-menu-list" role="menu" bind:this={listEl} use:floating={triggerEl}>
      {#each items as item, index (item.key)}
        <button
          type="button"
          role="menuitem"
          class="export-menu-item"
          class:export-menu-item-disabled={item.disabled}
          disabled={item.disabled}
          bind:this={itemEls[index]}
          onclick={() => {
            close();
            item.run();
          }}
          onkeydown={(event) => handleItemKeydown(event, index)}
        >
          <span>{item.label}</span>
          {#if item.hint}
            <span class="export-menu-item-hint">{item.hint}</span>
          {/if}
        </button>
      {/each}
    </div>
  {/if}
</span>

<style>
  .export-menu {
    position: relative;
    display: inline-flex;
    flex: 0 0 auto;
  }

  .export-menu-trigger {
    display: grid;
    width: 34px;
    height: 34px;
    place-items: center;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    cursor: pointer;
  }

  .export-menu-trigger:hover:not(:disabled),
  .export-menu-trigger[aria-expanded='true'] {
    background: var(--color-surface-sunken);
  }

  .export-menu-trigger:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }

  .export-menu-trigger:focus-visible,
  .export-menu-item:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }

  .export-menu-list {
    position: absolute;
    top: calc(100% + var(--space-1));
    right: 0;
    z-index: 40;
    display: grid;
    min-width: 180px;
    padding: var(--space-1);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    box-shadow: 0 1px 2px rgb(17 24 39 / 8%);
  }

  .export-menu-item {
    display: flex;
    flex-direction: column;
    min-height: 34px;
    align-items: flex-start;
    justify-content: center;
    padding: var(--space-1) var(--space-3);
    border: none;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-text);
    font-family: inherit;
    font-size: var(--text-body-size);
    text-align: left;
    cursor: pointer;
  }

  .export-menu-item:hover:not(:disabled) {
    background: var(--color-surface-sunken);
  }

  .export-menu-item-disabled {
    color: var(--color-text-muted);
    cursor: not-allowed;
  }

  .export-menu-item-hint {
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
    line-height: 1.3;
  }
</style>
