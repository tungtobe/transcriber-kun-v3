<script lang="ts">
  import { onMount } from 'svelte';
  import { InfoIcon, RadioIcon, UploadIcon } from '../components/icons';
  import DisabledHint from '../components/DisabledHint.svelte';
  import BannerStack, { type BannerItem } from '../components/BannerStack.svelte';
  import { i18n } from '../i18n/index.svelte';
  import { keysStore } from '../lib/stores/keys.svelte';

  onMount(() => {
    void keysStore.load();
  });

  // Loading is treated as "not yet confirmed missing" to avoid a flash of
  // the banner while `keysList` is still in flight; a load error is treated
  // conservatively as missing (spec I/O Matrix: no banner only once a usable
  // key is confirmed present).
  const showKeyBanner = $derived(
    keysStore.status !== 'idle'
      && keysStore.status !== 'loading'
      && (keysStore.status === 'error' || !keysStore.hasUsableKey),
  );

  const banners = $derived.by((): BannerItem[] =>
    showKeyBanner
      ? [
          {
            id: 'missing-key',
            variant: 'warning',
            title: i18n.t('home.banner.keyMissingTitle'),
            message: i18n.t('home.banner.keyMissingMessage'),
            actionLabel: i18n.t('home.banner.keyMissingAction'),
            actionHref: '/settings/gemini',
          },
        ]
      : [],
  );
</script>

<svelte:head>
  <title>{i18n.t('app.meta.homeTitle')}</title>
</svelte:head>

<section class="route-screen" aria-labelledby="home-title">
  <div class="screen-header">
    <div>
      <p class="route-kicker">{i18n.t('home.header.kicker')}</p>
      <h1 id="home-title">{i18n.t('home.header.title')}</h1>
    </div>
    <span class="status-pill status-off"><span class="status-dot" aria-hidden="true"></span> {i18n.t('home.header.emptyStatus')}</span>
  </div>

  <div class="key-banner-wrap">
    <BannerStack {banners} />
  </div>

  <div class="empty-grid">
    <article class="empty-card">
      <div class="empty-icon" aria-hidden="true"><UploadIcon size={18} strokeWidth={1.75} /></div>
      <h2>{i18n.t('home.file.title')}</h2>
      <p>{i18n.t('home.file.description')}</p>
      <DisabledHint reason={i18n.t('home.file.unavailable')}>
        <span class="button button-secondary">{i18n.t('home.file.action')}</span>
      </DisabledHint>
    </article>

    <article class="empty-card">
      <div class="empty-icon" aria-hidden="true"><RadioIcon size={18} strokeWidth={1.75} /></div>
      <h2>{i18n.t('home.live.title')}</h2>
      <p>{i18n.t('home.live.description')}</p>
      <DisabledHint reason={i18n.t('home.live.unavailable')}>
        <span class="button button-primary">{i18n.t('home.live.action')}</span>
      </DisabledHint>
    </article>
  </div>

  <div class="empty-note" role="status">
    <InfoIcon size={18} strokeWidth={1.75} aria-hidden="true" />
    <p>{i18n.t('home.note.ready')}</p>
  </div>
</section>

<style>
  .route-screen {
    max-width: 980px;
    margin: 0 auto;
    padding: var(--space-8);
  }

  .screen-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-4);
    margin-bottom: var(--space-6);
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
    margin: 0;
    font-size: var(--text-screen-title-size);
    line-height: 1.35;
  }

  h2 {
    margin: 0 0 var(--space-2);
    font-size: var(--text-h2-size);
  }

  .status-pill {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 30px;
    padding: 0 var(--space-3);
    border-radius: var(--radius-full);
    font-size: var(--text-label-size);
    font-weight: 600;
    white-space: nowrap;
  }

  .status-off {
    background: var(--color-surface-sunken);
    color: var(--color-text-secondary);
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: var(--radius-full);
    background: var(--color-text-muted);
  }

  .key-banner-wrap:not(:empty) {
    margin-bottom: var(--space-5);
  }

  .empty-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-4);
  }

  .empty-card {
    min-height: 216px;
    padding: var(--space-6);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
    background: var(--color-surface);
  }

  .empty-card p {
    min-height: 44px;
    margin: 0 0 var(--space-5);
    color: var(--color-text-secondary);
  }

  .empty-icon {
    display: grid;
    width: 32px;
    height: 32px;
    margin-bottom: var(--space-3);
    place-items: center;
    border-radius: var(--radius-md);
    background: var(--color-accent-soft);
    color: var(--color-accent);
    font-size: 20px;
    font-weight: 600;
  }

  .button {
    display: inline-flex;
    min-height: 36px;
    align-items: center;
    justify-content: center;
    padding: 0 14px;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    font-size: var(--text-body-size);
    font-weight: 500;
  }

  .button-primary {
    background: var(--color-primary-action);
    color: var(--color-on-primary);
  }

  .button-secondary {
    border-color: var(--color-border-strong);
    background: var(--color-surface);
    color: var(--color-text);
  }

  .empty-note {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    margin-top: var(--space-5);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    color: var(--color-text-secondary);
  }

  .empty-note :global(svg) {
    flex: 0 0 auto;
    color: var(--color-info);
  }

  .empty-note p {
    margin: 0;
  }
</style>
