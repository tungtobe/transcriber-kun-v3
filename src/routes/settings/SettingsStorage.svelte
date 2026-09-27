<script lang="ts">
  // Settings → Lưu trữ (story 3.4, spec Approach): dung lượng Media/DB/số
  // Phiên, nút "Mở thư mục", và "Xoá toàn bộ dữ liệu…" với xác nhận hai bước
  // (`WipeAllDialog`). Mẫu theo `SettingsDiagnostics.svelte` (state cục bộ,
  // `BannerStack` cho lỗi category, không store riêng cho số liệu — chỉ
  // `libraryStore.wipeAll()` cho hành động xoá vì nó cần reload
  // sessions/tags mà `libraryStore` đã sở hữu, spec Code Map).
  import { onMount } from 'svelte';
  import SettingsRow from '../../components/SettingsRow.svelte';
  import BannerStack from '../../components/BannerStack.svelte';
  import type { BannerItem } from '../../components/BannerStack.svelte';
  import WipeAllDialog from '../../components/WipeAllDialog.svelte';
  import { i18n } from '../../i18n/index.svelte';
  import { errorHint, errorTitle } from '../../lib/errors';
  import { commands, type AppError, type StorageStats } from '../../lib/bindings';
  import { libraryStore } from '../../lib/stores/library.svelte';

  const STATS_UNAVAILABLE_ERROR: AppError = {
    category: 'storage',
    code: 'storage',
    detailRedacted: 'storage stats unavailable',
  };
  const OPEN_DIR_UNAVAILABLE_ERROR: AppError = {
    category: 'storage',
    code: 'storage',
    detailRedacted: 'open data dir unavailable',
  };

  let stats = $state<StorageStats | null>(null);
  let statsError = $state<AppError | null>(null);

  let openDirError = $state<AppError | null>(null);

  let wipeButtonRef = $state<HTMLButtonElement | null>(null);
  let wipeDialogOpen = $state(false);
  let wiping = $state(false);
  let wipeError = $state<AppError | null>(null);
  let wipeBusyNotice = $state(false);
  let wipeSuccessNotice = $state(false);

  onMount(() => {
    void loadStats();
  });

  async function loadStats(): Promise<void> {
    statsError = null;
    try {
      const result = await commands.libraryStorageStats();
      if (result.status === 'ok') {
        stats = result.data;
      } else {
        statsError = result.error;
      }
    } catch {
      statsError = STATS_UNAVAILABLE_ERROR;
    }
  }

  const statsBanners = $derived<BannerItem[]>(
    statsError
      ? [{
          id: 'storage-stats-error',
          variant: 'warning',
          title: errorTitle(statsError),
          message: errorHint(statsError),
        }]
      : [],
  );

  const openDirBanners = $derived<BannerItem[]>(
    openDirError
      ? [{
          id: 'storage-open-dir-error',
          variant: 'warning',
          title: errorTitle(openDirError),
          message: errorHint(openDirError),
        }]
      : [],
  );

  const wipeBanners = $derived<BannerItem[]>(
    wipeError
      ? [{
          id: 'storage-wipe-error',
          variant: 'warning',
          title: errorTitle(wipeError),
          message: errorHint(wipeError),
        }]
      : [],
  );

  // Đơn vị dễ đọc B/KB/MB/GB/TB (spec Always) -- 0 hiển thị "0 B" thay vì
  // "0.0 B", và chỉ giữ một chữ số thập phân từ 10 trở xuống mỗi đơn vị (đủ
  // đọc, không rối số).
  function formatBytes(bytes: number): string {
    if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let value = bytes;
    let unitIndex = 0;
    while (value >= 1024 && unitIndex < units.length - 1) {
      value /= 1024;
      unitIndex += 1;
    }
    const decimals = unitIndex === 0 || value >= 10 ? 0 : 1;
    return `${value.toFixed(decimals)} ${units[unitIndex]}`;
  }

  // `?? 0`: specta xuất `f64` dạng `number | null` (cùng quy ước
  // `SessionListItem.durationSec`) -- `null` chỉ xảy ra trước lần tải đầu.
  const mediaBytes = $derived(stats?.mediaBytes ?? 0);
  const dbBytes = $derived(stats?.dbBytes ?? 0);
  const totalBytes = $derived(mediaBytes + dbBytes);
  const mediaPercent = $derived(totalBytes > 0 ? (mediaBytes / totalBytes) * 100 : 0);
  const dbPercent = $derived(totalBytes > 0 ? 100 - mediaPercent : 0);

  async function handleOpenDir(): Promise<void> {
    openDirError = null;
    try {
      const result = await commands.libraryOpenDataDir();
      if (result.status !== 'ok') {
        openDirError = result.error;
      }
    } catch {
      openDirError = OPEN_DIR_UNAVAILABLE_ERROR;
    }
  }

  function openWipeDialog(): void {
    wipeError = null;
    wipeBusyNotice = false;
    wipeSuccessNotice = false;
    wipeDialogOpen = true;
  }

  function cancelWipeDialog(): void {
    wipeDialogOpen = false;
    wipeButtonRef?.focus();
  }

  // Đóng dialog dù kết quả là gì (spec: cùng khuôn `SessionHeader.confirmDelete`)
  // -- giải thích Busy/lỗi hiển thị inline cạnh nút, không mở lỗi chung.
  async function confirmWipe(): Promise<void> {
    wiping = true;
    const result = await libraryStore.wipeAll();
    wiping = false;
    wipeDialogOpen = false;
    wipeButtonRef?.focus();

    if (result.status === 'error') {
      wipeError = result.error;
      return;
    }
    if (result.outcome === 'wiped') {
      wipeSuccessNotice = true;
      void loadStats();
      return;
    }
    wipeBusyNotice = true;
  }
