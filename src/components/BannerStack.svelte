<script lang="ts">
  import Banner, { type BannerVariant } from './Banner.svelte';

  export type BannerItem = {
    id: string;
    variant: BannerVariant;
    title: string;
    message: string;
    actionLabel?: string;
    actionHref?: string;
    onAction?: () => void;
  };

  let { banners, label }: { banners: BannerItem[]; label?: string } = $props();

  const SEVERITY: Record<BannerVariant, number> = { danger: 0, warning: 1, info: 2 };

  // At most two banners at once, danger first, then warning, then info
  // (spec Always: "stack tối đa 2 theo danger > warning > info").
  const visible = $derived(
    [...banners].sort((a, b) => SEVERITY[a.variant] - SEVERITY[b.variant]).slice(0, 2),
  );
</script>

{#if visible.length > 0}
  <div class="banner-stack" aria-label={label}>
    {#each visible as banner (banner.id)}
      <Banner
        variant={banner.variant}
        title={banner.title}
        message={banner.message}
        actionLabel={banner.actionLabel}
        actionHref={banner.actionHref}
        onAction={banner.onAction}
      />
    {/each}
  </div>
{/if}

<style>
  .banner-stack {
    display: grid;
    gap: var(--space-3);
  }
</style>
