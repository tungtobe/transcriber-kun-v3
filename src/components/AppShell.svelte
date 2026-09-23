<script lang="ts">
  import type { Snippet } from 'svelte';
  import { location, link, replace } from '@keenmate/svelte-spa-router';
  import {
    AlertTriangleIcon,
    HomeIcon,
    MonitorIcon,
    MoonIcon,
    RadioIcon,
    SettingsIcon,
    SunIcon,
  } from './icons';
  import { appStore } from '../lib/stores/app.svelte';
  import { settingsStore } from '../lib/stores/settings.svelte';
  import { intakeStore } from '../lib/stores/intake.svelte';
  import { onDragDropEvent, type DragDropEvent } from '../lib/dragdrop';
  import { i18n } from '../i18n/index.svelte';
  import type { Theme } from '../lib/bindings';
  import CloseConfirm from './CloseConfirm.svelte';
  import DropOverlay from './DropOverlay.svelte';

  let { children }: { children?: Snippet } = $props();

  const currentPath = $derived(location());
  const consentRestricted = $derived(settingsStore.consentStatus === 'declined');
  // Kéo thả (và overlay của nó) chỉ nhận ở Home và Transcript detail (spec
  // Never: "Không nhận file khi đang ở Onboarding/Settings").
  const dropEnabled = $derived(
    currentPath === '/home' || currentPath === '/' || currentPath.startsWith('/session/'),
  );
  const pageTitle = $derived(
    currentPath.startsWith('/settings')
      ? i18n.t('app.shell.settings')
      : currentPath === '/onboarding'
        ? i18n.t('app.shell.onboarding')
        : i18n.t('app.shell.home'),
  );

  let dragging = $state(false);

  $effect(() => {
    if (consentRestricted && currentPath !== '/onboarding' && currentPath !== '/settings/about') {
      void replace('/settings/about');
    }
  });

  // Đăng ký đúng một lần cho toàn app (spec Code Map: sự kiện drag-drop
  // native của webview, không phải HTML5 `drop`) — `dropEnabled` được đọc
  // lại bên trong handler mỗi lần sự kiện tới nên luôn phản ánh route hiện
  // tại, không phải route lúc đăng ký.
  $effect(() => {
    let active = true;
    let unlisten: (() => void) | null = null;
    void onDragDropEvent((event: DragDropEvent) => {
      const { payload } = event;
      if (payload.type === 'enter' || payload.type === 'over') {
        if (dropEnabled) dragging = true;
        return;
      }
      if (payload.type === 'leave') {
        dragging = false;
        return;
      }
      // 'drop'
      dragging = false;
      if (dropEnabled && payload.paths.length > 0) {
        void intakeStore.submit(payload.paths);
      }
    }).then((fn) => {
      if (!active) {
        fn();
        return;
      }
      unlisten = fn;
    });
    return () => {
      active = false;
      unlisten?.();
    };
  });

  function changeTheme(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value as Theme;
    void settingsStore.setTheme(value);
  }
</script>

