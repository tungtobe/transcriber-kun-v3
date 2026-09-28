<script lang="ts">
  import { untrack } from 'svelte';
  // Ghi chú tự lưu của một Phiên (story 3.5, spec Approach): textarea thuần
  // (spec Never: "Không làm Markdown/rich text") tự lưu qua `notesStore` sau
  // debounce 800 ms, dòng trạng thái "Đang lưu…"/"Đã lưu hh:mm"/"Chưa lưu" +
  // Thử lại. Component chỉ nhận `sessionId` (spec Code Map: "chỉ nhận
  // `sessionId` (+ tuỳ chọn) để Live tái dùng nguyên vẹn") — mọi state khác
  // (buffer, revision, debounce) sống trong `notesStore`, không ở đây.
  //
  // `$effect` bên dưới load khi mount HOẶC khi `sessionId` đổi (Session.svelte
  // giữ nguyên component qua hai Phiên `saved` liên tiếp, không destroy/tạo
  // lại) và flush Phiên cũ trước khi chuyển; cleanup của chính nó flush lúc
  // unmount thật sự (rời route) -- ba trong bốn thời điểm flush ở spec
  // Boundaries Always. Thời điểm thứ tư ("đổi tab panel") do `Session.svelte`
  // gọi `flush()` xuất khẩu dưới đây, vì tab strip chỉ ẩn/hiện bằng CSS
  // (component không unmount khi đổi tab).
  import { i18n } from '../i18n/index.svelte';
  import { formatLocalTime } from '../lib/time';
  import { notesStore, MAX_NOTE_BODY_LENGTH } from '../lib/stores/notes.svelte';

  let { sessionId }: { sessionId: string } = $props();

  $effect(() => {
    const id = sessionId;
    // `untrack`: `load()` đọc `entries` đồng bộ (kiểm buffer chưa lưu) -- nếu
    // bị theo dõi, effect vừa đọc vừa ghi `entries` và tự chạy lại vô hạn.
    untrack(() => void notesStore.load(id));
    return () => {
      void notesStore.flush(id);
    };
  });

  const view = $derived(notesStore.view(sessionId));

  const statusText = $derived.by(() => {
    if (view.status === 'saving') return i18n.t('notesPanel.status.saving');
    if (view.status === 'error') return i18n.t('notesPanel.status.unsaved');
    if (view.status === 'saved' && view.savedAtMs !== null) {
      return i18n.t('notesPanel.status.saved', {
        time: formatLocalTime(view.savedAtMs, i18n.locale),
      });
    }
    return '';
  });

  function handleInput(event: Event): void {
    notesStore.setBody(sessionId, (event.currentTarget as HTMLTextAreaElement).value);
  }

  async function handleRetry(): Promise<void> {
    await retry();
  }

  async function handleReload(): Promise<void> {
    await notesStore.load(sessionId);
  }

  /** Lưu bền ngay -- xuất khẩu qua `bind:this` cho `Session.svelte` (đổi tab)
   * và, sau này, Live ("lưu bền trước khi Phiên finalize", spec Code Map). */
  export async function flush(): Promise<boolean> {
    return notesStore.flush(sessionId);
  }

  /** Retries a previously failed save so Live can wait for its acknowledgement
   * before continuing the stop flow. */
  export async function retry(): Promise<boolean> {
    return notesStore.retry(sessionId);
  }
</script>

<div class="notes-panel">
  {#if view.loadError}
    <p class="notes-panel-load-error" role="alert">
      {i18n.t('notesPanel.status.loadError')}
      <button type="button" class="notes-panel-link-button" onclick={handleReload}>
        {i18n.t('notesPanel.status.reload')}
      </button>
    </p>
  {:else}
    <label for="notes-panel-textarea" class="notes-panel-label">
      {i18n.t('notesPanel.field.label')}
    </label>
    <textarea
      id="notes-panel-textarea"
      class="notes-panel-textarea"
      value={view.body}
      maxlength={MAX_NOTE_BODY_LENGTH}
      placeholder={i18n.t('notesPanel.field.placeholder')}
      oninput={handleInput}
    ></textarea>
    <p class="notes-panel-status" role="status" aria-live="polite">
      {statusText}
      {#if view.status === 'error'}
        <button type="button" class="notes-panel-link-button" onclick={handleRetry}>
          {i18n.t('notesPanel.status.retry')}
        </button>
      {/if}
    </p>
  {/if}
</div>

<style>
  .notes-panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    height: 100%;
    min-height: 0;
  }

  .notes-panel-label {
    color: var(--color-text-secondary);
    font-size: var(--text-label-size);
    font-weight: 600;
  }

  .notes-panel-textarea {
    flex: 1;
    min-height: 160px;
    padding: var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
    resize: vertical;
  }

  .notes-panel-textarea:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }

  .notes-panel-status {
    min-height: 1.2em;
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
  }

  .notes-panel-load-error {
    margin: 0;
    color: var(--color-danger);
    font-size: var(--text-help-size);
  }

  .notes-panel-link-button {
    margin-left: var(--space-2);
    padding: 0;
    border: none;
    background: none;
    color: var(--color-accent);
    font: inherit;
    font-size: var(--text-help-size);
    text-decoration: underline;
    cursor: pointer;
  }
</style>
