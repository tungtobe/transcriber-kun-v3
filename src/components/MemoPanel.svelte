<script lang="ts">
  // Panel Memo (story 3.7, spec Approach): tab thứ ba "Memo" trong aside
  // Session detail -- chọn Template, "Sinh"/"Sinh lại", trạng thái đang
  // sinh + Huỷ, dòng nguồn, nhãn "bản trước"/"ghi chú đã đổi", Copy/Tải .md,
  // lỗi inline theo category. Component chỉ nhận `sessionId` + vài cờ đầu
  // vào (transcript) từ `Session.svelte` -- mọi state sinh/hiển thị sống ở
  // `memoStore` (singleton, cùng lý do `NotesPanel`/`notesStore`).
  //
  // `Session.svelte` mount panel này thường trực (ẩn/hiện bằng CSS, không
  // unmount khi đổi tab, cùng mẫu `NotesPanel`) -- `active` cho biết tab này
  // có đang thật sự hiển thị hay không, để `memoStore` biết khi nào cần bắn
  // toast thay vì chỉ âm thầm cập nhật state (spec Boundaries Always).
  import { commands } from '../lib/bindings';
  import { i18n } from '../i18n/index.svelte';
  import { errorHint, errorTitle } from '../lib/errors';
  import { isHttpUrl, renderMemoMarkdown } from '../lib/markdown';
  import { formatLocalTime } from '../lib/time';
  import { keysStore } from '../lib/stores/keys.svelte';
  import { memoStore } from '../lib/stores/memo.svelte';
  import { memoTemplatesStore } from '../lib/stores/memoTemplates.svelte';
  import { notesStore } from '../lib/stores/notes.svelte';
  import { settingsStore } from '../lib/stores/settings.svelte';
  import Badge from './Badge.svelte';
  import DisabledHint from './DisabledHint.svelte';

  let {
    sessionId,
    active,
    hasTranscript,
    segmentTextCount,
    onMemoAvailable,
  }: {
    sessionId: string;
    active: boolean;
    hasTranscript: boolean;
    segmentTextCount: number;
    /** Gọi mỗi khi panel đang hiện một memo (đã cache hoặc vừa sinh xong) --
     * `Session.svelte` dùng để cập nhật badge `memo` ngay, không cần tải lại
     * `librarySessionDetail` (spec Boundaries Always: "Badge `memo` ... khi
     * Phiên có ít nhất một memo"). */
    onMemoAvailable?: () => void;
  } = $props();

  let selectedTemplateId = $state<string | null>(null);
  let exportBusy = $state(false);
  let exportMessage = $state<string | null>(null);

  $effect(() => {
    void keysStore.load();
  });

  $effect(() => {
    void memoTemplatesStore.load(i18n.locale);
  });

  // Mặc định chọn mẫu đầu tiên khi danh sách tới, không tự đổi lựa chọn của
  // người dùng ở lần render sau (chỉ đặt khi chưa chọn gì, hoặc mẫu đang
  // chọn không còn trong danh sách -- ví dụ vừa bị xoá).
  $effect(() => {
    const templates = memoTemplatesStore.templates;
    if (templates.length === 0) return;
    if (selectedTemplateId && templates.some((t) => t.id === selectedTemplateId)) return;
    selectedTemplateId = templates[0].id;
  });

  $effect(() => {
    const id = selectedTemplateId;
    if (!id) return;
    void memoStore.load(sessionId, id);
  });

  $effect(() => {
    if (active && selectedTemplateId) {
      memoStore.setVisible(sessionId, selectedTemplateId);
    } else {
      memoStore.setVisible(null, null);
    }
    return () => memoStore.setVisible(null, null);
  });

  const view = $derived(selectedTemplateId ? memoStore.view(sessionId, selectedTemplateId) : { status: 'idle' as const, memo: null, error: null });

  $effect(() => {
    if (view.memo) onMemoAvailable?.();
  });

  const disabledReason = $derived.by(() => {
    if (settingsStore.consentStatus !== 'current') return i18n.t('memoPanel.disabled.consent');
    if (!(keysStore.status === 'ready' && keysStore.hasUsableKey)) return i18n.t('memoPanel.disabled.key');
    if (!hasTranscript) return i18n.t('memoPanel.disabled.noTranscript');
    if (segmentTextCount === 0) return i18n.t('memoPanel.disabled.onlyGaps');
    return null;
  });

  const sourceLine = $derived.by(() => {
    const memo = view.memo;
    if (!memo) return null;
    const variant = memo.fromPreviousTranscript
      ? i18n.t('memoPanel.source.variantRerun')
      : i18n.t('memoPanel.source.variantPrimary');
    const status = memo.transcriptStatus === 'partial'
      ? i18n.t('memoPanel.source.statusPartial')
      : i18n.t('memoPanel.source.statusComplete');
    const notes = memo.usesNotes ? ` ${i18n.t('memoPanel.source.withNotes')}` : '';
    const time = memo.createdAt !== null ? formatLocalTime(memo.createdAt, i18n.locale) : '';
    return i18n.t('memoPanel.source.line', { variant, status, notes, time });
  });

  const renderedBody = $derived(view.memo ? renderMemoMarkdown(view.memo.body) : '');

  async function handleGenerate(): Promise<void> {
    if (disabledReason || view.status === 'generating' || !selectedTemplateId) return;
    exportMessage = null;
    // Spec Boundaries Always: "Trước khi sinh, flush ghi chú của Phiên".
    await notesStore.flush(sessionId);
    await memoStore.generate(sessionId, selectedTemplateId, i18n.locale);
  }

  async function handleCancel(): Promise<void> {
    if (!selectedTemplateId) return;
    await memoStore.cancel(sessionId, selectedTemplateId);
  }

  async function handleCopy(): Promise<void> {
    if (!view.memo) return;
    try {
      await navigator.clipboard.writeText(view.memo.body);
      exportMessage = i18n.t('memoPanel.export.copySuccess');
    } catch {
      exportMessage = i18n.t('memoPanel.export.copyError');
    }
  }

  async function handleExport(): Promise<void> {
    if (!selectedTemplateId || exportBusy) return;
    exportBusy = true;
    exportMessage = null;
    try {
      const result = await commands.memoExport(sessionId, selectedTemplateId);
      if (result.status !== 'ok') {
        exportMessage = i18n.t('memoPanel.export.exportError');
        return;
      }
      if (result.data) exportMessage = i18n.t('memoPanel.export.exportSuccess');
    } catch {
      exportMessage = i18n.t('memoPanel.export.exportError');
    } finally {
      exportBusy = false;
    }
  }

  // Spec Boundaries Always: "link: chặn click, chỉ mở URL http(s) qua lệnh
  // Rust `open_external_url`". Chặn *mọi* click trên một thẻ `<a>` (kể cả
  // scheme khác http/https) -- chỉ gọi lệnh khi href thật sự http(s).
  function handleBodyClick(event: MouseEvent): void {
    const target = event.target as HTMLElement | null;
    const anchor = target?.closest('a');
    if (!anchor) return;
    event.preventDefault();
    const href = anchor.getAttribute('href');
    if (href && isHttpUrl(href)) {
      void commands.openExternalUrl(href);
    }
  }
