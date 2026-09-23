<script lang="ts">
  // Một dòng Phiên trong danh sách Home (story 2.9, spec Boundaries "Có
  // phiên"). Cả dòng là một liên kết tới `/session/:id` — click hoặc Enter
  // mở (native `<a>` xử lý Enter miễn phí). Thứ tự hiển thị đúng theo spec:
  // tên (kèm badge partial/recover ngay sau) -> ngày cục bộ -> badge
  // live/file -> thời lượng mono tabular. Không hiển thị model, số Segment,
  // cột trạng thái (spec Boundaries: "Không hiển thị model, số Segment, cột
  // trạng thái").
  import { link } from '@keenmate/svelte-spa-router';
  import { i18n } from '../../i18n/index.svelte';
  import { formatTimestamp } from '../../lib/time';
  import Badge from '../../components/Badge.svelte';
  import type { SessionListItem } from '../../lib/bindings';

  let { session }: { session: SessionListItem } = $props();

  const LOCALE_TAG: Record<string, string> = { vi: 'vi-VN', en: 'en-US', ja: 'ja-JP' };

  const localDate = $derived(
    session.createdAt === null
      ? null
      : new Date(session.createdAt).toLocaleDateString(LOCALE_TAG[i18n.locale] ?? undefined),
  );
  const durationLabel = $derived(
    session.durationSec === null ? null : formatTimestamp(session.durationSec),
  );
</script>

<a class="session-row" href={`/session/${session.sessionId}`} use:link>
  <span class="session-row-name">
    <span class="session-row-title">{session.title}</span>
    {#if session.missingGapCount > 0}
      <Badge
        variant="partial"
        label={i18n.t('home.sessionRow.missingGaps', { count: session.missingGapCount })}
      />
    {/if}
    {#if session.recovered}
      <Badge variant="recover" label={i18n.t('home.sessionRow.recovered')} />
    {/if}
  </span>
  {#if localDate}
    <span class="session-row-date">{localDate}</span>
  {/if}
  <Badge
    variant={session.kind === 'live' ? 'live' : 'file'}
    label={i18n.t(session.kind === 'live' ? 'session.badge.live' : 'session.badge.file')}
  />
  {#if durationLabel}
    <span class="session-row-duration mono">{durationLabel}</span>
  {/if}
</a>

<style>
  .session-row {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    height: 56px;
    padding: 0 var(--space-4);
    border-bottom: 1px solid var(--color-border);
    color: var(--color-text);
    text-decoration: none;
  }

  .session-row:hover {
    background: var(--color-surface-sunken);
  }

  .session-row-name {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    gap: var(--space-2);
  }

  .session-row-title {
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .session-row-date {
    flex: 0 0 auto;
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
  }

  .session-row-duration {
    flex: 0 0 auto;
    min-width: 56px;
    color: var(--color-text-secondary);
    font-size: var(--text-body-size);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