</script>

<div class="settings-fields">
  <SettingsRow
    label={i18n.t('settings.storage.mediaLabel')}
    help={i18n.t('settings.storage.mediaHelp')}
    helperText={i18n.t('settings.storage.mediaHelper')}
  >
    <p class="storage-value mono">{formatBytes(mediaBytes)}</p>
    <div class="storage-ratio-bar" aria-hidden="true">
      <div class="storage-ratio-media" style={`width: ${mediaPercent}%`}></div>
      <div class="storage-ratio-db" style={`width: ${dbPercent}%`}></div>
    </div>
    <BannerStack banners={statsBanners} />
  </SettingsRow>

  <SettingsRow
    label={i18n.t('settings.storage.dbLabel')}
    help={i18n.t('settings.storage.dbHelp')}
    helperText={i18n.t('settings.storage.dbHelper')}
  >
    <p class="storage-value mono">{formatBytes(dbBytes)}</p>
  </SettingsRow>

  <SettingsRow
    label={i18n.t('settings.storage.sessionsLabel')}
    help={i18n.t('settings.storage.sessionsHelp')}
    helperText={i18n.t('settings.storage.sessionsHelper')}
  >
    <p class="storage-value mono">{stats?.sessionCount ?? 0}</p>
  </SettingsRow>

  <SettingsRow
    label={i18n.t('settings.storage.openDirLabel')}
    help={i18n.t('settings.storage.openDirHelp')}
    helperText={i18n.t('settings.storage.openDirHelper')}
  >
    <button type="button" onclick={() => void handleOpenDir()}>
      {i18n.t('settings.storage.openDirButton')}
    </button>
    <BannerStack banners={openDirBanners} />
  </SettingsRow>

  <SettingsRow
    label={i18n.t('settings.storage.wipeLabel')}
    help={i18n.t('settings.storage.wipeHelp')}
    helperText={i18n.t('settings.storage.wipeHelper')}
  >
    <button
      type="button"
      class="button-danger-soft"
      bind:this={wipeButtonRef}
      disabled={wiping}
      onclick={openWipeDialog}
    >
      {i18n.t('settings.storage.wipeButton')}
    </button>
    <div class="action-status" role="status" aria-live="polite">
      {#if wipeSuccessNotice}
        <p>{i18n.t('settings.storage.wipeSuccess')}</p>
      {:else if wipeBusyNotice}
        <p>{i18n.t('settings.storage.wipeBusyNotice')}</p>
      {/if}
    </div>
    <BannerStack banners={wipeBanners} />
  </SettingsRow>
</div>

{#if wipeDialogOpen}
  <WipeAllDialog
    confirming={wiping}
    onConfirm={() => void confirmWipe()}
    onCancel={cancelWipeDialog}
  />
{/if}

<style>
  .settings-fields {
    display: grid;
  }

  .storage-value {
    margin: 0;
    padding: 0 var(--space-3);
    min-height: 36px;
    display: flex;
    align-items: center;
    width: fit-content;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    font-size: var(--text-option-size);
    font-variant-numeric: tabular-nums;
  }

  .storage-ratio-bar {
    display: flex;
    height: 8px;
    width: 100%;
    max-width: 320px;
    overflow: hidden;
    border-radius: var(--radius-full);
    background: var(--color-surface-sunken);
  }

  .storage-ratio-media {
    height: 100%;
    background: var(--color-accent);
  }

  .storage-ratio-db {
    height: 100%;
    background: var(--color-info);
  }

  .action-status {
    min-height: 20px;
  }

  .action-status p {
    margin: 0;
    color: var(--color-text-secondary);
    font-size: var(--text-help-size);
  }

  button {
    display: inline-flex;
    min-height: 36px;
    width: fit-content;
    align-items: center;
    padding: 0 var(--space-4);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    cursor: pointer;
  }

  button[disabled] {
    cursor: not-allowed;
    opacity: 0.6;
  }

  .button-danger-soft {
    border-color: var(--color-danger-border);
    color: var(--color-danger-strong);
  }
</style>
