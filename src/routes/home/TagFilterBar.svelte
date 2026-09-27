<script lang="ts">
  // Hàng chip lọc tag ở Home (story 3.2, spec Boundaries Always): một dòng,
  // không wrap -- tag đang lọc (có ×) → tối đa 5 tag dùng nhiều nhất chưa lọc
  // → chip "+ N tag khác" mở `TagPicker` mode filter → vạch dọc → chip "Chưa
  // gắn tag". Mọi thao tác đọc/ghi tag đi qua `libraryStore` -- component này
  // chỉ vẽ UI và nối callback.
  import { i18n } from '../../i18n/index.svelte';
  import { XIcon } from '../../components/icons';
  import TagPicker from '../../components/TagPicker.svelte';
  import { libraryStore } from '../../lib/stores/library.svelte';

  const MAX_POPULAR_CHIPS = 5;

  const filteredTagIds = $derived(libraryStore.tagFilter.tagIds);
  const filteredTags = $derived(
    libraryStore.tags.filter((tag) => filteredTagIds.includes(tag.id)),
  );
  const unfilteredTags = $derived(
    libraryStore.tags.filter((tag) => !filteredTagIds.includes(tag.id)),
  );
  const popularTags = $derived(unfilteredTags.slice(0, MAX_POPULAR_CHIPS));
  const moreCount = $derived(unfilteredTags.length - popularTags.length);

  let pickerOpen = $state(false);
  let moreButtonEl = $state<HTMLButtonElement | null>(null);

  function openPicker(): void {
    pickerOpen = true;
  }

  function closePicker(): void {
    pickerOpen = false;
    moreButtonEl?.focus();
  }

  async function toggleTag(tagId: string): Promise<{ status: 'ok' } | { status: 'error'; message: string }> {
    libraryStore.toggleFilterTag(tagId);
    return { status: 'ok' };
  }

  async function createTag(name: string) {
    const result = await libraryStore.createTag(name);
    return result.status === 'ok'
      ? { status: 'ok' as const, id: result.tag.id, name: result.tag.name }
      : { status: 'error' as const, message: i18n.t('tagPicker.error.failed') };
  }

  async function deleteTag(tagId: string) {
    const result = await libraryStore.deleteTagGlobally(tagId);
    return result.status === 'ok'
      ? { status: 'ok' as const }
      : { status: 'error' as const, message: i18n.t('tagPicker.deleteDialog.error') };
  }
</script>

{#if libraryStore.tags.length > 0}
  <div class="tag-filter-bar">
    {#each filteredTags as tag (tag.id)}
      <button
        type="button"
        class="tag-chip tag-chip-on"
        onclick={() => libraryStore.toggleFilterTag(tag.id)}
      >
        <span class="tag-chip-label">{tag.name}</span>
        <XIcon size={12} strokeWidth={2} aria-hidden="true" />
        <span class="sr-only">{i18n.t('tag.filter.removeLabel', { name: tag.name })}</span>
      </button>
    {/each}

    {#each popularTags as tag (tag.id)}
      <button type="button" class="tag-chip" onclick={() => libraryStore.toggleFilterTag(tag.id)}>
        {tag.name}
      </button>
    {/each}

    {#if moreCount > 0}
      <div class="tag-filter-more-wrap">
        <button
          type="button"
          class="tag-chip"
          bind:this={moreButtonEl}
          aria-haspopup="dialog"
          aria-expanded={pickerOpen}
          onclick={openPicker}
        >
          {i18n.t('tag.filter.moreChip', { count: moreCount })}
        </button>
        {#if pickerOpen}
          <TagPicker
            mode="filter"
            dialogLabel={i18n.t('tagPicker.dialog.filterLabel')}
            tags={libraryStore.tags}
            selectedIds={filteredTagIds}
            onToggle={toggleTag}
            onCreate={createTag}
            onDeleteTag={deleteTag}
            onClose={closePicker}
          />
        {/if}
      </div>
    {/if}

    <span class="tag-filter-divider" aria-hidden="true"></span>

    <button
      type="button"
      class="tag-chip"
      class:tag-chip-on={libraryStore.tagFilter.untagged}
      onclick={() => libraryStore.toggleUntaggedFilter()}
    >
      {i18n.t('tag.filter.untaggedChip')}
    </button>
  </div>
{/if}

<style>
  .tag-filter-bar {
    display: flex;
    overflow: hidden;
    align-items: center;
    gap: var(--space-2);
    margin-bottom: var(--space-4);
    white-space: nowrap;
  }

  .tag-chip {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border: 1px solid transparent;
    border-radius: var(--radius-full);
    background: var(--color-surface-sunken);
    color: var(--color-text-secondary);
    font: inherit;
    font-size: var(--text-help-size);
    font-weight: 500;
    cursor: pointer;
  }

  .tag-chip:hover {
    background: var(--color-border);
  }

  .tag-chip:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }

  .tag-chip-on {
    border-color: var(--color-accent-border);
    background: var(--color-accent-soft);
    color: var(--color-accent-hover);
  }

  .tag-chip-label {
    overflow: hidden;
    max-width: 160px;
    text-overflow: ellipsis;
  }

  .tag-filter-more-wrap {
    position: relative;
    flex: 0 0 auto;
  }

  .tag-filter-divider {
    width: 1px;
    height: 18px;
    flex: 0 0 auto;
    background: var(--color-border-strong);
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
</style>