<div class="app-shell">
  <aside class="sidebar" aria-label={i18n.t('app.shell.primaryNavLabel')}>
    <div class="brand-lockup">
      <span class="brand-mark" aria-hidden="true">tk</span>
      <div>
        <p class="brand-name">trans-kun</p>
        <p class="brand-caption">{i18n.t('app.shell.brandCaption')}</p>
      </div>
    </div>

    <nav class="primary-nav" aria-label={i18n.t('app.shell.navigationLabel')}>
      {#if !consentRestricted}
      <a
        href="/home"
        use:link
        class:active={currentPath === '/home' || currentPath === '/'}
        aria-current={currentPath === '/home' || currentPath === '/' ? 'page' : undefined}
      >
        <HomeIcon size={18} strokeWidth={1.75} aria-hidden="true" />
        <span>{i18n.t('app.shell.home')}</span>
      </a>
      {/if}

      <button
        class="nav-disabled"
        type="button"
        aria-disabled="true"
        title={i18n.t('app.shell.liveUnavailable')}
        aria-label={i18n.t('app.shell.liveUnavailable')}
      >
        <RadioIcon size={18} strokeWidth={1.75} aria-hidden="true" />
        <span>{i18n.t('app.shell.live')}</span>
      </button>

      <a
        href="/settings/general"
        use:link
        class:active={currentPath.startsWith('/settings')}
        aria-current={currentPath.startsWith('/settings') ? 'page' : undefined}
      >
        <SettingsIcon size={18} strokeWidth={1.75} aria-hidden="true" />
        <span>{i18n.t('app.shell.settings')}</span>
      </a>
    </nav>

    <section class="job-placeholder" aria-label={i18n.t('app.shell.runningJobs')}>
      <div class="job-heading">
        <span>{i18n.t('app.shell.runningJobs')}</span>
        <span class="job-count">0</span>
      </div>
      <p>{i18n.t('app.shell.noRunningJobs')}</p>
    </section>

    <div class="sidebar-spacer"></div>

    <section class="sidebar-footer" aria-label={i18n.t('app.shell.statusLabel')}>
      <p class="version-label">{i18n.t('app.version.label')}</p>
      <p class="version-value mono">
        {#if appStore.version.status === 'ok'}
          {appStore.version.version}
        {:else if appStore.version.status === 'loading'}
          …
        {:else}
          —
        {/if}
      </p>
    </section>
  </aside>

  <div class="shell-content">
    <header class="app-header">
      <div>
        <p class="header-kicker">trans-kun</p>
        <h1>{pageTitle}</h1>
      </div>

      <div class="header-controls">
        <label class="theme-control" for="theme-select">
          <span class="sr-only">{i18n.t('app.theme.label')}</span>
          {#if settingsStore.theme === 'system'}
            <MonitorIcon size={16} strokeWidth={1.75} aria-hidden="true" />
          {:else if settingsStore.theme === 'dark'}
            <MoonIcon size={16} strokeWidth={1.75} aria-hidden="true" />
          {:else}
            <SunIcon size={16} strokeWidth={1.75} aria-hidden="true" />
          {/if}
          <select
            id="theme-select"
            aria-label={i18n.t('app.theme.label')}
            value={settingsStore.theme}
            onchange={changeTheme}
          >
            <option value="system">{i18n.t('app.theme.system')}</option>
            <option value="light">{i18n.t('app.theme.light')}</option>
            <option value="dark">{i18n.t('app.theme.dark')}</option>
          </select>
        </label>
      </div>
    </header>

    {#if settingsStore.error}
      <div class="shell-error" role="status">
        <AlertTriangleIcon size={18} strokeWidth={1.75} aria-hidden="true" />
        <div>
          <strong>{i18n.t('app.error.syncTitle')}</strong>
          <p>{i18n.t('app.error.syncBody')}</p>
        </div>
        <button type="button" onclick={() => void settingsStore.load()}>{i18n.t('app.error.reload')}</button>
      </div>
    {/if}

    {#if consentRestricted && currentPath !== '/onboarding'}
      <div class="consent-banner" role="status">
        <span>{i18n.t('onboarding.consent.returnBanner')}</span>
        <a href="/onboarding" use:link>{i18n.t('onboarding.consent.accept')}</a>
      </div>
    {/if}

    <main class="main-content" aria-label={pageTitle}>
      {@render children?.()}
    </main>
  </div>
</div>

<DropOverlay active={dragging && dropEnabled} />
<CloseConfirm />

<style>
  .app-shell {
    display: flex;
    width: 100%;
    min-width: var(--window-min-width);
    height: 100%;
    min-height: var(--window-min-height);
    background: var(--color-bg);
    color: var(--color-text);
  }

  .sidebar {
    display: flex;
    width: var(--sidebar-width);
    min-width: var(--sidebar-width);
    flex: 0 0 var(--sidebar-width);
    flex-direction: column;
    min-height: var(--window-min-height);
    padding: var(--space-5) var(--space-4) var(--space-4);
    border-right: 1px solid var(--color-border);
    background: var(--color-bg-sidebar);
  }

  .brand-lockup {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 40px;
    margin-bottom: var(--space-6);
  }

  .brand-mark {
    display: grid;
    width: 32px;
    height: 32px;
    place-items: center;
    border-radius: var(--radius-md);
    background: var(--color-primary-action);
    color: var(--color-on-primary);
    font-family: var(--font-mono);
    font-size: var(--text-help-size);
    font-weight: 600;
    letter-spacing: -0.05em;
  }

  .brand-name,
  .brand-caption,
  .header-kicker,
  .version-label,
  .version-value,
  .job-placeholder p {
    margin: 0;
  }

  .brand-name {
    font-size: 15px;
    font-weight: 600;
  }

  .brand-caption,
  .header-kicker,
  .version-label {
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
  }

  .primary-nav {
    display: grid;
    gap: var(--space-1);
  }

  .primary-nav a,
  .primary-nav button {
    display: flex;
    width: 100%;
    min-height: 36px;
    align-items: center;
    gap: var(--space-3);
    padding: 8px 12px;
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-text-secondary);
    text-align: left;
    text-decoration: none;
  }

  .primary-nav a:hover {
    background: var(--color-surface-sunken);
  }

  .primary-nav a.active {
    background: var(--color-accent-soft);
    color: var(--color-accent);
    font-weight: 600;
  }

  .nav-disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }

  .job-placeholder {
    margin-top: var(--space-6);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
  }

  .job-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: var(--space-2);
    color: var(--color-text-secondary);
    font-size: var(--text-label-size);
    font-weight: 600;
  }

  .job-count {
    display: grid;
    min-width: 20px;
    min-height: 20px;
    padding: 0 6px;
    place-items: center;
    border-radius: var(--radius-sm);
    background: var(--color-surface-sunken);
    font-family: var(--font-mono);
    font-size: var(--text-help-size);
    font-variant-numeric: tabular-nums;
  }

  .job-placeholder p {
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
  }

  .sidebar-spacer {
    flex: 1;
  }

  .sidebar-footer {
    padding-top: var(--space-3);
    border-top: 1px solid var(--color-border);
  }

  .version-value {
    color: var(--color-text-secondary);
    font-size: var(--text-help-size);
  }

  .shell-content {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
  }

  .app-header {
    display: flex;
    height: var(--header-height);
    min-height: var(--header-height);
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: 0 var(--space-6);
    border-bottom: 1px solid var(--color-border);
    background: var(--color-surface);
  }

  .header-kicker {
    line-height: 1.2;
  }

  .app-header h1 {
    margin: 2px 0 0;
    font-size: var(--text-screen-title-size);
    line-height: 1.35;
  }

  .header-controls,
  .theme-control {
    display: flex;
    align-items: center;
  }

  .theme-control {
    gap: var(--space-2);
    color: var(--color-text-secondary);
  }

  .theme-control select {
    min-height: 36px;
    padding: 0 32px 0 var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    cursor: pointer;
  }

  .shell-error {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    margin: var(--space-3) var(--space-6) 0;
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--color-warning-border);
    border-radius: var(--radius-lg);
    background: var(--color-warning-soft);
    color: var(--color-warning);
  }

  .shell-error div {
    min-width: 0;
    flex: 1;
  }

  .shell-error strong {
    display: block;
    font-size: var(--text-label-size);
  }

  .shell-error p {
    margin: 2px 0 0;
    color: var(--color-text-secondary);
    font-size: var(--text-help-size);
  }

  .shell-error button {
    min-height: 32px;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-warning-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    cursor: pointer;
    font-size: var(--text-label-size);
    font-weight: 600;
  }

  .consent-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin: var(--space-3) var(--space-6) 0;
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--color-accent-border);
    border-radius: var(--radius-lg);
    background: var(--color-accent-soft);
    color: var(--color-text-secondary);
    font-size: var(--text-help-size);
  }

  .consent-banner a { color: var(--color-accent); font-weight: 600; }

  .main-content {
    min-width: 0;
    flex: 1;
    overflow: auto;
    background: var(--color-bg);
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }

  @media (max-width: 1279px) {
    .app-header {
      padding-inline: var(--space-4);
    }

    .shell-error {
      margin-inline: var(--space-4);
    }
  }
</style>
