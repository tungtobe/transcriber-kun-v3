<script lang="ts">
  // Header của `/session/:id` khi đã là một Phiên đã lưu (spec I/O Matrix
  // "Mở Phiên": "Header (quay lại, tên, badge FILE, ngày địa phương, thời
  // lượng, số Segment text)"). `formatTimestamp` (không phải
  // `displayTimestamp`) cho thời lượng — offset dịch một *mốc thời gian*
  // trên timeline, không có nghĩa cộng vào một *độ dài* (spec Always chỉ nói
  // "Timestamp hiển thị" cho vị trí, không phải tổng thời lượng).
  import { link } from '@keenmate/svelte-spa-router';
  import { i18n } from '../../i18n/index.svelte';
  import { formatTimestamp } from '../../lib/time';
  import { ArrowLeftIcon } from '../../components/icons';
  import Badge from '../../components/Badge.svelte';

  let {
    title,
    kind,
    createdAtMs,
    durationSec,
    segmentTextCount,
    recovered,
    partial,
  }: {
    title: string;
    /** `sessions.kind` thô ("file" | "live") — quyết định badge FILE/LIVE
     * (spec I/O Matrix "Phiên live thiếu Proxy": một Phiên live vẫn có thể
     * mở ở `/session/:id`, chỉ khác ở trình phát, không phải header). */
    kind: string;
    createdAtMs: number | null;
    durationSec: number | null;
    segmentTextCount: number;
    recovered: boolean;
    partial: boolean;
  } = $props();

  const LOCALE_TAG: Record<string, string> = { vi: 'vi-VN', en: 'en-US', ja: 'ja-JP' };

  const localDate = $derived(
    createdAtMs === null
      ? null
      : new Date(createdAtMs).toLocaleDateString(LOCALE_TAG[i18n.locale] ?? undefined),
  );
  const durationLabel = $derived(durationSec === null ? null : formatTimestamp(durationSec));
</script>

<header class="session-header">
  <a class="back-link" href="/home" use:link aria-label={i18n.t('session.header.back')}>
    <ArrowLeftIcon size={18} strokeWidth={1.75} />
  </a>

  <div class="session-header-main">
    <div class="session-header-title-row">
      <h1 id="session-title">{title}</h1>
      {#if kind === 'live'}
        <Badge variant="live" label={i18n.t('session.badge.live')} />
      {:else}
        <Badge variant="file" label={i18n.t('session.badge.file')} />
      {/if}
      {#if recovered}
        <Badge variant="recover" label={i18n.t('session.badge.recover')} />
      {/if}
      {#if partial}
        <Badge variant="partial" label={i18n.t('session.badge.partial')} />
      {/if}
    </div>
    <p class="session-header-meta">
      {#if localDate}
        <span>{localDate}</span>
      {/if}
      {#if durationLabel}
        <span>{durationLabel}</span>
      {/if}
      <span>{i18n.t('session.header.segmentCount', { count: segmentTextCount })}</span>
    </p>
  </div>
</header>

<style>
  .session-header {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-6);
    border-bottom: 1px solid var(--color-border);
    background: var(--color-surface);
  }

  .back-link {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    flex: 0 0 auto;
    border-radius: var(--radius-md);
    color: var(--color-text-secondary);
    text-decoration: none;
  }

  .back-link:hover {
    background: var(--color-surface-sunken);
  }

  .session-header-main {
    min-width: 0;
    flex: 1;
  }

  .session-header-title-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  h1 {
    margin: 0;
    overflow: hidden;
    font-size: var(--text-screen-title-size);
    line-height: 1.35;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .session-header-meta {
    display: flex;
    gap: var(--space-3);
    margin: var(--space-1) 0 0;
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
  }

  .session-header-meta span:not(:last-child)::after {
    content: '·';
    margin-left: var(--space-3);
    color: var(--color-border-strong);
  }
</style>
