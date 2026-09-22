<script lang="ts">
  import { link } from '@keenmate/svelte-spa-router';
  import { AlertCircleIcon, AlertTriangleIcon, InfoIcon } from './icons';

  export type BannerVariant = 'danger' | 'warning' | 'info';

  let {
    variant,
    title,
    message,
    actionLabel,
    actionHref,
    onAction,
  }: {
    variant: BannerVariant;
    /** Part 1 of 3: the category heading. */
    title: string;
    /** Part 2 of 3: cause + fix in one line. */
    message: string;
    /** Part 3 of 3: exactly one action — a link when `actionHref` is set, a button otherwise. */
    actionLabel?: string;
    actionHref?: string;
    onAction?: () => void;
  } = $props();
</script>

<div class="banner banner-{variant}" role="status">
  <span class="banner-icon" aria-hidden="true">
    {#if variant === 'danger'}
      <AlertCircleIcon size={18} strokeWidth={1.75} />
    {:else if variant === 'warning'}
      <AlertTriangleIcon size={18} strokeWidth={1.75} />
    {:else}
      <InfoIcon size={18} strokeWidth={1.75} />
    {/if}
  </span>

  <div class="banner-body">
    <strong class="banner-title">{title}</strong>
    <p class="banner-message">{message}</p>
  </div>

  {#if actionLabel}
    {#if actionHref}
      <a class="banner-action" href={actionHref} use:link>{actionLabel}</a>
    {:else}
      <button class="banner-action" type="button" onclick={() => onAction?.()}>{actionLabel}</button>
    {/if}
  {/if}
</div>

<style>
  .banner {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
  }

  .banner-icon {
    display: flex;
    flex: 0 0 auto;
    padding-top: 1px;
  }

  .banner-body {
    min-width: 0;
    flex: 1;
  }

  .banner-title {
    display: block;
    font-size: var(--text-label-size);
  }

  .banner-message {
    margin: 2px 0 0;
    color: var(--color-text-secondary);
    font-size: var(--text-help-size);
  }

  .banner-action {
    flex: 0 0 auto;
    min-height: 30px;
    padding: 0 var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    background: transparent;
    color: inherit;
    font-size: var(--text-label-size);
    font-weight: 600;
    text-decoration: underline;
    cursor: pointer;
  }

  .banner-danger {
    border-color: var(--color-danger-border);
    background: var(--color-danger-soft);
    color: var(--color-danger-strong);
  }

  .banner-danger .banner-icon {
    color: var(--color-danger);
  }

  .banner-warning {
    border-color: var(--color-warning-border);
    background: var(--color-warning-soft);
    color: var(--color-warning);
  }

  .banner-warning .banner-icon {
    color: var(--color-warning);
  }

  .banner-info {
    border-color: var(--color-info-border);
    background: var(--color-info-soft);
    color: var(--color-info);
  }

  .banner-info .banner-icon {
    color: var(--color-info);
  }
</style>
