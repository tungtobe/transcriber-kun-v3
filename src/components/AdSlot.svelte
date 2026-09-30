<script lang="ts">
  import { commands, type AdCreativeView, type AdsLocale } from '../lib/bindings';
  import { i18n } from '../i18n/index.svelte';

  let { eligible }: { eligible: boolean } = $props();

  let ad = $state<AdCreativeView | null>(null);
  let loading = $state(true);
  let whyOpen = $state(false);
  let actionStatus = $state('');
  let slotElement = $state<HTMLElement | null>(null);

  let requestSequence = 0;
  const acknowledgedTokens = new Set<string>();

  $effect(() => {
    const canShowAds = eligible;
    const locale = i18n.locale as AdsLocale;
    const request = ++requestSequence;

    ad = null;
    whyOpen = false;
    actionStatus = '';
    loading = canShowAds;
    if (!canShowAds) return;

    void (async () => {
      try {
        const result = await commands.adsNext(locale);
        if (request !== requestSequence || !eligible) return;
        ad = result.status === 'ok' ? result.data : null;
      } catch {
        if (request !== requestSequence || !eligible) return;
        ad = null;
      } finally {
        if (request === requestSequence && eligible) loading = false;
      }
    })();

    return () => {
      if (request === requestSequence) requestSequence += 1;
    };
  });

  async function acknowledgeVisible(token: string): Promise<void> {
    if (!eligible || ad?.token !== token || acknowledgedTokens.has(token)) return;
    acknowledgedTokens.add(token);
    try {
      await commands.adsImpression(token);
    } catch {
      // An impression is best-effort local accounting and never blocks the UI.
    }
  }

  function isActuallyVisible(element: HTMLElement): boolean {
    if (document.visibilityState === 'hidden') return false;
    const rect = element.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return false;
    const visibleWidth = Math.max(0, Math.min(rect.right, window.innerWidth) - Math.max(rect.left, 0));
    const visibleHeight = Math.max(0, Math.min(rect.bottom, window.innerHeight) - Math.max(rect.top, 0));
    return (visibleWidth * visibleHeight) / (rect.width * rect.height) >= 0.5;
  }

  $effect(() => {
    const selected = ad;
    const element = slotElement;
    if (!eligible || !selected || !element) return;

    if (typeof IntersectionObserver === 'undefined') {
      if (isActuallyVisible(element)) void acknowledgeVisible(selected.token);
      return;
    }

    const observer = new IntersectionObserver((entries) => {
      if (
        eligible &&
        entries.some((entry) => entry.target === element && entry.isIntersecting && entry.intersectionRatio >= 0.5) &&
        isActuallyVisible(element)
      ) {
        void acknowledgeVisible(selected.token);
      }
    }, { threshold: 0.5 });
    observer.observe(element);

    const recheckVisibility = () => {
      if (isActuallyVisible(element)) void acknowledgeVisible(selected.token);
    };
    document.addEventListener('visibilitychange', recheckVisibility);
    return () => {
      observer.disconnect();
      document.removeEventListener('visibilitychange', recheckVisibility);
    };
  });

  async function runAction(action: 'click' | 'report'): Promise<void> {
    const selected = ad;
    if (!eligible || !selected) return;

    actionStatus = '';
    try {
      const result = action === 'click'
        ? await commands.adsClick(selected.token)
        : await commands.adsReport(selected.token);
      if (!eligible || ad?.token !== selected.token) return;
      if (result.status !== 'ok') {
        actionStatus = i18n.t('ads.ui.actionError');
      } else if (result.data === 'opened') {
        actionStatus = i18n.t(action === 'click' ? 'ads.ui.opened' : 'ads.ui.reportOpened');
      } else if (result.data === 'unavailable') {
        actionStatus = i18n.t(action === 'click' ? 'ads.ui.openUnavailable' : 'ads.ui.reportUnavailable');
      } else if (result.data === 'suppressed') {
        ad = null;
      } else {
        actionStatus = i18n.t('ads.ui.actionError');
      }
    } catch {
      if (eligible && ad?.token === selected.token) actionStatus = i18n.t('ads.ui.actionError');
    }
  }
</script>

