<script lang="ts">
  import { replace } from '@keenmate/svelte-spa-router';
  import { i18n, type Locale, type TranslationKey } from '../i18n/index.svelte';
  import { settingsStore } from '../lib/stores/settings.svelte';
  import { keysStore, type ApiKeyCheckOutcome } from '../lib/stores/keys.svelte';
  import { errorHint, errorTitle } from '../lib/errors';
  import DisabledHint from '../components/DisabledHint.svelte';
  import ConsentText from '../components/ConsentText.svelte';
  import {
    AlertCircleIcon,
    CircleCheckIcon,
    EyeIcon,
    EyeOffIcon,
  } from '../components/icons';

  type Stage = 'language' | 'consent' | 'apiKey';
  let stage = $state<Stage>('language');
  let consentError = $state(false);
  let saving = $state(false);

  let apiKeyInput = $state('');
  let revealed = $state(false);
  let checking = $state(false);
  let checkOutcome = $state<ApiKeyCheckOutcome | null>(null);
  let hasUsableKey = $state(false);
  let finishing = $state(false);

  $effect(() => {
    const status = settingsStore.consentStatus;
    if (status === 'declined' || status === 'stale') stage = 'consent';
    if (status === 'current') stage = 'apiKey';
  });

  $effect(() => {
    // Existing keys from an earlier run of this session count as "present"
    // without a background network test (spec Always/Design Notes).
    if (stage === 'apiKey') void refreshHasUsableKey();
  });

  async function refreshHasUsableKey(): Promise<void> {
    await keysStore.load();
    hasUsableKey = keysStore.hasUsableKey;
  }

  async function checkApiKey(): Promise<void> {
    if (checking) return;
    checking = true;
    checkOutcome = null;
    try {
      checkOutcome = await keysStore.checkKeys(apiKeyInput);
      hasUsableKey = keysStore.hasUsableKey;
    } finally {
      checking = false;
    }
  }

  async function finishOnboarding(): Promise<void> {
    if (finishing) return;
    finishing = true;
    try {
      await settingsStore.setOnboardingCompleted(true);
      await replace('/home');
    } finally {
      finishing = false;
    }
  }

  const languages: Array<{
    value: Locale;
    nameKey: TranslationKey;
    descriptionKey: TranslationKey;
  }> = [
    { value: 'vi', nameKey: 'onboarding.language.viName', descriptionKey: 'onboarding.language.viDescription' },
    { value: 'en', nameKey: 'onboarding.language.enName', descriptionKey: 'onboarding.language.enDescription' },
    { value: 'ja', nameKey: 'onboarding.language.jaName', descriptionKey: 'onboarding.language.jaDescription' },
  ];

  const selectedLocale = $derived(
    settingsStore.uiLanguage === 'system' ? i18n.systemLocale : settingsStore.uiLanguage,
  );

  function chooseLanguage(event: Event): void {
    const value = (event.currentTarget as HTMLInputElement).value as Locale;
    void settingsStore.setUiLanguage(value);
  }

  async function continueFromLanguage(): Promise<void> {
    stage = 'consent';
  }

  async function acceptConsent(): Promise<void> {
    if (saving) return;
    saving = true;
    consentError = false;
    try {
      await settingsStore.acceptConsent();
      if (settingsStore.error) throw new Error('consent save failed');
      stage = 'apiKey';
    } catch {
      consentError = true;
    } finally {
      saving = false;
    }
  }

  async function declineConsent(): Promise<void> {
    if (saving) return;
    saving = true;
    consentError = false;
    try {
      await settingsStore.declineConsent();
      if (settingsStore.error) throw new Error('consent save failed');
      window.history.replaceState({}, '', '/settings/about');
      window.dispatchEvent(new PopStateEvent('popstate'));
    } catch {
      consentError = true;
    } finally {
      saving = false;
    }
  }

  function onConsentPrivacyError(): void {
    consentError = true;
  }
</script>

<svelte:head>
  <title>{i18n.t('app.meta.onboardingTitle')}</title>
</svelte:head>

