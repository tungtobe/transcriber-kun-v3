<script lang="ts">
  // Footer dưới danh sách Home (story 3.3, spec Boundaries Always): hiện
  // luôn khi có ít nhất một Phiên -- "38 phiên" khi không lọc, "6 / 38 phiên
  // · lọc…" + nút "Xoá bộ lọc" khi đang lọc (tag và/hoặc tên). `aria-live`
  // để trình đọc màn hình báo lại số đếm mỗi khi gõ/đổi tag mà không cần
  // focus di chuyển tới đây.
  import { i18n } from '../../i18n/index.svelte';
  import { isSessionFilterActive } from '../../lib/session-filter';
  import { libraryStore } from '../../lib/stores/library.svelte';

  const totalCount = $derived(libraryStore.sessions.length);
  const shownCount = $derived(libraryStore.filteredSessions.length);

  const currentFilter = $derived({ ...libraryStore.tagFilter, query: libraryStore.nameQuery });
  const filtering = $derived(isSessionFilterActive(currentFilter));

  // "lọc…" nêu ngắn gọn query (nếu có) và/hoặc số tag / "Chưa gắn tag" (spec
  // Always) -- untagged và tagIds không bao giờ cùng có hiệu lực
  // (`libraryStore.toggleFilterTag`/`toggleUntaggedFilter` đảm bảo).
  const summaryParts = $derived.by(() => {
    const parts: string[] = [];
    const trimmedQuery = libraryStore.nameQuery.trim();
    if (trimmedQuery.length > 0) {
      parts.push(i18n.t('home.footer.summaryQuery', { query: trimmedQuery }));
    }
    if (libraryStore.tagFilter.untagged) {
      parts.push(i18n.t('tag.filter.untaggedChip'));
    } else if (libraryStore.tagFilter.tagIds.length > 0) {
      parts.push(i18n.t('home.footer.summaryTagCount', { count: libraryStore.tagFilter.tagIds.length }));
    }
    return parts;
  });

  // Một chuỗi duy nhất (thay vì nhiều expression liền kề trong template) để
  // tránh trộn nhiều text node trong cùng một phần tử -- vừa dễ đọc, vừa dễ
  // test (một `getByText` khớp trọn nội dung).
  const filteredLabel = $derived(
    `${i18n.t('home.footer.filtered', { shown: shownCount, total: totalCount })} · ${i18n.t('home.footer.filterNote', { summary: summaryParts.join(', ') })}`,
  );
</script>

{#if totalCount > 0}
  <div class="session-list-footer" role="status" aria-live="polite">
    {#if filtering}
      <span class="session-list-footer-count">{filteredLabel}</span>
      <button type="button" class="session-list-footer-clear" onclick={() => libraryStore.clearAllFilters()}>
        {i18n.t('home.footer.clearFilters')}
      </button>
    {:else}
      <span class="session-list-footer-count">{i18n.t('home.footer.total', { count: totalCount })}</span>
    {/if}
  </div>
{/if}

<style>
  .session-list-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin-top: var(--space-3);
    color: var(--color-text-secondary);
    font-size: var(--text-help-size);
  }

  .session-list-footer-count {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .session-list-footer-clear {
    flex: 0 0 auto;
    padding: 0;
    border: none;
    background: none;
    color: var(--color-accent);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .session-list-footer-clear:hover {
    text-decoration: underline;
  }

  .session-list-footer-clear:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }
</style>
