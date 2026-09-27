<script lang="ts" module>
  export type TagPickerMode = 'filter' | 'assign';

  export type TagPickerAction = { status: 'ok' } | { status: 'error'; message: string };
  export type TagPickerCreateResult =
    | { status: 'ok'; id: string; name: string }
    | { status: 'error'; message: string };

  export interface TagPickerTag {
    id: string;
    name: string;
    sessionCount: number;
  }
</script>

<script lang="ts">
  // Tag picker dùng chung (story 3.2, spec Approach): mode "filter" ở Home,
  // mode "assign" ở Transcript detail và menu ⋯ của dòng phiên. Component này
  // là "dumb" -- không tự gọi IPC, chỉ nhận `tags`/`selectedIds` và gọi lại
  // `onToggle`/`onCreate`/`onDeleteTag`; caller (`TagFilterBar`/`SessionRow`/
  // `SessionHeader`) nối chúng vào `libraryStore` (spec Never: "chỉ đảm bảo
  // component nhận mode/props đủ để tái dùng" -- không làm LiveSetup ở đây,
  // nhưng cùng props này đủ để Epic 4 tái dùng sau).
  //
  // Vị trí: component không tự định vị (không `position: fixed`) -- caller
  // bọc nó trong một phần tử `position: relative` và style `.tag-picker` neo
  // `top: calc(100% + ...)` giống `SessionMenu`'s dropdown, để luôn nằm cạnh
  // đúng nút đã mở nó bất kể nút đó ở đâu.
  import { i18n } from '../i18n/index.svelte';
  import { SearchIcon, TagIcon, Trash2Icon, ArrowLeftIcon } from './icons';
  import ConfirmDialog from './ConfirmDialog.svelte';
  import { floating } from '../lib/floating';

  const MAX_TAG_NAME_SCALARS = 80;
  const MAX_TAGS_PER_SESSION = 20;

  let {
    mode,
    dialogLabel,
    tags,
    selectedIds,
    onToggle,
    onCreate,
    onDeleteTag,
    onClose,
    anchor = null,
  }: {
    mode: TagPickerMode;
    /** Khi có: picker nổi `position: fixed` trên `document.body`, neo vào
     * phần tử này (xem `lib/floating.ts`) -- dùng khi mở từ dòng phiên trong
     * virtual list, nơi `position: absolute` bị cắt. */
    anchor?: HTMLElement | null;
    /** `aria-label` của `role="dialog"` -- caller truyền câu đã dịch đúng
     * ngữ cảnh ("Chọn tag để lọc" | "Chọn tag cho phiên"). */
    dialogLabel: string;
    /** Mọi tag kèm số Phiên, đã sắp số phiên giảm dần rồi tên (spec
     * Boundaries) -- thường là `libraryStore.tags` truyền thẳng. */
    tags: TagPickerTag[];
    /** Id tag đang chọn: `tagFilter.tagIds` (mode filter) hoặc tag của Phiên
     * (mode assign). */
    selectedIds: string[];
    /** Bật/tắt chọn một tag đã có -- caller quyết định gọi
     * `toggleFilterTag`/`attachTag`/`detachTag`. */
    onToggle: (tagId: string) => Promise<TagPickerAction>;
    /** Tạo tag mới (hoặc lấy lại tag đã có cùng tên chuẩn hoá) -- picker tự
     * chọn nó ngay sau khi tạo thành công. */
    onCreate: (name: string) => Promise<TagPickerCreateResult>;
    /** Xoá hẳn một tag toàn cục từ chế độ quản lý. */
    onDeleteTag: (tagId: string) => Promise<TagPickerAction>;
    /** Esc, click ngoài, hoặc xong việc -- caller đóng popover và trả focus
     * về nút đã mở nó. */
    onClose: () => void;
  } = $props();

  let query = $state('');
  let error = $state<string | null>(null);
  let busyTagId = $state<string | null>(null);
  let creating = $state(false);

  let managing = $state(false);
  let deleteTarget = $state<TagPickerTag | null>(null);
  let deleteError = $state<string | null>(null);
  let deleting = $state(false);

  let rootEl = $state<HTMLDivElement | null>(null);
  let searchInputEl = $state<HTMLInputElement | null>(null);

  $effect(() => {
    if (!managing) searchInputEl?.focus();
  });

  const normalizedQuery = $derived(query.trim().toLocaleLowerCase());
  const selectedTags = $derived(tags.filter((tag) => selectedIds.includes(tag.id)));
  const restTags = $derived(
    tags
      .filter((tag) => !selectedIds.includes(tag.id))
      .filter((tag) => normalizedQuery === '' || tag.name.toLocaleLowerCase().includes(normalizedQuery)),
  );
  const exactMatch = $derived(
    tags.find((tag) => tag.name.toLocaleLowerCase() === normalizedQuery) ?? null,
  );
  const canCreate = $derived(
    normalizedQuery !== '' && exactMatch === null && query.trim().length <= MAX_TAG_NAME_SCALARS,
  );
  // Spec Boundaries Always: "Phiên đã có 20 tag thì gắn thêm bị từ chối --
  // UI chặn tại chỗ có giải thích, Rust trả `Request`" -- chặn phía UI ở đây
  // (không cho tick thêm tag mới) là lớp phòng thủ đầu; `onToggle` vẫn có
  // thể trả lỗi `Request` nếu race với một tab/cửa sổ khác.
  const atLimit = $derived(mode === 'assign' && selectedIds.length >= MAX_TAGS_PER_SESSION);

  async function toggle(tagId: string): Promise<void> {
    error = null;
    busyTagId = tagId;
    const result = await onToggle(tagId);
    busyTagId = null;
    if (result.status === 'error') error = result.message;
  }

  async function submitSearch(): Promise<void> {
    const trimmed = query.trim();
    if (trimmed === '') return;
    if (Array.from(trimmed).length > MAX_TAG_NAME_SCALARS) {
      error = i18n.t('tagPicker.error.tooLong');
      return;
    }
    error = null;
    if (exactMatch) {
      const matchId = exactMatch.id;
      if (!selectedIds.includes(matchId) && atLimit) {
        error = i18n.t('tagPicker.error.limit');
        return;
      }
      query = '';
      await toggle(matchId);
      return;
    }
    if (atLimit) {
      error = i18n.t('tagPicker.error.limit');
      return;
    }
    creating = true;
    const result = await onCreate(trimmed);
    creating = false;
    if (result.status === 'error') {
      error = result.message;
      return;
    }
    query = '';
    if (!selectedIds.includes(result.id)) {
      await toggle(result.id);
    }
  }

  function handleSearchKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter') {
      event.preventDefault();
      void submitSearch();
    }
  }

  function openManage(): void {
    managing = true;
    deleteError = null;
  }

  function closeManage(): void {
    managing = false;
  }

  function requestDelete(tag: TagPickerTag): void {
    deleteError = null;
    deleteTarget = tag;
  }

  function cancelDelete(): void {
    deleteTarget = null;
  }

  async function confirmDelete(): Promise<void> {
    if (!deleteTarget) return;
    deleting = true;
    const result = await onDeleteTag(deleteTarget.id);
    deleting = false;
    if (result.status === 'error') {
      deleteError = result.message;
      deleteTarget = null;
      return;
    }
    deleteTarget = null;
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      onClose();
    }
  }

  function handleWindowPointerDown(event: PointerEvent): void {
    // `ConfirmDialog` portals to `document.body` (outside `rootEl`'s own DOM
    // subtree) -- a click anywhere inside it (Huỷ/Xoá tag/backdrop) would
    // otherwise look like "outside" and close the whole picker out from
    // under it. Its own Cancel/Confirm/Escape handling is what dismisses it.
    if (deleteTarget) return;
    const target = event.target as Node | null;
    if (rootEl && target && !rootEl.contains(target)) onClose();
  }