</script>

<div class="memo-panel">
  <div class="memo-panel-controls">
    <label for="memo-panel-template" class="memo-panel-label">{i18n.t('memoPanel.field.templateLabel')}</label>
    <select
      id="memo-panel-template"
      class="memo-panel-select"
      value={selectedTemplateId ?? ''}
      onchange={(event) => (selectedTemplateId = (event.currentTarget as HTMLSelectElement).value)}
    >
      {#each memoTemplatesStore.templates as template (template.id)}
        <option value={template.id}>{template.name}</option>
      {/each}
    </select>

    <div class="memo-panel-generate-row">
      {#if view.status === 'generating'}
        <span class="memo-panel-generating" role="status" aria-live="polite">
          {i18n.t('memoPanel.state.generating')}
        </span>
        <button type="button" class="button button-secondary" onclick={handleCancel}>
          {i18n.t('memoPanel.action.cancel')}
        </button>
      {:else if disabledReason}
        <DisabledHint reason={disabledReason}>
          <span class="button button-primary">
            {view.memo ? i18n.t('memoPanel.action.regenerate') : i18n.t('memoPanel.action.generate')}
          </span>
        </DisabledHint>
      {:else}
        <button type="button" class="button button-primary" onclick={handleGenerate}>
          {view.memo ? i18n.t('memoPanel.action.regenerate') : i18n.t('memoPanel.action.generate')}
        </button>
      {/if}
      <Badge variant="token" label={i18n.t('memoPanel.field.tokenBadge')} />
    </div>
  </div>

  {#if view.error}
    <p class="memo-panel-error" role="alert">
      <strong>{errorTitle(view.error)}</strong> {errorHint(view.error)}
      <button type="button" class="memo-panel-link-button" onclick={handleGenerate}>
        {i18n.t('memoPanel.action.retry')}
      </button>
    </p>
  {/if}

  {#if view.memo}
    {#if sourceLine}
      <p class="memo-panel-source">{sourceLine}</p>
    {/if}
    {#if view.memo.fromPreviousTranscript}
      <p class="memo-panel-hint">{i18n.t('memoPanel.hint.staleTranscript')}</p>
    {/if}
    {#if view.memo.notesChanged && view.memo.usesNotes}
      <p class="memo-panel-hint">{i18n.t('memoPanel.hint.notesChanged')}</p>
    {/if}

    <div class="memo-panel-actions">
      <button type="button" class="button button-secondary" onclick={handleCopy}>
        {i18n.t('memoPanel.action.copy')}
      </button>
      <button type="button" class="button button-secondary" disabled={exportBusy} onclick={handleExport}>
        {i18n.t('memoPanel.action.download')}
      </button>
    </div>
    {#if exportMessage}
      <p class="memo-panel-export-message" role="status" aria-live="polite">{exportMessage}</p>
    {/if}

    <!-- Sự kiện click chỉ để bắt sự kiện nổi bọt từ các thẻ `<a>` bên trong
         HTML đã sanitize (event delegation) -- bản thân link đã là phần tử
         tương tác/điều hướng bàn phím được, div bọc ngoài không cần thêm
         role/tabindex. -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- eslint-disable-next-line svelte/no-at-html-tags -->
    <div class="memo-panel-body" onclick={handleBodyClick}>{@html renderedBody}</div>
  {:else if view.status !== 'generating'}
    <p class="memo-panel-empty">{i18n.t('memoPanel.state.empty')}</p>
  {/if}
</div>

<style>
  .memo-panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-height: 0;
    overflow-y: auto;
  }

  .memo-panel-controls {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .memo-panel-label {
    color: var(--color-text-secondary);
    font-size: var(--text-label-size);
    font-weight: 600;
  }

  .memo-panel-select {
    min-height: 34px;
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
  }

  .memo-panel-generate-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .memo-panel-generating {
    color: var(--color-text-secondary);
    font-size: var(--text-help-size);
  }

  .memo-panel-error {
    margin: 0;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    background: var(--color-danger-soft);
    color: var(--color-danger-strong);
    font-size: var(--text-help-size);
  }

  .memo-panel-source,
  .memo-panel-hint,
  .memo-panel-export-message {
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
  }

  .memo-panel-hint {
    color: var(--color-warning);
  }

  .memo-panel-empty {
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--text-body-size);
  }

  .memo-panel-actions {
    display: flex;
    gap: var(--space-2);
  }

  .memo-panel-link-button {
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

  .memo-panel-body {
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    font-size: var(--text-body-size);
    line-height: 1.5;
    overflow-wrap: anywhere;
  }

  .memo-panel-body :global(h1),
  .memo-panel-body :global(h2),
  .memo-panel-body :global(h3) {
    margin: var(--space-3) 0 var(--space-2);
  }

  .memo-panel-body :global(p) {
    margin: 0 0 var(--space-2);
  }

  .memo-panel-body :global(ul),
  .memo-panel-body :global(ol) {
    margin: 0 0 var(--space-2);
    padding-left: var(--space-5);
  }

  .memo-panel-body :global(a) {
    color: var(--color-accent);
  }

  .button {
    display: inline-flex;
    width: fit-content;
    min-height: 34px;
    align-items: center;
    justify-content: center;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-label-size);
    font-weight: 500;
    cursor: pointer;
  }

  .button:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }

  .button-primary {
    border-color: var(--color-primary-action);
    background: var(--color-primary-action);
    color: var(--color-on-primary);
  }

  .button-secondary {
    border-color: var(--color-border-strong);
    background: var(--color-surface);
    color: var(--color-text);
  }
</style>
