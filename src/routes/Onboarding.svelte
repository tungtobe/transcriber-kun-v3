<script lang="ts">
  import { i18n, type Locale, type TranslationKey } from '../i18n/index.svelte';
  import { settingsStore } from '../lib/stores/settings.svelte';

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
</script>

<svelte:head>
  <title>{i18n.t('app.meta.onboardingTitle')}</title>
</svelte:head>

<section class="route-screen onboarding-screen" aria-labelledby="onboarding-title">
  <div class="onboarding-card">
    <p class="route-kicker">{i18n.t('onboarding.header.kicker')}</p>
    <h1 id="onboarding-title">{i18n.t('onboarding.header.title')}</h1>
    <p class="route-lede">{i18n.t('onboarding.header.description')}</p>

    <ol class="stepper" aria-label={i18n.t('onboarding.stepper.label')}>
      <li class="step step-current" aria-current="step"><span>1</span> {i18n.t('onboarding.stepper.language')}</li>
      <li class="step"><span>2</span> {i18n.t('onboarding.stepper.data')}</li>
      <li class="step"><span>3</span> {i18n.t('onboarding.stepper.apiKey')}</li>
    </ol>

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
        aria-disabled="true"
        title={i18n.t('onboarding.action.unavailable')}
      >
        {i18n.t('onboarding.action.continue')}
      </button>
    </div>
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

  .actions button[aria-disabled='true'] {
    cursor: not-allowed;
    opacity: 0.45;
  }
</style>
