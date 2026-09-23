<script lang="ts">
  // Settings → Gemini: the API key (paste/check/reveal + stored-key list with
  // delete) and the three free-text model fields (transcribe/live/memo).
  // Reuses `keysStore.checkKeys` verbatim (same flow as Onboarding's apiKey
  // step) and `settingsStore.setModel` for persistence — no new IPC command
  // is added here (spec Boundaries: "Chỉ dùng command đã có").
  import { onMount } from 'svelte';
  import SettingsRow from '../../components/SettingsRow.svelte';
  import BannerStack from '../../components/BannerStack.svelte';
  import { i18n, type TranslationKey } from '../../i18n/index.svelte';
  import { keysStore, type ApiKeyCheckOutcome } from '../../lib/stores/keys.svelte';
  import { settingsStore } from '../../lib/stores/settings.svelte';
  import { errorHint, errorTitle, modelErrorBanner } from '../../lib/errors';
  import { AlertCircleIcon, CircleCheckIcon, EyeIcon, EyeOffIcon } from '../../components/icons';
  import type { KeyId, ModelKind, TranscribeLanguage } from '../../lib/bindings';

  onMount(() => {
    void keysStore.load();
  });

  let apiKeyInput = $state('');
  let revealed = $state(false);
  let checking = $state(false);
  let checkOutcome = $state<ApiKeyCheckOutcome | null>(null);
  let deletingId = $state<KeyId | null>(null);

  async function checkApiKey(): Promise<void> {
    if (checking) return;
    checking = true;
    try {
      checkOutcome = await keysStore.checkKeys(apiKeyInput);
    } finally {
      checking = false;
    }
  }

  async function removeKey(id: KeyId): Promise<void> {
    if (deletingId) return;
    deletingId = id;
    try {
      await keysStore.deleteKey(id);
    } finally {
      deletingId = null;
    }
  }

  function formatCheckedAt(date: Date): string {
    return new Intl.DateTimeFormat(undefined, {
      hour: '2-digit',
      minute: '2-digit',
      hourCycle: 'h23',
    }).format(date);
  }

  type ModelRow = {
    kind: ModelKind;
    labelKey: TranslationKey;
    helpKey: TranslationKey;
    helperKey: TranslationKey;
  };

  const MODEL_ROWS: ModelRow[] = [
    {
      kind: 'transcribe',
      labelKey: 'settings.gemini.transcribeLabel',
      helpKey: 'settings.gemini.transcribeHelp',
      helperKey: 'settings.gemini.transcribeHelper',
    },
    {
      kind: 'live',
      labelKey: 'settings.gemini.liveLabel',
      helpKey: 'settings.gemini.liveHelp',
      helperKey: 'settings.gemini.liveHelper',
    },
    {
      kind: 'memo',
      labelKey: 'settings.gemini.memoLabel',
      helpKey: 'settings.gemini.memoHelp',
      helperKey: 'settings.gemini.memoHelper',
    },
  ];

  // Inline "rỗng bị chặn ngay khi nhập" state, keyed by kind — never sent to
  // `setModel` while true (spec Always).
  let inlineErrors = $state<Record<ModelKind, boolean>>({
    transcribe: false,
    live: false,
    memo: false,
  });

  function currentModelValue(kind: ModelKind): string {
    if (kind === 'transcribe') return settingsStore.transcribeModel;
    if (kind === 'live') return settingsStore.liveModel;
    return settingsStore.memoModel;
  }

  function handleModelInput(kind: ModelKind, event: Event): void {
    const value = (event.currentTarget as HTMLInputElement).value;
    inlineErrors = { ...inlineErrors, [kind]: value.trim().length === 0 };
  }

  function commitModel(kind: ModelKind, event: Event): void {
    const value = (event.currentTarget as HTMLInputElement).value;
    const trimmed = value.trim();
    if (!trimmed) {
      inlineErrors = { ...inlineErrors, [kind]: true };
      return;
    }
    inlineErrors = { ...inlineErrors, [kind]: false };
    void settingsStore.setModel(kind, trimmed);
  }

  function changeTranscribeLanguage(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value as TranscribeLanguage;
    void settingsStore.setTranscribeLanguage(value);
  }
</script>

