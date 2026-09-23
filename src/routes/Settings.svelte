<script lang="ts">
  import { link } from '@keenmate/svelte-spa-router';
  import { routePaths } from '../lib/router';
  import { i18n, type TranslationKey } from '../i18n/index.svelte';
  import { settingsStore } from '../lib/stores/settings.svelte';
  import SettingsGeneral from './settings/SettingsGeneral.svelte';
  import SettingsGemini from './settings/SettingsGemini.svelte';
  import SettingsChunking from './settings/SettingsChunking.svelte';
  import SettingsDiagnostics from './settings/SettingsDiagnostics.svelte';
  import SettingsAbout from './settings/SettingsAbout.svelte';

  type RouteParams = { group?: string };
  let { routeParams = {} }: { routeParams?: RouteParams } = $props();

  const groups: Array<{ key: string; labelKey: TranslationKey }> = [
    { key: 'general', labelKey: 'settings.group.general' },
    { key: 'gemini', labelKey: 'settings.group.gemini' },
    { key: 'chunking', labelKey: 'settings.group.chunking' },
    { key: 'diagnostics', labelKey: 'settings.group.diagnostics' },
    { key: 'about', labelKey: 'settings.group.about' },
  ];

  let group = $derived(routeParams.group ?? 'general');
  let visibleGroups = $derived(settingsStore.consentStatus === 'declined'
    ? groups.filter((item) => item.key === 'about')
    : groups);
  let groupLabel = $derived(i18n.t(groups.find((item) => item.key === group)?.labelKey ?? 'settings.group.general'));
</script>

<svelte:head>
  <title>{i18n.t('settings.meta.title', { group: groupLabel })}</title>
</svelte:head>

<section class="route-screen" aria-labelledby="settings-title">
  <div class="route-kicker">{i18n.t('settings.header.kicker')}</div>
  <h1 id="settings-title">{i18n.t('settings.header.title')}</h1>

  <div class="settings-layout">
    <nav class="settings-nav" aria-label={i18n.t('settings.navigation.label')}>
      {#each visibleGroups as item}
        <a
          use:link
          class:active={item.key === group}
          href={routePaths.settings({ group: item.key })}
          aria-current={item.key === group ? 'page' : undefined}
        >
          {i18n.t(item.labelKey)}
        </a>
      {/each}
    </nav>

    <div class="settings-panel">
      <h2>{groupLabel}</h2>
      {#if group === 'general'}
        <SettingsGeneral />
      {:else if group === 'gemini'}
        <SettingsGemini />
      {:else if group === 'chunking'}
        <SettingsChunking />
      {:else if group === 'diagnostics'}
        <SettingsDiagnostics />
      {:else if group === 'about'}
        <SettingsAbout />
      {:else}
        <p>{i18n.t('settings.placeholder.description')}</p>
        <div class="settings-row">
          <span class="settings-label">{i18n.t('settings.placeholder.routePath')}</span>
          <code>/settings/{group}</code>
        </div>
      {/if}
    </div>
  </div>
</section>

<style>
  .route-screen {
    max-width: 980px;
    margin: 0 auto;
    padding: var(--space-8);
  }

  .route-kicker {
    margin: 0 0 var(--space-1);
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  h1 {
    margin: 0 0 var(--space-6);
    font-size: var(--text-screen-title-size);
    line-height: 1.35;
  }

  h2 {
    margin: 0 0 var(--space-2);
    font-size: var(--text-h2-size);
  }

  .settings-layout {
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr);
    gap: var(--space-6);
    align-items: start;
  }

  .settings-nav {
    display: grid;
    gap: var(--space-1);
  }

  .settings-nav a {
    min-height: 36px;
    padding: 8px 12px;
    border-radius: var(--radius-md);
    color: var(--color-text-secondary);
    text-decoration: none;
  }

  .settings-nav a:hover {
    background: var(--color-surface-sunken);
  }

  .settings-nav a.active {
    background: var(--color-accent-soft);
    color: var(--color-accent);
    font-weight: 600;
  }

  .settings-panel {
    min-height: 240px;
    padding: var(--space-6);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
    background: var(--color-surface);
  }

  .settings-panel p {
    margin: 0 0 var(--space-6);
    color: var(--color-text-secondary);
  }

  .settings-row {
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr);
    gap: var(--space-4);
    align-items: center;
    padding-top: var(--space-4);
    border-top: 1px solid var(--color-border);
  }

  .settings-label {
    color: var(--color-text-secondary);
    font-size: var(--text-label-size);
    font-weight: 600;
  }

  code {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }
</style>
