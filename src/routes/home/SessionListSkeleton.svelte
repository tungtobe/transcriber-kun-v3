<script lang="ts">
  // Skeleton 6 dòng khớp bố cục dòng thật (spec Boundaries "Đang tải": "vùng
  // danh sách là skeleton 6 dòng cùng bố cục dòng thật"; EXPERIENCE.md "Cold
  // load"). Chiều cao mỗi dòng khớp `SessionRow`/`SessionList.ROW_HEIGHT`
  // (56px) để không có nhảy layout khi dữ liệu thật thay vào.
  import { i18n } from '../../i18n/index.svelte';

  const ROWS = 6;
</script>

<div class="session-list-skeleton" role="status" aria-label={i18n.t('home.header.title')}>
  {#each Array(ROWS) as _, index (index)}
    <div class="skeleton-row">
      <span class="skeleton-bar skeleton-title"></span>
      <span class="skeleton-bar skeleton-date"></span>
      <span class="skeleton-bar skeleton-badge"></span>
      <span class="skeleton-bar skeleton-duration"></span>
    </div>
  {/each}
</div>

<style>
  .session-list-skeleton {
    display: flex;
    flex-direction: column;
  }

  .skeleton-row {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    height: 56px;
    padding: 0 var(--space-4);
    border-bottom: 1px solid var(--color-border);
  }

  .skeleton-bar {
    display: inline-block;
    height: 12px;
    border-radius: var(--radius-sm);
    background: var(--color-surface-sunken);
  }

  .skeleton-title {
    flex: 1;
    max-width: 240px;
  }

  .skeleton-date {
    width: 72px;
  }

  .skeleton-badge {
    width: 44px;
    height: 20px;
    border-radius: var(--radius-sm);
  }

  .skeleton-duration {
    width: 48px;
  }
</style>