<div class="settings-fields">
  <SettingsRow
    label={i18n.t('settings.gemini.keyLabel')}
    help={i18n.t('settings.gemini.keyHelp')}
    helperText={i18n.t('settings.gemini.keyHelper')}
    fieldId="settings-gemini-key"
  >
    <form
      class="key-input-row"
      onsubmit={(event) => {
        event.preventDefault();
        void checkApiKey();
      }}
    >
      <input
        id="settings-gemini-key"
        name="gemini-key"
        type={revealed ? 'text' : 'password'}
        autocomplete="off"
        spellcheck="false"
        placeholder={i18n.t('settings.gemini.keyPlaceholder')}
        bind:value={apiKeyInput}
      />
      <button
        type="button"
        class="reveal-toggle"
        aria-pressed={revealed}
        aria-label={revealed ? i18n.t('settings.gemini.keyHide') : i18n.t('settings.gemini.keyReveal')}
        onclick={() => (revealed = !revealed)}
      >
        {#if revealed}
          <EyeOffIcon size={18} strokeWidth={1.75} aria-hidden="true" />
        {:else}
          <EyeIcon size={18} strokeWidth={1.75} aria-hidden="true" />
        {/if}
      </button>
      <button type="submit" disabled={checking}>{i18n.t('settings.gemini.checkKey')}</button>
    </form>

    <div class="key-status-region" role="status" aria-live="polite">
      {#if checking}
        <p class="key-status">{i18n.t('settings.gemini.checking')}</p>
      {:else if checkOutcome}
        {@const checkedAt = keysStore.lastCheckedAt}
        <div class="key-status">
          {#if checkOutcome.kind === 'success'}
            <CircleCheckIcon size={18} strokeWidth={1.75} aria-hidden="true" />
            <span>
              {checkOutcome.modelCount !== null
                ? i18n.t('settings.gemini.statusValid', { count: checkOutcome.modelCount })
                : i18n.t('settings.gemini.statusValidUnknown')}
              {#if checkOutcome.rejectedCount > 0}
                {' '}{i18n.t('settings.gemini.statusRejected', { count: checkOutcome.rejectedCount })}
              {/if}
              {#if checkedAt}<span class="checked-at"> · {formatCheckedAt(checkedAt)}</span>{/if}
            </span>
          {:else}
            <AlertCircleIcon size={18} strokeWidth={1.75} aria-hidden="true" />
            <span>
              <strong>{errorTitle(checkOutcome.error)}</strong> {errorHint(checkOutcome.error)}
              {#if checkedAt}<span class="checked-at"> · {formatCheckedAt(checkedAt)}</span>{/if}
            </span>
          {/if}
        </div>
      {/if}
    </div>
  </SettingsRow>

  <SettingsRow
    label={i18n.t('settings.gemini.keyListTitle')}
    help={i18n.t('settings.gemini.keyListHelper')}
    helperText={i18n.t('settings.gemini.keyListHelper')}
  >
    {#if keysStore.keys.length === 0}
      <p class="key-list-empty">{i18n.t('settings.gemini.keyListEmpty')}</p>
    {:else}
      <ul class="key-list">
        {#each keysStore.keys as storedKey (storedKey.id)}
          <li>
            <span class="key-chip">{storedKey.label}</span>
            <button
              type="button"
              disabled={deletingId === storedKey.id}
              aria-label={i18n.t('settings.gemini.keyDeleteLabel', { label: storedKey.label })}
              onclick={() => void removeKey(storedKey.id)}
            >
              {i18n.t('settings.gemini.keyDelete')}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    {#if keysStore.error}
      <BannerStack banners={[modelErrorBanner(keysStore.error, 'key-delete')]} />
    {/if}
  </SettingsRow>

  {#each MODEL_ROWS as row (row.kind)}
    {@const options = keysStore.modelLists[row.kind]}
    {@const loadStatus = keysStore.modelListStatus[row.kind]}
    {@const loadError = keysStore.modelListError[row.kind]}
    {@const value = currentModelValue(row.kind)}
    {@const inlineError = inlineErrors[row.kind]}
    {@const unlisted = !inlineError && options.length > 0 && !options.some((option) => option.name === value)}
    <SettingsRow
      label={i18n.t(row.labelKey)}
      help={i18n.t(row.helpKey)}
      helperText={i18n.t(row.helperKey)}
      fieldId={`settings-gemini-model-${row.kind}`}
    >
      <div class="model-input-row">
        <input
          id={`settings-gemini-model-${row.kind}`}
          list={`settings-gemini-model-${row.kind}-options`}
          value={value}
          placeholder={i18n.t('settings.gemini.modelPlaceholder')}
          aria-invalid={inlineError ? 'true' : undefined}
          oninput={(event) => handleModelInput(row.kind, event)}
          onchange={(event) => commitModel(row.kind, event)}
        />
        <datalist id={`settings-gemini-model-${row.kind}-options`}>
          {#each options as option (option.name)}
            <option value={option.name}>{option.displayName}</option>
          {/each}
        </datalist>
        <button
          type="button"
          disabled={loadStatus === 'loading'}
          aria-busy={loadStatus === 'loading'}
          onclick={() => void keysStore.loadModelList(row.kind)}
        >
          {loadStatus === 'loading' ? i18n.t('settings.gemini.modelLoading') : i18n.t('settings.gemini.modelLoad')}
        </button>
      </div>

      {#if inlineError}
        <p class="inline-error" role="alert">{i18n.t('settings.gemini.modelEmpty')}</p>
      {:else if unlisted}
        <p class="soft-warning">{i18n.t('settings.gemini.modelUnlisted')}</p>
      {/if}

      {#if loadError}
        <BannerStack banners={[modelErrorBanner(loadError, row.kind)]} />
      {/if}
    </SettingsRow>
  {/each}

  <SettingsRow
    label={i18n.t('settings.gemini.languageLabel')}
    help={i18n.t('settings.gemini.languageHelp')}
    helperText={i18n.t('settings.gemini.languageHelper')}
    fieldId="settings-gemini-transcribe-language"
  >
    <select
      id="settings-gemini-transcribe-language"
      value={settingsStore.transcribeLanguage}
      onchange={changeTranscribeLanguage}
    >
      <option value="auto">{i18n.t('settings.gemini.languageAuto')}</option>
      <option value="ja">{i18n.t('settings.gemini.languageJa')}</option>
      <option value="vi">{i18n.t('settings.gemini.languageVi')}</option>
      <option value="en">{i18n.t('settings.gemini.languageEn')}</option>
    </select>
  </SettingsRow>
</div>

<style>
  .settings-fields {
    display: grid;
  }

  .key-input-row,
  .model-input-row {
    display: flex;
    align-items: stretch;
    gap: var(--space-2);
  }

  .settings-fields select {
    min-height: 36px;
    max-width: 280px;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    cursor: pointer;
  }

  .key-input-row input,
  .model-input-row input {
    min-height: 36px;
    flex: 1;
    min-width: 0;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font-family: var(--font-mono);
  }

  .key-input-row input[type='password'],
  .key-input-row input[type='text'] {
    letter-spacing: 0.02em;
  }

  .reveal-toggle {
    display: grid;
    width: 36px;
    height: 36px;
    flex: 0 0 auto;
    place-items: center;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text-secondary);
    cursor: pointer;
  }

  .key-input-row button[type='submit'],
  .model-input-row button {
    min-height: 36px;
    flex: 0 0 auto;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    cursor: pointer;
    white-space: nowrap;
  }

  .key-input-row button[disabled],
  .model-input-row button[disabled] {
    cursor: not-allowed;
    opacity: 0.6;
  }

  .key-status-region {
    min-height: 20px;
  }

  .key-status {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    color: var(--color-text-secondary);
    font-size: var(--text-help-size);
  }

  .checked-at {
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .key-list-empty {
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
  }

  .key-list {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .key-list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    min-height: 36px;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
  }

  .key-chip {
    min-width: 0;
    overflow: hidden;
    color: var(--color-text-secondary);
    font-family: var(--font-mono);
    font-size: var(--text-help-size);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .key-list li button {
    flex: 0 0 auto;
    min-height: 28px;
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-danger);
    cursor: pointer;
    font-size: var(--text-help-size);
  }

  .key-list li button[disabled] {
    cursor: not-allowed;
    opacity: 0.6;
  }

  .inline-error {
    margin: 0;
    color: var(--color-danger);
    font-size: var(--text-help-size);
  }

  .soft-warning {
    margin: 0;
    color: var(--color-warning);
    font-size: var(--text-help-size);
  }
</style>
