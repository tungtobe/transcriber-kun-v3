<script lang="ts">
  // Settings → Memo (story 3.6, spec Approach): master-detail -- danh sách
  // trái (2 mặc định của locale UI hiện tại + mọi mẫu người dùng, badge
  // "Mặc định · vi|en|ja" hoặc "Của bạn") và editor tên + prompt bên phải
  // (`memo/TemplateEditor.svelte`). Nạp lại khi `i18n.locale` đổi (spec
  // Acceptance: "đổi ngôn ngữ UI khi đang ở nhóm Memo ... hiện mặc định của
  // ngôn ngữ mới và mẫu người dùng, không nhân bản").
  //
  // "Bỏ thay đổi?" (đổi mẫu đang chọn/mở "+ Thêm mẫu" khi editor có thay đổi
  // chưa lưu) và "Khôi phục mẫu mặc định" đều xác nhận inline ngay trong panel
  // này (spec Never: "Không dùng dialog modal cho xoá/khôi phục") -- component
  // con `TemplateEditor` chỉ báo `dirty` lên đây qua `onDirtyChange`, mọi
  // quyết định "có nên chuyển mẫu không" nằm ở đây vì chỉ đây mới biết đích
  // đến (mẫu khác hay bản nháp mới).
  import { errorHint, errorTitle } from '../../lib/errors';
  import { i18n } from '../../i18n/index.svelte';
  import BannerStack from '../../components/BannerStack.svelte';
  import type { BannerItem } from '../../components/BannerStack.svelte';
  import Badge from '../../components/Badge.svelte';
  import { memoTemplatesStore } from '../../lib/stores/memoTemplates.svelte';
  import TemplateEditor from './memo/TemplateEditor.svelte';
  import type { AppError, MemoTemplate } from '../../lib/bindings';

  type Mode = 'existing' | 'draft';
  type PendingAction = { type: 'select'; id: string } | { type: 'draft' };

  let mode = $state<Mode>('existing');
  let selectedId = $state<string | null>(null);
  let draftSeed = $state<{ name: string; prompt: string } | null>(null);
  let editorDirty = $state(false);
  let pendingAction = $state<PendingAction | null>(null);
  // Ép remount editor (qua `{#key}`) kể cả khi `selectedId` không đổi --
  // "Khôi phục mẫu mặc định" có thể sửa nội dung của đúng mẫu đang mở mà
  // không đổi id của nó.
  let restoreRevision = $state(0);

  let restoreConfirming = $state(false);
  let restoring = $state(false);
  let restoreError = $state<AppError | null>(null);

  let loadedLocale: string | null = null;

  const templates = $derived(memoTemplatesStore.templates);
  const selectedTemplate = $derived(templates.find((row) => row.id === selectedId) ?? null);

  $effect(() => {
    const locale = i18n.locale;
    if (loadedLocale === locale) return;
    loadedLocale = locale;
    void memoTemplatesStore.load(locale);
  });

  // Giữ `selectedId` luôn trỏ tới một dòng có thật trong `templates` -- chạy
  // lại mỗi khi `templates` đổi (load xong, tạo/xoá mẫu, khôi phục mặc định)
  // thay vì tự suy luận riêng sau từng thao tác. Tách khỏi effect nạp locale
  // ở trên để việc chọn dòng đầu tiên luôn cùng một chu kỳ flush với chính
  // `templates` vừa đổi -- gộp state trong một async callback (sau `await`)
  // từng khiến `findByText` của test thấy danh sách trước khi DOM của editor
  // kịp cập nhật (một nhịp reactive riêng, trễ so với danh sách).
  $effect(() => {
    const rows = templates;
    if (mode !== 'existing' || rows.length === 0) return;
    if (!rows.some((row) => row.id === selectedId)) {
      selectedId = rows[0].id;
    }
  });

  const loadBanners = $derived<BannerItem[]>(
    memoTemplatesStore.status === 'error' && memoTemplatesStore.error
      ? [{
          id: 'memo-templates-load-error',
          variant: 'warning',
          title: errorTitle(memoTemplatesStore.error),
          message: errorHint(memoTemplatesStore.error),
        }]
      : [],
  );
  const restoreBanners = $derived<BannerItem[]>(
    restoreError
      ? [{ id: 'memo-templates-restore-error', variant: 'warning', title: errorTitle(restoreError), message: errorHint(restoreError) }]
      : [],
  );

  function requestSelect(id: string): void {
    if (mode === 'existing' && selectedId === id) return;
    if (editorDirty) {
      pendingAction = { type: 'select', id };
      return;
    }
    applySelect(id);
  }

  function applySelect(id: string): void {
    selectedId = id;
    mode = 'existing';
    pendingAction = null;
  }

  function requestDraft(): void {
    if (editorDirty) {
      pendingAction = { type: 'draft' };
      return;
    }
    applyDraft();
  }

  function applyDraft(): void {
    mode = 'draft';
    selectedId = null;
    draftSeed = {
      name: i18n.t('memoTemplates.editor.newDraftName'),
      prompt: i18n.t('memoTemplates.editor.newDraftPrompt'),
    };
    pendingAction = null;
  }

  function confirmPending(): void {
    if (!pendingAction) return;
    if (pendingAction.type === 'select') {
      applySelect(pendingAction.id);
    } else {
      applyDraft();
    }
  }

  function cancelPending(): void {
    pendingAction = null;
  }

  function handleSaved(saved: MemoTemplate): void {
    selectedId = saved.id;
    mode = 'existing';
  }

  /** Không cần tự chọn lại ở đây -- `selectedId` (id vừa xoá) hết hợp lệ
   * ngay khi `templates` mất dòng đó, effect "giữ `selectedId` hợp lệ" ở
   * trên tự chọn dòng đầu tiên trong cùng chu kỳ flush. */
  function handleDeleted(_id: string): void {}

  function startRestore(): void {
    restoreError = null;
    restoreConfirming = true;
  }

  function cancelRestore(): void {
    restoreConfirming = false;
  }

  async function confirmRestore(): Promise<void> {
    restoring = true;
    const result = await memoTemplatesStore.restoreDefaults(i18n.locale);
    restoring = false;
    restoreConfirming = false;
    if (result.status !== 'ok') {
      restoreError = result.error;
      return;
    }
    // Khôi phục không bao giờ xoá dòng nào (chỉ ghi lại tên/prompt gốc của
    // 2 mặc định) nên `selectedId` luôn còn hợp lệ -- chỉ cần ép remount để
    // editor (nếu đang mở đúng mẫu mặc định đó) hiển thị nội dung mới.
    restoreRevision += 1;
  }
