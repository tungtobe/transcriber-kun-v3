<script lang="ts">
  // Một component badge duy nhất, 7 biến thể ngữ nghĩa (spec Boundaries
  // Always: "Badge dùng một component 7 biến thể (memo, audio, partial,
  // recover, live, file, token) đúng cặp token DESIGN.md, cao 20 px, radius
  // 6 px, 11px/600, `min-width` (không `width` cố định)"). Màu nền/chữ của
  // từng biến thể tới thẳng từ DESIGN.md dòng 199-225 — không lặp lại một
  // bảng mapping thứ hai ở nơi khác.
  import type { Component } from 'svelte';

  export type BadgeVariant = 'memo' | 'audio' | 'partial' | 'recover' | 'live' | 'file' | 'token';

  let {
    variant,
    label,
    icon,
  }: {
    variant: BadgeVariant;
    label: string;
    /** Icon Lucide tuỳ chọn, 14 px (spec Boundaries: "icon tuỳ chọn 14 px"). */
    icon?: Component;
  } = $props();
</script>

<span class="badge badge-{variant}">
  {#if icon}
    {@const Icon = icon}
    <Icon size={14} strokeWidth={1.75} aria-hidden="true" />
  {/if}
  {label}
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    height: 20px;
    min-width: 20px;
    padding: 0 7px;
    border-radius: var(--radius-sm);
    font-size: var(--text-badge-size);
    font-weight: 600;
    line-height: 1;
    white-space: nowrap;
  }

  .badge-memo {
    background: var(--color-memo-soft);
    color: var(--color-memo);
  }

  .badge-audio {
    background: var(--color-info-soft);
    color: var(--color-info);
  }

  .badge-partial {
    background: var(--color-warning-soft);
    color: var(--color-warning);
  }

  .badge-recover {
    background: var(--color-recover-soft);
    color: var(--color-recover);
  }

  .badge-live {
    background: var(--color-danger-soft);
    color: var(--color-danger-strong);
  }

  .badge-file {
    background: var(--color-surface-sunken);
    color: var(--color-text-secondary);
  }

  .badge-token {
    border: 1px solid var(--color-warning-border);
    background: var(--color-token-soft);
    color: var(--color-warning);
  }
</style>
