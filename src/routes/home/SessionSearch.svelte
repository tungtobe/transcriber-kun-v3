<script lang="ts">
  // Ô tìm theo tên ở header Home (story 3.3, spec Code Map: có thể tách khỏi
  // `Home.svelte`). Đọc/ghi thẳng `libraryStore.nameQuery` -- component này
  // chỉ vẽ UI, không tự lọc (logic AND với tag nằm trong
  // `filterSessions`/`libraryStore.filteredSessions`).
  //
  // `focusAndSelect` được `Home.svelte` gọi qua `bind:this` khi `⌘F`/`Ctrl+F`
  // kích hoạt (spec Always: "focus + chọn nội dung ô tìm").
  import { SearchIcon, XIcon } from '../../components/icons';
  import { i18n } from '../../i18n/index.svelte';
  import { libraryStore } from '../../lib/stores/library.svelte';

  let inputEl = $state<HTMLInputElement | null>(null);

  export function focusAndSelect(): void {
    inputEl?.focus();
    inputEl?.select();
  }

  function handleInput(event: Event): void {
    libraryStore.setNameQuery((event.currentTarget as HTMLInputElement).value);
  }

  // Nút × -- chỉ hiện khi có query, trả focus về ô sau khi xoá (spec Always).
  function clearQuery(): void {
    libraryStore.clearNameQuery();
    inputEl?.focus();
  }

  // Esc trong ô khi có query xoá query nhưng không blur/đóng gì khác (spec
  // Always: "giữ nguyên bộ lọc tag") -- không preventDefault khi query đã
  // rỗng, để Esc giữ hành vi mặc định của trình duyệt/WebView.
  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape' && libraryStore.nameQuery.length > 0) {
      event.preventDefault();
      event.stopPropagation();
      clearQuery();
    }
  }
</script>

<div class="session-search">
  <span class="session-search-icon" aria-hidden="true"><SearchIcon size={16} strokeWidth={1.75} /></span>
  <label for="home-session-search-input" class="sr-only">{i18n.t('home.search.label')}</label>
  <input
    bind:this={inputEl}
    id="home-session-search-input"
    type="text"
    class="session-search-input"
    placeholder={i18n.t('home.search.placeholder')}
    value={libraryStore.nameQuery}
    oninput={handleInput}
    onkeydown={handleKeydown}
  />
  {#if libraryStore.nameQuery.length > 0}
    <button
      type="button"
      class="session-search-clear"
      aria-label={i18n.t('home.search.clearLabel')}
      onclick={clearQuery}
    >
      <XIcon size={14} strokeWidth={2} aria-hidden="true" />
    </button>
  {/if}
</div>

<style>
  .session-search {
    position: relative;
    display: flex;
    flex: 1 1 260px;
    align-items: center;
    max-width: 360px;
    min-height: 36px;
  }

  .session-search-icon {
    position: absolute;
    left: 10px;
    display: grid;
    place-items: center;
    color: var(--color-text-muted);
    pointer-events: none;
  }

  .session-search-input {
    width: 100%;
    min-height: 36px;
    padding: 0 32px 0 32px;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-body-size);
  }

  .session-search-input:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }

  .session-search-clear {
    position: absolute;
    right: 6px;
    display: grid;
    width: 24px;
    height: 24px;
    place-items: center;
    border: none;
    border-radius: var(--radius-full);
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .session-search-clear:hover {
    background: var(--color-surface-sunken);
    color: var(--color-text-secondary);
  }

  .session-search-clear:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
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