</script>

<svelte:window onpointerdown={handleWindowPointerDown} />

<div
  class="tag-picker"
  bind:this={rootEl}
  use:floating={anchor}
  role="dialog"
  aria-label={dialogLabel}
  tabindex="-1"
  onkeydown={handleKeydown}
>
  {#if managing}
    <div class="tag-picker-manage-header">
      <button type="button" class="tag-picker-back" onclick={closeManage} aria-label={i18n.t('tagPicker.manage.back')}>
        <ArrowLeftIcon size={16} strokeWidth={1.75} aria-hidden="true" />
        <span>{i18n.t('tagPicker.manage.title')}</span>
      </button>
    </div>
    {#if deleteError}
      <p class="tag-picker-error" role="alert">{deleteError}</p>
    {/if}
    <div class="tag-picker-manage-list">
      {#if tags.length === 0}
        <p class="tag-picker-empty">{i18n.t('tagPicker.manage.empty')}</p>
      {:else}
        {#each tags as tag (tag.id)}
          <div class="tag-picker-manage-row">
            <span class="tag-picker-manage-name">{tag.name}</span>
            <span class="tag-picker-count mono">{tag.sessionCount}</span>
            <button
              type="button"
              class="tag-picker-delete"
              aria-label={i18n.t('tagPicker.manage.deleteLabel', { name: tag.name })}
              onclick={() => requestDelete(tag)}
            >
              <Trash2Icon size={16} strokeWidth={1.75} aria-hidden="true" />
            </button>
          </div>
        {/each}
      {/if}
    </div>
  {:else}
    <div class="tag-picker-search">
      <span class="tag-picker-search-icon" aria-hidden="true">
        <SearchIcon size={16} strokeWidth={1.75} />
      </span>
      <input
        bind:this={searchInputEl}
        type="search"
        class="tag-picker-search-input"
        aria-label={i18n.t('tagPicker.search.label')}
        placeholder={i18n.t('tagPicker.search.placeholder')}
        maxlength={MAX_TAG_NAME_SCALARS}
        disabled={creating}
        bind:value={query}
        onkeydown={handleSearchKeydown}
      />
    </div>

    {#if error}
      <p class="tag-picker-error" role="alert">{error}</p>
    {:else if atLimit}
      <p class="tag-picker-error" role="status">{i18n.t('tagPicker.error.limit')}</p>
    {/if}

    <div class="tag-picker-list">
      {#if selectedTags.length > 0}
        <div class="tag-picker-section-label">
          {i18n.t(mode === 'filter' ? 'tagPicker.section.filtering' : 'tagPicker.section.selected', {
            count: selectedTags.length,
          })}
        </div>
        {#each selectedTags as tag (tag.id)}
          <label class="tag-picker-row">
            <input
              type="checkbox"
              checked
              disabled={busyTagId === tag.id}
              onchange={() => toggle(tag.id)}
            />
            <span class="tag-picker-row-name">{tag.name}</span>
            <span class="tag-picker-count mono">{tag.sessionCount}</span>
          </label>
        {/each}
      {/if}

      {#if tags.length === 0}
        <p class="tag-picker-empty">{i18n.t('tagPicker.empty.first')}</p>
      {:else}
        {#if restTags.length > 0 || selectedTags.length === 0}
          <div class="tag-picker-section-label">
            {i18n.t('tagPicker.section.all', { count: tags.length })}
          </div>
        {/if}
        {#if restTags.length === 0 && normalizedQuery !== '' && exactMatch === null}
          <p class="tag-picker-empty">{i18n.t('tagPicker.empty.noMatch')}</p>
        {/if}
        {#each restTags as tag (tag.id)}
          <label class="tag-picker-row">
            <input
              type="checkbox"
              disabled={busyTagId === tag.id || atLimit}
              onchange={() => toggle(tag.id)}
            />
            <span class="tag-picker-row-name">{tag.name}</span>
            <span class="tag-picker-count mono">{tag.sessionCount}</span>
          </label>
        {/each}
      {/if}

      {#if canCreate}
        <button type="button" class="tag-picker-create" disabled={creating || atLimit} onclick={submitSearch}>
          <TagIcon size={14} strokeWidth={1.75} aria-hidden="true" />
          {i18n.t('tagPicker.action.create', { name: query.trim() })}
        </button>
      {/if}
    </div>

    <div class="tag-picker-footer">
      <span class="tag-picker-hint">{i18n.t('tagPicker.footer.hint')}</span>
      <button type="button" class="tag-picker-manage-link" onclick={openManage}>
        {i18n.t('tagPicker.footer.manage')}
      </button>
    </div>
  {/if}
</div>

{#if deleteTarget}
  <ConfirmDialog
    title={i18n.t('tagPicker.deleteDialog.title', { name: deleteTarget.name })}
    body={i18n.t('tagPicker.deleteDialog.body')}
    confirmLabel={deleting ? i18n.t('tagPicker.deleteDialog.deleting') : i18n.t('tagPicker.deleteDialog.confirm')}
    cancelLabel={i18n.t('tagPicker.deleteDialog.cancel')}
    confirming={deleting}
    onConfirm={confirmDelete}
    onCancel={cancelDelete}
  />
{/if}

<style>
  .tag-picker {
    position: absolute;
    top: calc(100% + var(--space-2));
    right: 0;
    z-index: 30;
    display: flex;
    width: 320px;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
    background: var(--color-surface);
    /* `check:ui` resolves `box-shadow` only against tokens defined in the
       same file — the literal allow-listed value (== `--shadow-floating` in
       tokens.css) matches `ConfirmDialog.svelte`'s existing convention. */
    box-shadow: 0 10px 32px rgb(17 24 39 / 12%);
  }

  .tag-picker-search {
    position: relative;
    display: flex;
    padding: var(--space-2);
    border-bottom: 1px solid var(--color-border);
  }

  .tag-picker-search-icon {
    position: absolute;
    top: 50%;
    left: calc(var(--space-2) + 10px);
    display: flex;
    transform: translateY(-50%);
    color: var(--color-text-muted);
    pointer-events: none;
  }

  .tag-picker-search-input {
    width: 100%;
    height: 36px;
    padding: 0 var(--space-3) 0 34px;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-body-size);
  }

  .tag-picker-search-input:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }

  .tag-picker-error {
    margin: 0;
    padding: var(--space-2) var(--space-3) 0;
    color: var(--color-danger-strong);
    font-size: var(--text-help-size);
  }

  .tag-picker-list {
    display: flex;
    max-height: 280px;
    flex-direction: column;
    gap: 2px;
    overflow-y: auto;
    padding: var(--space-2);
  }

  .tag-picker-section-label {
    padding: var(--space-2) var(--space-2) 4px;
    color: var(--color-text-muted);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .tag-picker-row {
    display: flex;
    height: 36px;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-2);
    border-radius: var(--radius-md);
    cursor: pointer;
  }

  .tag-picker-row:hover {
    background: var(--color-surface-sunken);
  }

  .tag-picker-row input[type='checkbox'] {
    width: 16px;
    height: 16px;
    margin: 0;
    accent-color: var(--color-accent);
  }

  .tag-picker-row-name {
    overflow: hidden;
    flex: 1;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-body-size);
  }

  .tag-picker-count {
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
  }

  .tag-picker-empty {
    margin: 0;
    padding: var(--space-3) var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
  }

  .tag-picker-create {
    display: flex;
    height: 36px;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-2);
    border: none;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-accent);
    font: inherit;
    font-size: var(--text-body-size);
    font-weight: 500;
    text-align: left;
    cursor: pointer;
  }

  .tag-picker-create:hover {
    background: var(--color-accent-soft);
  }

  .tag-picker-create:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }

  .tag-picker-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-3);
    border-top: 1px solid var(--color-border);
    background: var(--color-surface-sunken);
  }

  .tag-picker-hint {
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
  }

  .tag-picker-manage-link {
    flex: 0 0 auto;
    padding: 0;
    border: none;
    background: none;
    color: var(--color-accent);
    font: inherit;
    font-size: var(--text-label-size);
    font-weight: 600;
    cursor: pointer;
  }

  .tag-picker-manage-header {
    display: flex;
    padding: var(--space-2);
    border-bottom: 1px solid var(--color-border);
  }

  .tag-picker-back {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border: none;
    background: none;
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-body-size);
    font-weight: 600;
    cursor: pointer;
  }

  .tag-picker-manage-list {
    display: flex;
    max-height: 320px;
    flex-direction: column;
    overflow-y: auto;
    padding: var(--space-2);
  }

  .tag-picker-manage-row {
    display: flex;
    height: 36px;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-2);
    border-radius: var(--radius-md);
  }

  .tag-picker-manage-row:hover {
    background: var(--color-surface-sunken);
  }

  .tag-picker-manage-name {
    overflow: hidden;
    flex: 1;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-body-size);
  }

  .tag-picker-delete {
    display: grid;
    width: 28px;
    height: 28px;
    flex: 0 0 auto;
    place-items: center;
    border: none;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-danger-strong);
    cursor: pointer;
  }

  .tag-picker-delete:hover {
    background: var(--color-danger-soft);
  }

  .tag-picker-back:focus-visible,
  .tag-picker-manage-link:focus-visible,
  .tag-picker-delete:focus-visible,
  .tag-picker-create:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }
</style>
