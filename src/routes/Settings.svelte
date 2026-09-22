<script lang="ts">
  import { link } from '@keenmate/svelte-spa-router';
  import { routePaths } from '../lib/router';

  type RouteParams = { group?: string };
  let { routeParams = {} }: { routeParams?: RouteParams } = $props();

  const groups = [
    { key: 'general', label: 'Chung' },
    { key: 'gemini', label: 'Gemini' },
    { key: 'diagnostics', label: 'Chẩn đoán' },
    { key: 'about', label: 'Giới thiệu & Quyền riêng tư' },
  ];

  let group = $derived(routeParams.group ?? 'general');
  let groupLabel = $derived(groups.find((item) => item.key === group)?.label ?? 'Chung');
</script>

<svelte:head>
  <title>{groupLabel} · Cài đặt · trans-kun</title>
</svelte:head>

<section class="route-screen" aria-labelledby="settings-title">
  <div class="route-kicker">Cấu hình</div>
  <h1 id="settings-title">Cài đặt</h1>

  <div class="settings-layout">
    <nav class="settings-nav" aria-label="Nhóm cài đặt">
      {#each groups as item}
        <a
          use:link
          class:active={item.key === group}
          href={routePaths.settings({ group: item.key })}
          aria-current={item.key === group ? 'page' : undefined}
        >
          {item.label}
        </a>
      {/each}
    </nav>

    <div class="settings-panel">
      <h2>{groupLabel}</h2>
      <p>
        Nhóm cài đặt này là placeholder có thật trong shell. Các trường cấu hình sẽ được
        bổ sung ở các story Settings tương ứng.
      </p>
      <div class="settings-row">
        <span class="settings-label">Đường dẫn route</span>
        <code>/settings/{group}</code>
      </div>
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