<section class="route-screen onboarding-screen" aria-labelledby="onboarding-title">
  <div class="onboarding-card">
    <p class="route-kicker">{stage === 'language' ? i18n.t('onboarding.header.kicker') : stage === 'consent' ? i18n.t('onboarding.consent.kicker') : i18n.t('onboarding.apiKey.kicker')}</p>
    <h1 id="onboarding-title">{stage === 'language' ? i18n.t('onboarding.header.title') : stage === 'consent' ? i18n.t('onboarding.consent.title') : i18n.t('onboarding.apiKey.title')}</h1>
    <p class="route-lede">
      {stage === 'language' ? i18n.t('onboarding.header.description') : stage === 'consent' ? i18n.t('onboarding.consent.description') : i18n.t('onboarding.apiKey.description')}
    </p>

    <ol class="stepper" aria-label={i18n.t('onboarding.stepper.label')}>
      <li class:step-current={stage === 'language'} class="step" aria-current={stage === 'language' ? 'step' : undefined}><span>1</span> {i18n.t('onboarding.stepper.language')}</li>
      <li class:step-current={stage === 'consent'} class="step" aria-current={stage === 'consent' ? 'step' : undefined}><span>2</span> {i18n.t('onboarding.stepper.data')}</li>
      <li class:step-current={stage === 'apiKey'} class="step" aria-current={stage === 'apiKey' ? 'step' : undefined}><span>3</span> {i18n.t('onboarding.stepper.apiKey')}</li>
    </ol>

    {#if stage === 'language'}
    <fieldset class="language-picker">
      <legend>{i18n.t('onboarding.language.legend')}</legend>
      {#each languages as language}
        <label class:selected={selectedLocale === language.value} class="language-card">
          <input
            type="radio"
            name="ui-language"
            value={language.value}
            checked={selectedLocale === language.value}
            onchange={chooseLanguage}
          />
          <span class="language-copy">
            <strong>{i18n.t(language.nameKey)}</strong>
            <span>{i18n.t(language.descriptionKey)}</span>
          </span>
          {#if i18n.systemLocale === language.value}
            <span class="system-badge">{i18n.t('onboarding.language.systemBadge')}</span>
          {/if}
        </label>
      {/each}
    </fieldset>

    <div class="actions">
      <button
        type="button"
        onclick={() => void continueFromLanguage()}
      >
        {i18n.t('onboarding.action.continue')}
      </button>
    </div>
    {:else if stage === 'consent'}
      <ConsentText
        version={settingsStore.consentPolicy?.currentVersion ?? 1}
        privacyUrl={settingsStore.consentPolicy?.privacyUrl}
        onPrivacyError={onConsentPrivacyError}
      />
      {#if consentError}<p class="consent-error" role="alert">{i18n.t('onboarding.consent.error')}</p>{/if}
      <div class="actions consent-actions">
        <button class="ghost" type="button" disabled={saving} onclick={() => void declineConsent()}>{i18n.t('onboarding.consent.decline')}</button>
        <button type="button" disabled={saving} onclick={() => void acceptConsent()}>{i18n.t('onboarding.consent.accept')}</button>
      </div>
    {:else}
      <form
        class="api-key-form"
        onsubmit={(event) => {
          event.preventDefault();
          void checkApiKey();
        }}
      >
        <label class="field-label" for="api-key-input">{i18n.t('onboarding.apiKey.label')}</label>
        <p class="field-help">{i18n.t('onboarding.apiKey.help')}</p>
        <div class="key-input-row">
          <input
            id="api-key-input"
            name="api-key"
            type={revealed ? 'text' : 'password'}
            autocomplete="off"
            spellcheck="false"
            placeholder={i18n.t('onboarding.apiKey.placeholder')}
            bind:value={apiKeyInput}
          />
          <button
            class="reveal-toggle"
            type="button"
            aria-pressed={revealed}
            aria-label={revealed ? i18n.t('onboarding.apiKey.hide') : i18n.t('onboarding.apiKey.reveal')}
            onclick={() => (revealed = !revealed)}
          >
            {#if revealed}
              <EyeOffIcon size={18} strokeWidth={1.75} aria-hidden="true" />
            {:else}
              <EyeIcon size={18} strokeWidth={1.75} aria-hidden="true" />
            {/if}
          </button>
        </div>

        <div class="key-status-region" role="status" aria-live="polite">
        {#if checking}
          <p class="key-status">{i18n.t('onboarding.apiKey.checking')}</p>
        {:else if checkOutcome}
          <div class="key-status">
            {#if checkOutcome.kind === 'success'}
              <CircleCheckIcon size={18} strokeWidth={1.75} aria-hidden="true" />
              <span>
                {checkOutcome.modelCount !== null
                  ? i18n.t('onboarding.apiKey.statusValid', { count: checkOutcome.modelCount })
                  : i18n.t('onboarding.apiKey.statusValidUnknown')}
                {#if checkOutcome.rejectedCount > 0}
                  {i18n.t('onboarding.apiKey.statusRejected', { count: checkOutcome.rejectedCount })}
                {/if}
              </span>
            {:else}
              <AlertCircleIcon size={18} strokeWidth={1.75} aria-hidden="true" />
              <span><strong>{errorTitle(checkOutcome.error)}</strong> {errorHint(checkOutcome.error)}</span>
            {/if}
          </div>
        {/if}
        </div>

        <div class="actions api-key-actions">
          <button class="ghost" type="button" disabled={finishing} onclick={() => void finishOnboarding()}>
            {i18n.t('onboarding.apiKey.skip')}
          </button>
          <div class="api-key-primary-actions">
            <button type="submit" disabled={checking}>{i18n.t('onboarding.apiKey.check')}</button>
            {#if hasUsableKey}
              <button type="button" disabled={finishing} onclick={() => void finishOnboarding()}>
                {i18n.t('onboarding.apiKey.continue')}
              </button>
            {:else}
              <DisabledHint
                reason={i18n.t('onboarding.apiKey.continueDisabled')}
                shortcut={i18n.t('onboarding.apiKey.continueShortcut')}
              >
                <span class="continue-look">{i18n.t('onboarding.apiKey.continue')}</span>
              </DisabledHint>
            {/if}
          </div>
        </div>
      </form>
    {/if}
  </div>
</section>

<style>
  .route-screen {
    display: grid;
    min-height: 100%;
    padding: var(--space-8);
    place-items: start center;
  }

  .onboarding-card {
    width: min(100%, 600px);
    padding: var(--space-8);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-2xl);
    background: var(--color-surface);
  }

  .route-kicker {
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  h1 {
    margin: var(--space-2) 0 var(--space-3);
    font-size: var(--text-h1-size);
    line-height: 1.3;
  }

  .route-lede {
    margin: 0;
    color: var(--color-text-secondary);
  }

  .stepper {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin: var(--space-6) 0;
    padding: 0;
    list-style: none;
  }

  .step {
    display: inline-flex;
    min-width: max-content;
    min-height: 32px;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-full);
    color: var(--color-text-muted);
    font-size: var(--text-label-size);
  }

  .step > span {
    display: grid;
    width: 20px;
    height: 20px;
    place-items: center;
    border-radius: var(--radius-full);
    background: var(--color-surface-sunken);
    font-size: var(--text-help-size);
    font-weight: 600;
  }

  .step-current {
    border-color: var(--color-accent-border);
    background: var(--color-accent-soft);
    color: var(--color-text);
  }

  .step-current > span {
    background: var(--color-accent);
    color: var(--color-surface);
  }

  .language-picker {
    display: grid;
    gap: var(--space-3);
    padding: 0;
    border: 0;
    margin: 0;
  }

  .language-picker legend {
    margin-bottom: var(--space-3);
    font-size: var(--text-label-size);
    font-weight: 600;
  }

  .language-card {
    display: flex;
    min-width: 0;
    min-height: 68px;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-lg);
    cursor: pointer;
  }

  .language-card:hover {
    background: var(--color-surface-sunken);
  }

  .language-card.selected {
    border-color: var(--color-accent-border);
    background: var(--color-accent-soft);
  }

  .language-card input {
    width: 18px;
    height: 18px;
    accent-color: var(--color-accent);
  }

  .language-copy {
    display: grid;
    min-width: 0;
    flex: 1;
    gap: 2px;
  }

  .language-copy strong {
    font-size: var(--text-option-size);
  }

  .language-copy > span {
    color: var(--color-text-secondary);
    font-size: var(--text-help-size);
  }

  .system-badge {
    min-width: max-content;
    min-height: 24px;
    padding: 3px var(--space-2);
    border: 1px solid var(--color-accent-border);
    border-radius: var(--radius-full);
    color: var(--color-accent);
    font-size: var(--text-badge-size);
    font-weight: 600;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: var(--space-6);
  }

  .actions button {
    min-width: 112px;
    min-height: 36px;
    padding: 0 var(--space-4);
    border: 0;
    border-radius: var(--radius-md);
    background: var(--color-primary-action);
    color: var(--color-on-primary);
    font-weight: 500;
  }

  .consent-error { color: var(--color-danger, #b42318); font-size: var(--text-help-size); }
  .consent-actions { justify-content: space-between; }
  .consent-actions .ghost,
  .api-key-actions .ghost { border: 1px solid var(--color-border-strong); background: transparent; color: var(--color-text); }

  .api-key-form {
    margin-top: var(--space-6);
  }

  .field-label {
    display: block;
    margin-bottom: var(--space-1);
    font-size: var(--text-label-size);
    font-weight: 600;
  }

  .field-help {
    margin: 0 0 var(--space-3);
    color: var(--color-text-secondary);
    font-size: var(--text-help-size);
  }

  .key-input-row {
    display: flex;
    align-items: stretch;
    gap: var(--space-2);
  }

  .key-input-row input {
    min-height: 40px;
    flex: 1;
    min-width: 0;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font-family: var(--font-mono);
  }

  .reveal-toggle {
    display: grid;
    width: 40px;
    height: 40px;
    flex: 0 0 auto;
    place-items: center;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text-secondary);
    cursor: pointer;
  }

  .key-status {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    margin: var(--space-4) 0 0;
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    color: var(--color-text-secondary);
    font-size: var(--text-help-size);
  }

  .api-key-actions {
    justify-content: space-between;
  }

  .api-key-primary-actions {
    display: flex;
    gap: var(--space-3);
  }

  .continue-look {
    display: inline-flex;
    min-width: 112px;
    min-height: 36px;
    align-items: center;
    justify-content: center;
    padding: 0 var(--space-4);
    border-radius: var(--radius-md);
    background: var(--color-primary-action);
    color: var(--color-on-primary);
    font-weight: 500;
  }

  .api-key-primary-actions button[disabled],
  .api-key-actions .ghost[disabled] {
    cursor: not-allowed;
    opacity: 0.6;
  }
</style>
