<script lang="ts">
  // Header của `/session/:id` khi đã là một Phiên đã lưu (spec I/O Matrix
  // "Mở Phiên": "Header (quay lại, tên, badge FILE, ngày địa phương, thời
  // lượng, số Segment text)"). `formatTimestamp` (không phải
  // `displayTimestamp`) cho thời lượng — offset dịch một *mốc thời gian*
  // trên timeline, không có nghĩa cộng vào một *độ dài* (spec Always chỉ nói
  // "Timestamp hiển thị" cho vị trí, không phải tổng thời lượng).
  //
  // Story 3.1: click tên (nút) hoặc menu ⋯ đều mở đổi tên inline; Xoá mở
  // dialog xác nhận rồi báo `onDeleted` cho `Session.svelte` điều hướng về
  // `/home` (spec Code Map: "header nhận `sessionId`/callback"). Đổi tên/xoá
  // thật đi qua `libraryStore` (cùng store Home dùng) — component này chỉ
  // giữ state UI (đang sửa/đang xoá/lỗi inline).
  import { link } from '@keenmate/svelte-spa-router';
  import { i18n } from '../../i18n/index.svelte';
  import { formatTimestamp } from '../../lib/time';
  import { libraryStore } from '../../lib/stores/library.svelte';
  import { ArrowLeftIcon } from '../../components/icons';
  import Badge from '../../components/Badge.svelte';
  import SessionMenu from '../../components/SessionMenu.svelte';
  import InlineRename from '../../components/InlineRename.svelte';
  import ConfirmDialog from '../../components/ConfirmDialog.svelte';

  let {
    sessionId,
    title,
    kind,
    createdAtMs,
    durationSec,
    segmentTextCount,
    recovered,
    partial,
    onRenamed,
    onDeleted,
  }: {
    sessionId: string;
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
    /** Gọi với tên mới đã lưu -- `Session.svelte` cập nhật `view.detail.title`. */
    onRenamed: (title: string) => void;
    /** Gọi khi outcome là `deleted` -- `Session.svelte` điều hướng về `/home`. */
    onDeleted: () => void;
  } = $props();

  const LOCALE_TAG: Record<string, string> = { vi: 'vi-VN', en: 'en-US', ja: 'ja-JP' };

  const localDate = $derived(
    createdAtMs === null
      ? null
      : new Date(createdAtMs).toLocaleDateString(LOCALE_TAG[i18n.locale] ?? undefined),
  );
  const durationLabel = $derived(durationSec === null ? null : formatTimestamp(durationSec));

  let titleButtonRef = $state<HTMLButtonElement | null>(null);
  let menuRef = $state<{ focusTrigger: () => void } | null>(null);

  // Đổi tên có thể mở từ nút tên hoặc từ menu ⋯ -- Esc phải trả focus về
  // đúng nơi đã mở nó (spec Boundaries: "Esc huỷ và trả focus về nút/tên đã
  // mở").
  let renameOpener: 'title' | 'menu' = 'title';
  let renaming = $state(false);
  let renameSaving = $state(false);
  let renameError = $state<string | null>(null);

  let deleteConfirming = $state(false);
  let deleteSaving = $state(false);
  let deleteError = $state<string | null>(null);

  function openRename(opener: 'title' | 'menu'): void {
    renameOpener = opener;
    deleteError = null;
    renameError = null;
    renaming = true;
  }

  function cancelRename(): void {
    renaming = false;
    renameError = null;
    if (renameOpener === 'menu') {
      menuRef?.focusTrigger();
    } else {
      titleButtonRef?.focus();
    }
  }

  async function saveRename(newTitle: string): Promise<void> {
    renameSaving = true;
    const result = await libraryStore.rename(sessionId, newTitle);
    renameSaving = false;
    if (result.status === 'ok') {
      renaming = false;
      renameError = null;
      if (result.title !== null) onRenamed(result.title);
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
    const result = await libraryStore.remove(sessionId);
    deleteSaving = false;
    deleteConfirming = false;
    if (result.status === 'ok' && result.outcome === 'deleted') {
      onDeleted();
      return;
    }
    deleteError =
      result.status === 'ok'
        ? i18n.t('sessionDelete.error.busy')
        : i18n.t('sessionDelete.error.failed');
    menuRef?.focusTrigger();
  }
</script>

<header class="session-header">
  <a class="back-link" href="/home" use:link aria-label={i18n.t('session.header.back')}>
    <ArrowLeftIcon size={18} strokeWidth={1.75} />
  </a>

  <div class="session-header-main">
    <div class="session-header-title-row">
      {#if renaming}
        <InlineRename
          value={title}
          label={i18n.t('sessionRename.input.label')}
          saving={renameSaving}
          serverError={renameError}
          onSave={saveRename}
          onCancel={cancelRename}
        />
      {:else}
        <button
          type="button"
          class="session-header-title-button"
          bind:this={titleButtonRef}
          onclick={() => openRename('title')}
        >
          <h1 id="session-title">{title}</h1>
        </button>
      {/if}
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
    {#if deleteError}
      <p class="session-header-inline-notice" role="alert">{deleteError}</p>
    {/if}
  </div>

  <SessionMenu bind:this={menuRef} onRename={() => openRename('menu')} onDelete={startDelete} />
</header>

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
    position: relative;
    min-width: 0;
    flex: 1;
  }

  .session-header-title-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .session-header-title-button {
    max-width: 100%;
    padding: 0;
    border: none;
    background: none;
    cursor: pointer;
    text-align: left;
  }

  .session-header-title-button:hover h1 {
    text-decoration: underline;
  }

  .session-header-title-button:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
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

  .session-header-inline-notice {
    margin: var(--space-1) 0 0;
    color: var(--color-danger-strong);
    font-size: var(--text-help-size);
  }
</style>