</script>

<div class="memo-layout">
  <div class="memo-list-panel">
    <div class="memo-list-actions">
      <button type="button" onclick={requestDraft}>{i18n.t('memoTemplates.list.addButton')}</button>
      <button type="button" disabled={restoring} onclick={startRestore}>
        {i18n.t('memoTemplates.list.restoreButton')}
      </button>
    </div>

    <ul class="memo-list" aria-label={i18n.t('memoTemplates.list.ariaLabel')}>
      {#each templates as row (row.id)}
        <li>
          <button
            type="button"
            class="memo-list-row"
            class:active={mode === 'existing' && selectedId === row.id}
            aria-current={mode === 'existing' && selectedId === row.id ? 'true' : undefined}
            onclick={() => requestSelect(row.id)}
          >
            <span class="memo-list-name">{row.name}</span>
            {#if row.isDefault}
              <Badge variant="memo" label={i18n.t('memoTemplates.list.badgeDefault', { locale: row.locale ?? '' })} />
            {:else}
              <Badge variant="token" label={i18n.t('memoTemplates.list.badgeCustom')} />
            {/if}
          </button>
        </li>
      {/each}
    </ul>

    <BannerStack banners={loadBanners} />

    {#if restoreConfirming}
      <div class="inline-confirm">
        <p>{i18n.t('memoTemplates.list.restoreConfirm')}</p>
        <div class="inline-confirm-actions">
          <button type="button" disabled={restoring} onclick={() => void confirmRestore()}>
            {i18n.t('memoTemplates.list.restoreConfirmButton')}
          </button>
          <button type="button" disabled={restoring} onclick={cancelRestore}>
            {i18n.t('memoTemplates.list.restoreCancelButton')}
          </button>
        </div>
        <BannerStack banners={restoreBanners} />
      </div>
    {/if}

    {#if pendingAction}
      <div class="inline-confirm">
        <p>{i18n.t('memoTemplates.list.discardConfirm')}</p>
        <div class="inline-confirm-actions">
          <button type="button" onclick={confirmPending}>{i18n.t('memoTemplates.list.discardConfirmButton')}</button>
          <button type="button" onclick={cancelPending}>{i18n.t('memoTemplates.list.discardCancelButton')}</button>
        </div>
      </div>
    {/if}
  </div>

  <div class="memo-editor-panel">
    {#key `${mode}:${selectedId ?? 'draft'}:${restoreRevision}`}
      {#if mode === 'draft' || selectedTemplate}
        <TemplateEditor
          template={mode === 'draft' ? null : selectedTemplate}
          draftSeed={mode === 'draft' ? draftSeed : null}
          onSaved={handleSaved}
          onDeleted={handleDeleted}
          onDirtyChange={(dirty) => (editorDirty = dirty)}
        />
      {/if}
    {/key}
  </div>
</div>

<style>
  .memo-layout {
    display: grid;
    grid-template-columns: 260px minmax(0, 1fr);
    gap: var(--space-6);
    align-items: start;
  }

  .memo-list-panel {
    display: grid;
    gap: var(--space-3);
  }

  .memo-list-actions {
    display: grid;
    gap: var(--space-2);
  }

  .memo-list {
    display: grid;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .memo-list-row {
    display: flex;
    width: 100%;
    min-height: 36px;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: 0 var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .memo-list-row:hover {
    background: var(--color-surface-sunken);
  }

  .memo-list-row.active {
    border-color: var(--color-accent-border);
    background: var(--color-accent-soft);
  }

  .memo-list-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  button {
    display: inline-flex;
    min-height: 36px;
    width: fit-content;
    align-items: center;
    padding: 0 var(--space-4);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    cursor: pointer;
  }

  button[disabled] {
    cursor: not-allowed;
    opacity: 0.6;
  }

  .inline-confirm {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--color-danger-border);
    border-radius: var(--radius-md);
    background: var(--color-danger-soft);
  }

  .inline-confirm p {
    margin: 0;
    color: var(--color-danger-strong);
    font-size: var(--text-help-size);
  }

  .inline-confirm-actions {
    display: flex;
    gap: var(--space-2);
  }
</style>
