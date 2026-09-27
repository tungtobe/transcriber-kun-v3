<script lang="ts">
  // Một dòng Phiên trong danh sách Home (story 2.9, spec Boundaries "Có
  // phiên"; story 3.1: menu ⋯ + đổi tên/xoá). Bình thường, khu vực chính
  // (tên + badge + ngày + loại + thời lượng) là một `<a>` mở `/session/:id` —
  // click hoặc Enter (native `<a>` xử lý Enter miễn phí). `SessionMenu` là
  // một `button` độc lập, không lồng trong `<a>` (spec Code Map: "không lồng
  // button trong `<a>`"). Khi đang đổi tên, khu vực chính đổi thành
  // `InlineRename` thay vì link (đang gõ thì không nên vô tình điều hướng).
  //
  // `.session-row` giữ đúng `height: 56px` trong MỌI trạng thái (kể cả đang
  // đổi tên hay có lỗi Busy) — `SessionList.svelte` là một virtual list tự
  // viết giả định mỗi dòng cao đúng 56px cho toán `scrollTop`/`translateY`;
  // lỗi/gợi ý bên dưới input hay dòng phải nổi bằng `position: absolute`
  // (xem `InlineRename.svelte` và `.session-row-inline-notice` dưới đây),
  // không bao giờ đẩy layout dòng khác.
  import { link } from '@keenmate/svelte-spa-router';
  import { i18n } from '../../i18n/index.svelte';
  import { formatTimestamp } from '../../lib/time';
  import { libraryStore } from '../../lib/stores/library.svelte';
  import Badge from '../../components/Badge.svelte';
  import SessionMenu from '../../components/SessionMenu.svelte';
  import InlineRename from '../../components/InlineRename.svelte';
  import ConfirmDialog from '../../components/ConfirmDialog.svelte';
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

  let menuRef = $state<{ focusTrigger: () => void } | null>(null);

  let renaming = $state(false);
  let renameSaving = $state(false);
  let renameError = $state<string | null>(null);

  let deleteConfirming = $state(false);
  let deleteSaving = $state(false);
  let deleteError = $state<string | null>(null);

  function startRename(): void {
    deleteError = null;
    renameError = null;
    renaming = true;
  }

  function cancelRename(): void {
    renaming = false;
    renameError = null;
    menuRef?.focusTrigger();
  }

  async function saveRename(title: string): Promise<void> {
    renameSaving = true;
    const result = await libraryStore.rename(session.sessionId, title);
    renameSaving = false;
    if (result.status === 'ok') {
      renaming = false;
      renameError = null;
    } else {
      renameError = i18n.t('sessionRename.error.failed');
    }
  }

  function startDelete(): void {
    renameError = null;
    deleteError = null;
    deleteConfirming = true;
  }

  function cancelDelete(): void {
    deleteConfirming = false;
    menuRef?.focusTrigger();
  }

  async function confirmDelete(): Promise<void> {
    deleteSaving = true;
    const result = await libraryStore.remove(session.sessionId);
    deleteSaving = false;
    deleteConfirming = false;
    if (result.status === 'ok' && result.outcome === 'deleted') {
      // Dòng này biến mất qua reload của `libraryStore.remove` — không còn
      // gì để làm ở đây.
      return;
    }
    deleteError =
      result.status === 'ok'
        ? i18n.t('sessionDelete.error.busy')
        : i18n.t('sessionDelete.error.failed');
    menuRef?.focusTrigger();
  }
</script>

<div class="session-row">
  {#if renaming}
    <div class="session-row-name session-row-editing">
      <InlineRename
        value={session.title}
        label={i18n.t('sessionRename.input.label')}
        saving={renameSaving}
        serverError={renameError}
        onSave={saveRename}
        onCancel={cancelRename}
      />
    </div>
  {:else}
    <a class="session-row-link" href={`/session/${session.sessionId}`} use:link>
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
  {/if}
  <SessionMenu bind:this={menuRef} onRename={startRename} onDelete={startDelete} />
  {#if deleteError}
    <p class="session-row-inline-notice" role="alert">{deleteError}</p>
  {/if}
</div>

{#if deleteConfirming}
  <ConfirmDialog
    title={i18n.t('sessionDelete.dialog.title')}
    body={i18n.t('sessionDelete.dialog.body')}
    confirmLabel={deleteSaving ? i18n.t('sessionDelete.dialog.deleting') : i18n.t('sessionDelete.dialog.confirm')}
    cancelLabel={i18n.t('sessionDelete.dialog.cancel')}
    confirming={deleteSaving}
    onConfirm={confirmDelete}
    onCancel={cancelDelete}
  />
{/if}

<style>
  .session-row {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-4);
    height: 56px;
    padding: 0 var(--space-4);
    border-bottom: 1px solid var(--color-border);
    color: var(--color-text);
  }

  .session-row:hover {
    background: var(--color-surface-sunken);
  }

  .session-row-link {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    gap: var(--space-4);
    color: inherit;
    text-decoration: none;
  }

  .session-row-name {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    gap: var(--space-2);
  }

  .session-row-editing {
    min-width: 0;
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

  /* Nổi bên dưới dòng bằng `position: absolute` -- không bao giờ đẩy chiều
   * cao 56px của `.session-row` (virtual list giả định mỗi dòng đúng 56px). */
  .session-row-inline-notice {
    position: absolute;
    top: 100%;
    right: var(--space-4);
    left: var(--space-4);
    z-index: 5;
    margin: 2px 0 0;
    padding: 2px var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--color-danger-soft);
    color: var(--color-danger-strong);
    font-size: var(--text-help-size);
  }
</style>
