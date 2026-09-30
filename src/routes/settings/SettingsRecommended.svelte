<script lang="ts">
  import { commands, type AppError, type RecommendedSettingField, type RecommendedSettingsPreview } from '../../lib/bindings';
  import { errorTitle } from '../../lib/errors';
  import { i18n, type TranslationKey } from '../../i18n/index.svelte';

  type BusyAction = 'preview' | 'apply' | 'cancel' | null;

  const settingLabels: Record<RecommendedSettingField, TranslationKey> = {
    transcribeModel: 'settings.recommended.transcribeModel',
    liveModel: 'settings.recommended.liveModel',
    memoModel: 'settings.recommended.memoModel',
    chunkMinutes: 'settings.recommended.chunkMinutes',
  };

  const localeLabels: Record<string, TranslationKey> = {
    vi: 'settings.recommendedLocale.vi',
    en: 'settings.recommendedLocale.en',
    ja: 'settings.recommendedLocale.ja',
  };

  let preview = $state<RecommendedSettingsPreview | null>(null);
  let busy = $state<BusyAction>(null);
  let error = $state<AppError | null>(null);
  let stale = $state(false);
  let statusMessage = $state('');

  const hasChanges = $derived(Boolean(preview && (preview.changes.length > 0 || preview.templateChanges.length > 0)));

  async function fetchPreview(): Promise<void> {
    busy = 'preview';
    error = null;
    stale = false;
    statusMessage = '';
    try {
      const result = await commands.settingsRecommendedPreview();
      if (result.status === 'error') {
        error = result.error;
        return;
      }
      preview = result.data;
    } catch {
      error = transportError();
    } finally {
      busy = null;
    }
  }

  async function applyPreview(): Promise<void> {
    if (!preview || !hasChanges) return;
    busy = 'apply';
    error = null;
    stale = false;
    statusMessage = '';
    try {
      const result = await commands.settingsRecommendedApply(preview.token);
      if (result.status === 'error') {
        error = result.error;
        return;
      }
      if (result.data.stale) {
        preview = result.data.preview;
        stale = true;
        return;
      }
      preview = null;
      statusMessage = i18n.t('settings.recommended.applied', { count: result.data.appliedCount });
    } catch {
      error = transportError();
    } finally {
      busy = null;
    }
  }

  async function cancelPreview(): Promise<void> {
    if (!preview) return;
    const token = preview.token;
    busy = 'cancel';
    error = null;
    stale = false;
    statusMessage = '';
    try {
      const result = await commands.settingsRecommendedCancel(token);
      if (result.status === 'error') error = result.error;
    } catch {
      error = transportError();
    } finally {
      // Remove the displayed plan even when IPC fails. A later preview
      // replaces any remaining process-local token before it can be applied.
      preview = null;
      busy = null;
      statusMessage = i18n.t('settings.recommended.cancelled');
    }
  }

  function transportError(): AppError {
    return {
      category: 'network',
      code: 'network',
      detailRedacted: '',
    };
  }
</script>

<p class="description">{i18n.t('settings.recommended.description')}</p>

<div class="actions">
  <button type="button" class="primary" disabled={busy !== null} onclick={fetchPreview}>
    {busy === 'preview' ? i18n.t('settings.recommended.fetching') : i18n.t('settings.recommended.fetch')}
  </button>
</div>

