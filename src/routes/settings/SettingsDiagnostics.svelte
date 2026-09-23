<script lang="ts">
  // Settings → Chẩn đoán: bộ đếm cục bộ (phiên, lỗi theo category, crash) và
  // hai hành động — xuất gói nhật ký (dialog lưu hệ thống mở phía Rust) và
  // xoá nhật ký. Không có checkbox thống kê/opt-in, không request mạng nào
  // (spec Never).
  import { onMount } from 'svelte';
  import SettingsRow from '../../components/SettingsRow.svelte';
  import BannerStack from '../../components/BannerStack.svelte';
  import type { BannerItem } from '../../components/BannerStack.svelte';
  import { i18n, type TranslationKey } from '../../i18n/index.svelte';
  import { diagnosticsStore } from '../../lib/stores/diagnostics.svelte';
  import { errorHint, errorTitle } from '../../lib/errors';
  import type { Category } from '../../lib/bindings';

  onMount(() => {
    void diagnosticsStore.load();
  });

  // Cùng thứ tự category ổn định dùng ở `error.<category>.title` — tái dùng
  // nhãn ngắn đã có thay vì đặt thêm khoá i18n mới cho tên category.
  const CATEGORY_TITLE_KEY: Record<Category, TranslationKey> = {
    quota: 'error.quota.title',
    auth: 'error.auth.title',
    model: 'error.model.title',
    network: 'error.network.title',
    format: 'error.format.title',
    permission: 'error.permission.title',
    storage: 'error.storage.title',
    blocked: 'error.blocked.title',
  };

  const summaryBanners = $derived<BannerItem[]>(
    diagnosticsStore.error
      ? [{
          id: 'diagnostics-summary-error',
          variant: 'warning',
          title: errorTitle(diagnosticsStore.error),
          message: errorHint(diagnosticsStore.error),
        }]
      : [],
  );

  const exportBanners = $derived<BannerItem[]>(
    diagnosticsStore.exportError
      ? [{
          id: 'diagnostics-export-error',
          variant: 'warning',
          title: errorTitle(diagnosticsStore.exportError),
          message: errorHint(diagnosticsStore.exportError),
        }]
      : [],
  );

  const clearBanners = $derived<BannerItem[]>(
    diagnosticsStore.clearError
      ? [{
          id: 'diagnostics-clear-error',
          variant: 'warning',
          title: errorTitle(diagnosticsStore.clearError),
          message: errorHint(diagnosticsStore.clearError),
        }]
      : [],
  );

  async function handleExport(): Promise<void> {
    await diagnosticsStore.exportBundle();
  }

  async function handleClear(): Promise<void> {
    await diagnosticsStore.clearLogs();
  }
</script>

<div class="settings-fields">
  <SettingsRow
    label={i18n.t('settings.diagnostics.sessionsLabel')}
    help={i18n.t('settings.diagnostics.sessionsHelp')}
    helperText={i18n.t('settings.diagnostics.sessionsHelper')}
  >
    <p class="counter-value mono">{diagnosticsStore.summary?.sessions ?? 0}</p>
  </SettingsRow>

  <SettingsRow
    label={i18n.t('settings.diagnostics.crashesLabel')}
    help={i18n.t('settings.diagnostics.crashesHelp')}
    helperText={i18n.t('settings.diagnostics.crashesHelper')}
  >
    <p class="counter-value mono">{diagnosticsStore.summary?.crashes ?? 0}</p>
  </SettingsRow>

  <SettingsRow
    label={i18n.t('settings.diagnostics.errorsLabel')}
    help={i18n.t('settings.diagnostics.errorsHelp')}
    helperText={i18n.t('settings.diagnostics.errorsHelper')}
  >
    <dl class="error-counters">
      {#each (diagnosticsStore.summary?.errorsByCategory ?? []) as row (row.category)}
        <div class="error-counter-row">
          <dt>{i18n.t(CATEGORY_TITLE_KEY[row.category])}</dt>
          <dd class="mono">{row.count}</dd>
        </div>
      {/each}
    </dl>
    <BannerStack banners={summaryBanners} />
  </SettingsRow>

  <SettingsRow
    label={i18n.t('settings.diagnostics.exportLabel')}
    help={i18n.t('settings.diagnostics.exportHelp')}
    helperText={i18n.t('settings.diagnostics.exportHelper')}
  >
    <button
      type="button"
      disabled={diagnosticsStore.exportStatus === 'running'}
      aria-busy={diagnosticsStore.exportStatus === 'running'}
      onclick={() => void handleExport()}
    >
      {diagnosticsStore.exportStatus === 'running'
        ? i18n.t('settings.diagnostics.exportBusy')
        : i18n.t('settings.diagnostics.exportButton')}
    </button>
    <div class="action-status" role="status" aria-live="polite">
      {#if diagnosticsStore.exportStatus === 'done' && diagnosticsStore.lastExportSaved === true}
        <p>{i18n.t('settings.diagnostics.exportSuccess')}</p>
      {:else if diagnosticsStore.exportStatus === 'done' && diagnosticsStore.lastExportSaved === false}
        <p>{i18n.t('settings.diagnostics.exportCancelled')}</p>
      {/if}
    </div>
    <BannerStack banners={exportBanners} />
  </SettingsRow>

  <SettingsRow
    label={i18n.t('settings.diagnostics.clearLabel')}
    help={i18n.t('settings.diagnostics.clearHelp')}
    helperText={i18n.t('settings.diagnostics.clearHelper')}
  >
    <button
      type="button"
      disabled={diagnosticsStore.clearStatus === 'running'}
      aria-busy={diagnosticsStore.clearStatus === 'running'}
      onclick={() => void handleClear()}
    >
      {diagnosticsStore.clearStatus === 'running'
        ? i18n.t('settings.diagnostics.clearBusy')
        : i18n.t('settings.diagnostics.clearButton')}
    </button>
    <div class="action-status" role="status" aria-live="polite">
      {#if diagnosticsStore.clearStatus === 'done'}
        <p>{i18n.t('settings.diagnostics.clearSuccess')}</p>
      {/if}
    </div>
    <BannerStack banners={clearBanners} />
  </SettingsRow>
</div>

<style>
  .settings-fields {
    display: grid;
  }

  .counter-value {
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

  .error-counters {
    display: grid;
    gap: var(--space-2);
    margin: 0;
  }

  .error-counter-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    min-height: 28px;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
  }

  .error-counter-row dt {
    color: var(--color-text-secondary);
    font-size: var(--text-help-size);
  }

  .error-counter-row dd {
    margin: 0;
    color: var(--color-text);
    font-size: var(--text-help-size);
    font-variant-numeric: tabular-nums;
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
    min-height: 36px;
    width: fit-content;
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
</style>