{#if loading}
  <section class="ad-slot ad-loading" aria-label={i18n.t('ads.slot.label')} role="status">
    <span class="loading-label">{i18n.t('ads.ui.loading')}</span>
    <span class="loading-block image-placeholder" aria-hidden="true"></span>
    <span class="loading-line" aria-hidden="true"></span>
    <span class="loading-line short" aria-hidden="true"></span>
    <span class="loading-controls" aria-hidden="true">
      <span></span><span></span>
    </span>
  </section>
{:else if ad}
  <section class="ad-slot" aria-label={i18n.t('ads.slot.label')} bind:this={slotElement}>
    <span class="sponsored-label">{ad.sponsoredLabel}</span>

    {#if ad.imageDataUrl}
      <button
        class="creative-button"
        type="button"
        aria-label={i18n.t('ads.ui.open', { sponsor: ad.sponsor, title: ad.title })}
        aria-describedby={whyOpen ? 'ad-creative-description ad-why-details' : 'ad-creative-description'}
        onclick={() => void runAction('click')}
      >
        <span class="image-frame" style={`aspect-ratio: ${ad.width} / ${ad.height}`}>
          <img src={ad.imageDataUrl} alt="" aria-hidden="true" />
        </span>
        <span class="creative-copy">
          <span class="sponsor-name">{ad.sponsor}</span>
          <span class="creative-title">{ad.title}</span>
          <span class="creative-body" id="ad-creative-description">{ad.body}</span>
        </span>
      </button>
    {:else}
      <div class="creative-button static-creative">
        <span class="image-frame" style={`aspect-ratio: ${ad.width} / ${ad.height}`}>
          <span class="image-fallback" aria-hidden="true">{ad.sponsor}</span>
        </span>
        <span class="creative-copy">
          <span class="sponsor-name">{ad.sponsor}</span>
          <span class="creative-title">{ad.title}</span>
          <span class="creative-body">{ad.body}</span>
        </span>
      </div>
    {/if}

    <div class="ad-controls" role="group" aria-label={i18n.t('ads.ui.controls')}>
      <button type="button" aria-label={i18n.t('ads.ui.why')} aria-expanded={whyOpen} aria-controls="ad-why-details" onclick={() => (whyOpen = !whyOpen)}>
        {i18n.t('ads.ui.why')}
      </button>
      <button type="button" aria-label={i18n.t('ads.ui.report')} onclick={() => void runAction('report')}>
        {i18n.t('ads.ui.report')}
      </button>
    </div>

    <p class="why-details" id="ad-why-details" hidden={!whyOpen}>{ad.whyThisAd}</p>
    <p class="sr-only" role="status" aria-live="polite">{actionStatus}</p>
  </section>
{/if}

<style>
  .ad-slot {
    box-sizing: border-box;
    display: flex;
    width: 236px;
    min-width: 236px;
    min-height: 220px;
    flex: 0 0 236px;
    align-self: center;
    flex-direction: column;
    gap: var(--space-2);
    margin-bottom: var(--space-4);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    color: var(--color-text);
  }

  .sponsored-label,
  .sponsor-name {
    color: var(--color-text-secondary);
    font-size: var(--text-badge-size);
    font-weight: 600;
    line-height: 1.3;
  }

  .sponsored-label {
    align-self: flex-start;
    padding: 2px var(--space-2);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-sm);
  }

  .creative-button {
    display: flex;
    width: 100%;
    flex-direction: column;
    gap: var(--space-2);
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    cursor: pointer;
    font: inherit;
    text-align: left;
  }

  .static-creative {
    cursor: default;
  }

  .image-frame {
    display: grid;
    width: 100%;
    max-height: 90px;
    place-items: center;
    overflow: hidden;
    border-radius: var(--radius-md);
    background: var(--color-surface-sunken);
  }

  .image-frame img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .image-fallback {
    color: var(--color-text-secondary);
    font-size: var(--text-label-size);
    font-weight: 600;
  }

  .creative-copy {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 2px;
  }

  .creative-title {
    color: var(--color-text);
    font-size: var(--text-label-size);
    font-weight: 600;
    line-height: 1.3;
    overflow-wrap: anywhere;
  }

  .creative-body,
  .why-details {
    margin: 0;
    color: var(--color-text-secondary);
    font-size: var(--text-help-size);
    line-height: 1.4;
    overflow-wrap: anywhere;
  }

  .ad-controls {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin-top: auto;
  }

  .ad-controls button {
    min-height: 30px;
    padding: 4px var(--space-2);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text-secondary);
    cursor: pointer;
    font: inherit;
    font-size: var(--text-badge-size);
    line-height: 1.2;
  }

  .creative-button:focus-visible,
  .ad-controls button:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }

  .why-details {
    padding: var(--space-2);
    border-radius: var(--radius-md);
    background: var(--color-surface-sunken);
  }

  .ad-loading {
    justify-content: flex-start;
  }

  .loading-label {
    color: var(--color-text-secondary);
    font-size: var(--text-badge-size);
    font-weight: 600;
  }

  .loading-block,
  .loading-line,
  .loading-controls span {
    display: block;
    border-radius: var(--radius-md);
    background: var(--color-surface-sunken);
  }

  .image-placeholder {
    width: 100%;
    height: 72px;
  }

  .loading-line {
    width: 86%;
    height: 10px;
  }

  .loading-line.short {
    width: 64%;
  }

  .loading-controls {
    display: flex;
    gap: var(--space-1);
    margin-top: auto;
  }

  .loading-controls span {
    width: 78px;
    height: 30px;
    border: 1px solid var(--color-border);
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
</style>