{#if error}
  <div class="notice error" role="alert">
    <strong>{errorTitle(error)}</strong>
    <p>{i18n.t('settings.recommended.errorBody')}</p>
  </div>
{/if}

{#if stale}
  <div class="notice stale" role="status" aria-live="polite">
    <strong>{i18n.t('settings.recommended.staleTitle')}</strong>
    <p>{i18n.t('settings.recommended.staleBody')}</p>
  </div>
{/if}

{#if statusMessage}
  <p class="status" role="status" aria-live="polite">{statusMessage}</p>
{/if}

{#if preview}
  <section class="preview" aria-labelledby="recommended-preview-title">
    <h3 id="recommended-preview-title">{i18n.t('settings.recommended.previewTitle')}</h3>

    {#if preview.changes.length > 0}
      <h4>{i18n.t('settings.recommended.settingsHeading')}</h4>
      <ul class="diff-list">
        {#each preview.changes as change (change.field)}
          <li>
            <strong>{i18n.t(settingLabels[change.field])}</strong>
            <div class="diff-values">
              <span>{change.currentValue}</span>
              <span aria-hidden="true">→</span>
              <span>{change.proposedValue}</span>
            </div>
          </li>
        {/each}
      </ul>
    {/if}

    {#if preview.templateChanges.length > 0}
      <h4>{i18n.t('settings.recommended.templatesHeading')}</h4>
      <ul class="diff-list">
        {#each preview.templateChanges as change (`${change.locale}:${change.externalId}`)}
          <li class="template-diff">
            <strong>{i18n.t(localeLabels[change.locale])} · {change.externalId}</strong>
            {#if change.currentName !== change.proposedName}
              <div class="diff-values">
                <span>{change.currentName ?? i18n.t('settings.recommended.noTemplate')}</span>
                <span aria-hidden="true">→</span>
                <span>{change.proposedName}</span>
              </div>
            {/if}
            {#if change.currentPrompt !== change.proposedPrompt}
              <details>
                <summary>{i18n.t('settings.recommended.promptChanges')}</summary>
                <div class="prompt-diff">
                  <div>
                    <span class="value-label">{i18n.t('settings.recommended.current')}</span>
                    <pre>{change.currentPrompt ?? i18n.t('settings.recommended.noTemplate')}</pre>
                  </div>
                  <div>
                    <span class="value-label">{i18n.t('settings.recommended.proposed')}</span>
                    <pre>{change.proposedPrompt}</pre>
                  </div>
                </div>
              </details>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}

    {#if preview.templateConflicts.length > 0}
      <div class="conflicts" role="status">
        <h4>{i18n.t('settings.recommended.conflictsHeading')}</h4>
        <p>{i18n.t('settings.recommended.conflictsBody')}</p>
        <ul>
          {#each preview.templateConflicts as conflict (`${conflict.locale}:${conflict.externalId}`)}
            <li>{i18n.t(localeLabels[conflict.locale])} · {conflict.externalId}</li>
          {/each}
        </ul>
      </div>
    {/if}

    {#if !hasChanges}
      <p class="empty">{i18n.t('settings.recommended.noChanges')}</p>
    {/if}

    <div class="actions preview-actions">
      <button type="button" class="primary" disabled={busy !== null || !hasChanges} onclick={applyPreview}>
        {busy === 'apply' ? i18n.t('settings.recommended.applying') : i18n.t('settings.recommended.apply')}
      </button>
      <button type="button" disabled={busy !== null} onclick={cancelPreview}>
        {busy === 'cancel' ? i18n.t('settings.recommended.cancelling') : i18n.t('settings.recommended.cancel')}
      </button>
    </div>
  </section>
{/if}

<style>
  .description { margin: 0 0 var(--space-4); color: var(--color-text-secondary); }
  .actions { display: flex; flex-wrap: wrap; gap: var(--space-2); }
  button { min-height: 36px; padding: 7px 13px; border: 1px solid var(--color-border); border-radius: var(--radius-md); color: var(--color-text); background: var(--color-surface); font: inherit; font-weight: 600; cursor: pointer; }
  button:hover:not(:disabled) { background: var(--color-surface-sunken); }
  button:disabled { opacity: 0.55; cursor: not-allowed; }
  button.primary { border-color: var(--color-primary-action); color: var(--color-on-primary); background: var(--color-primary-action); }
  button.primary:hover:not(:disabled) { filter: brightness(0.94); }
  .preview { margin-top: var(--space-5); padding-top: var(--space-4); border-top: 1px solid var(--color-border); }
  h3 { margin: 0 0 var(--space-3); font-size: var(--text-h3-size); }
  h4 { margin: var(--space-4) 0 var(--space-2); font-size: var(--text-body-size); }
  .diff-list { display: grid; gap: var(--space-2); margin: 0; padding: 0; list-style: none; }
  .diff-list li { padding: var(--space-3); border: 1px solid var(--color-border); border-radius: var(--radius-md); background: var(--color-surface-sunken); }
  .diff-values { display: grid; grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr); gap: var(--space-2); align-items: start; margin-top: var(--space-2); overflow-wrap: anywhere; }
  .template-diff details { margin-top: var(--space-2); }
  summary { color: var(--color-accent); cursor: pointer; }
  .prompt-diff { display: grid; gap: var(--space-3); margin-top: var(--space-2); }
  .value-label { color: var(--color-text-muted); font-size: var(--text-help-size); font-weight: 600; }
  pre { max-height: 220px; margin: var(--space-1) 0 0; padding: var(--space-2); overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; border-radius: var(--radius-sm); background: var(--color-surface); font: inherit; font-size: var(--text-help-size); }
  .conflicts, .notice { margin-top: var(--space-4); padding: var(--space-3); border-radius: var(--radius-md); }
  .conflicts { border: 1px solid var(--color-warning-border); background: var(--color-warning-soft); }
  .conflicts p, .notice p { margin: var(--space-1) 0 0; }
  .conflicts ul { margin: var(--space-2) 0 0; padding-left: 1.25rem; }
  .notice.error { border: 1px solid var(--color-danger-border); background: var(--color-danger-soft); }
  .notice.stale { border: 1px solid var(--color-warning-border); background: var(--color-warning-soft); }
  .status, .empty { margin: var(--space-3) 0 0; }
  .status { color: var(--color-accent-hover); }
  .empty { color: var(--color-text-secondary); }
  .preview-actions { margin-top: var(--space-4); }
  @media (min-width: 760px) { .prompt-diff { grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); } }
</style>
