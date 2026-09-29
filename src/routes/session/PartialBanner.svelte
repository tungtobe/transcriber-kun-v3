<script lang="ts">
  // Banner partial ở đầu Transcript (spec Always: "Banner partial liệt kê
  // mọi khoảng `chunk_failed` dạng `mm:ss–mm:ss` (qua `displayTimestamp`) +
  // 'Chạy lại phần thiếu' (`missing`) và 'Chạy lại toàn bộ' (`all`), mỗi nút
  // kèm badge token 'Tốn token Gemini'"). Từng dòng gap riêng lẻ trong luồng
  // Segment (nút "Chạy lại khoảng này") thuộc `SegmentList.svelte`, không
  // phải ở đây — đây chỉ là banner tổng hợp đầu trang.
  import { AlertTriangleIcon } from '../../components/icons';
  import { i18n } from '../../i18n/index.svelte';
  import { displayTimestamp } from '../../lib/time';
  import type { RerunScope } from '../../lib/bindings';
  import Badge from '../../components/Badge.svelte';

  let {
    gapRanges,
    starting,
    errorMessage,
    onRerun,
  }: {
    gapRanges: { startSec: number; endSec: number }[];
    starting: boolean;
    errorMessage: string | null;
    onRerun: (scope: RerunScope) => void;
  } = $props();
</script>

<div class="partial-banner" role="status">
  <span class="partial-banner-icon" aria-hidden="true">
    <AlertTriangleIcon size={18} strokeWidth={1.75} />
  </span>
  <div class="partial-banner-body">
    <strong class="partial-banner-title">{i18n.t('session.partial.title')}</strong>
    {#if gapRanges.length > 0}
      <ul class="partial-banner-ranges">
        {#each gapRanges as range, index (index)}
          <li>{displayTimestamp(range.startSec)}–{displayTimestamp(range.endSec)}</li>
        {/each}
      </ul>
    {/if}
    <div class="partial-banner-actions">
      <button
        type="button"
        class="partial-banner-action"
        disabled={starting}
        onclick={() => onRerun({ kind: 'missing' })}
      >
        {i18n.t('session.partial.rerunMissing')}
        <Badge variant="token" label={i18n.t('session.badge.token')} />
      </button>
      <button
        type="button"
        class="partial-banner-action"
        disabled={starting}
        onclick={() => onRerun({ kind: 'all' })}
      >
        {i18n.t('session.partial.rerunAll')}
        <Badge variant="token" label={i18n.t('session.badge.token')} />
      </button>
    </div>
    {#if errorMessage}
      <p class="partial-banner-error" role="alert">{errorMessage}</p>
    {/if}
  </div>
</div>

<style>
  .partial-banner {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--color-warning-border);
    border-radius: var(--radius-lg);
    background: var(--color-warning-soft);
    color: var(--color-warning);
  }

  .partial-banner-icon {
    display: flex;
    flex: 0 0 auto;
    padding-top: 1px;
  }

  .partial-banner-body {
    min-width: 0;
    flex: 1;
  }

  .partial-banner-title {
    display: block;
    font-size: var(--text-label-size);
  }

  .partial-banner-ranges {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-3);
    max-height: 4.5rem;
    margin: var(--space-1) 0 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
    color: var(--color-text-secondary);
    font-family: var(--font-mono);
    font-size: var(--text-help-size);
    font-variant-numeric: tabular-nums;
  }

  .partial-banner-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }

  .partial-banner-action {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 32px;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-warning-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: var(--text-label-size);
    font-weight: 500;
    cursor: pointer;
  }

  .partial-banner-action:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }

  .partial-banner-error {
    margin: var(--space-2) 0 0;
    color: var(--color-danger);
    font-size: var(--text-help-size);
    font-weight: 600;
  }
</style>
