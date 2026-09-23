<script lang="ts">
  // Danh sách thông báo kết quả nhận file, có thể đóng từng dòng (story 2.8
  // Always: "một danh sách thông báo có thể đóng"). Mount ở cả Home và
  // Session — `intakeStore` là store module-level nên cả hai màn luôn thấy
  // cùng một danh sách sống (spec Code Map: "notices render trên Home và
  // Session").
  import { link } from '@keenmate/svelte-spa-router';
  import { AlertCircleIcon, AlertTriangleIcon, InfoIcon, XIcon } from './icons';
  import { i18n } from '../i18n/index.svelte';
  import { intakeStore } from '../lib/stores/intake.svelte';
</script>

{#if intakeStore.notices.length > 0}
  <div class="intake-notices" aria-label={i18n.t('intake.notice.dismissLabel')}>
    {#each intakeStore.notices as notice (notice.id)}
      <div class="intake-notice intake-notice-{notice.variant}" role="status">
        <span class="intake-notice-icon" aria-hidden="true">
          {#if notice.variant === 'danger'}
            <AlertCircleIcon size={18} strokeWidth={1.75} />
          {:else if notice.variant === 'warning'}
            <AlertTriangleIcon size={18} strokeWidth={1.75} />
          {:else}
            <InfoIcon size={18} strokeWidth={1.75} />
          {/if}
        </span>

        <div class="intake-notice-body">
          <strong class="intake-notice-title">{notice.title}</strong>
          <p class="intake-notice-message">{notice.message}</p>
        </div>

        {#if notice.actionLabel && notice.actionHref}
          <a class="intake-notice-action" href={notice.actionHref} use:link>{notice.actionLabel}</a>
        {/if}

        <button
          type="button"
          class="intake-notice-dismiss"
          aria-label={i18n.t('intake.notice.dismissLabel')}
          onclick={() => intakeStore.dismiss(notice.id)}
        >
          <XIcon size={14} strokeWidth={1.75} />
        </button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .intake-notices {
    display: grid;
    gap: var(--space-2);
  }

  .intake-notice {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
  }

  .intake-notice-icon {
    display: flex;
    flex: 0 0 auto;
    padding-top: 1px;
  }

  .intake-notice-body {
    min-width: 0;
    flex: 1;
  }

  .intake-notice-title {
    display: block;
    font-size: var(--text-label-size);
  }

  .intake-notice-message {
    margin: 2px 0 0;
    color: var(--color-text-secondary);
    font-size: var(--text-help-size);
  }

  .intake-notice-action {
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
  }

  .intake-notice-dismiss {
    display: grid;
    flex: 0 0 auto;
    width: 24px;
    height: 24px;
    place-items: center;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: inherit;
    cursor: pointer;
    opacity: 0.7;
  }

  .intake-notice-dismiss:hover {
    opacity: 1;
  }

  .intake-notice-danger {
    border-color: var(--color-danger-border);
    background: var(--color-danger-soft);
    color: var(--color-danger-strong);
  }

  .intake-notice-danger .intake-notice-icon {
    color: var(--color-danger);
  }

  .intake-notice-warning {
    border-color: var(--color-warning-border);
    background: var(--color-warning-soft);
    color: var(--color-warning);
  }

  .intake-notice-warning .intake-notice-icon {
    color: var(--color-warning);
  }

  .intake-notice-info {
    border-color: var(--color-info-border);
    background: var(--color-info-soft);
    color: var(--color-info);
  }

  .intake-notice-info .intake-notice-icon {
    color: var(--color-info);
  }
</style>
