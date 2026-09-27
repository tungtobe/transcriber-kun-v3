<script lang="ts">
  // Editor tên + prompt của một Template memo (story 3.6, spec Approach):
  // dùng chung cho mẫu đã có (`template` khác null, kể cả mẫu mặc định --
  // sửa được, xoá không được) và bản nháp "+ Thêm mẫu" (`template === null`,
  // `draftSeed` cấp tên/prompt khung ban đầu). Kiểm tra `{transcript}`/
  // `{notes}` cập nhật theo từng lần gõ (spec Boundaries Always) -- Rust vẫn
  // là nguồn thật của validate (`memo::templates::validate_prompt`), ở đây
  // chỉ chặn *gửi đi* sớm cho UX, cùng khuôn `InlineRename.svelte`.
  //
  // Component này không tự biết "đang chọn mẫu nào" -- cha
  // (`SettingsMemo.svelte`) remount nó qua `{#key}` mỗi khi đổi mẫu/mode,
  // nên state khởi tạo một lần bằng `untrack` là đủ, không cần đồng bộ lại
  // với prop sau đó.
  import { untrack } from 'svelte';
  import { i18n } from '../../../i18n/index.svelte';
  import { errorHint, errorTitle } from '../../../lib/errors';
  import { memoTemplatesStore } from '../../../lib/stores/memoTemplates.svelte';
  import DisabledHint from '../../../components/DisabledHint.svelte';
  import BannerStack from '../../../components/BannerStack.svelte';
  import type { BannerItem } from '../../../components/BannerStack.svelte';
  import type { MemoTemplate } from '../../../lib/bindings';

  const MAX_NAME_SCALARS = 100;
  const MAX_PROMPT_SCALARS = 20_000;

  let {
    template,
    draftSeed,
    onSaved,
    onDeleted,
    onDirtyChange,
  }: {
    /** Mẫu đang sửa, hoặc `null` khi đây là bản nháp "+ Thêm mẫu" chưa lưu. */
    template: MemoTemplate | null;
    /** Tên/prompt khởi tạo cho bản nháp -- chỉ dùng khi `template === null`. */
    draftSeed: { name: string; prompt: string } | null;
    /** Gọi với mẫu vừa tạo/sửa sau khi lưu thành công. */
    onSaved: (template: MemoTemplate) => void;
    /** Gọi với `id` sau khi xoá thành công. */
    onDeleted: (id: string) => void;
    /** Gọi mỗi khi trạng thái "có thay đổi chưa lưu" đổi. */
    onDirtyChange: (dirty: boolean) => void;
  } = $props();

  const initialName = untrack(() => template?.name ?? draftSeed?.name ?? '');
  const initialPrompt = untrack(() => template?.prompt ?? draftSeed?.prompt ?? '');

  let name = $state(initialName);
  let prompt = $state(initialPrompt);
  let saving = $state(false);
  let saveError = $state<import('../../../lib/bindings').AppError | null>(null);
  let deleteConfirming = $state(false);
  let deleting = $state(false);
  let deleteError = $state<import('../../../lib/bindings').AppError | null>(null);

  function scalarLength(text: string): number {
    return Array.from(text).length;
  }

  const trimmedName = $derived(name.trim());
  const nameError = $derived.by(() => {
    if (trimmedName.length === 0) return i18n.t('memoTemplates.editor.nameErrorEmpty');
    if (scalarLength(trimmedName) > MAX_NAME_SCALARS) return i18n.t('memoTemplates.editor.nameErrorTooLong');
    return null;
  });
  const promptError = $derived(
    scalarLength(prompt) > MAX_PROMPT_SCALARS ? i18n.t('memoTemplates.editor.promptErrorTooLong') : null,
  );
  const hasTranscript = $derived(prompt.includes('{transcript}'));
  const hasNotes = $derived(prompt.includes('{notes}'));
  const isValid = $derived(nameError === null && promptError === null && hasTranscript);
  const dirty = $derived(name !== initialName || prompt !== initialPrompt);

  $effect(() => {
    onDirtyChange(dirty);
  });

  const saveBanners = $derived<BannerItem[]>(
    saveError
      ? [{ id: 'memo-template-save-error', variant: 'warning', title: errorTitle(saveError), message: errorHint(saveError) }]
      : [],
  );
  const deleteBanners = $derived<BannerItem[]>(
    deleteError
      ? [{ id: 'memo-template-delete-error', variant: 'warning', title: errorTitle(deleteError), message: errorHint(deleteError) }]
      : [],
  );

  async function handleSave(): Promise<void> {
    if (!isValid || saving) return;
    saving = true;
    saveError = null;
    const result = template
      ? await memoTemplatesStore.update(template.id, trimmedName, prompt)
      : await memoTemplatesStore.create(trimmedName, prompt);
    saving = false;
    if (result.status === 'ok') {
      onSaved(result.template);
    } else {
      saveError = result.error;
    }
  }

  function startDelete(): void {
    saveError = null;
    deleteError = null;
    deleteConfirming = true;
  }

  function cancelDelete(): void {
    deleteConfirming = false;
  }

  async function confirmDelete(): Promise<void> {
    if (!template) return;
    deleting = true;
    const result = await memoTemplatesStore.remove(template.id);
    deleting = false;
    if (result.status === 'ok') {
      onDeleted(template.id);
      return;
    }
    deleteConfirming = false;
    deleteError = result.error;
  }
</script>

<div class="template-editor">
  <div class="editor-field">
    <label class="editor-label" for="memo-template-name">{i18n.t('memoTemplates.editor.nameLabel')}</label>
    <input
      id="memo-template-name"
      type="text"
      class="editor-name-input"
      maxlength={MAX_NAME_SCALARS + 20}
      bind:value={name}
      disabled={saving}
    />
    {#if nameError}<p class="editor-error" role="alert">{nameError}</p>{/if}
  </div>

  <div class="editor-field">
    <label class="editor-label" for="memo-template-prompt">{i18n.t('memoTemplates.editor.promptLabel')}</label>
    <p class="editor-help">{i18n.t('memoTemplates.editor.promptHelp')}</p>
    <textarea id="memo-template-prompt" class="editor-prompt-input" rows="16" bind:value={prompt} disabled={saving}></textarea>
    {#if promptError}<p class="editor-error" role="alert">{promptError}</p>{/if}
  </div>

  <ul class="placeholder-checks" aria-live="polite">
    <li class:placeholder-ok={hasTranscript} class:placeholder-missing={!hasTranscript}>
      {hasTranscript ? i18n.t('memoTemplates.editor.transcriptOk') : i18n.t('memoTemplates.editor.transcriptMissing')}
    </li>
    <li class:placeholder-ok={hasNotes}>
      {hasNotes ? i18n.t('memoTemplates.editor.notesPresent') : i18n.t('memoTemplates.editor.notesAbsent')}
    </li>
  </ul>

  <div class="editor-actions">
    <button type="button" disabled={!isValid || saving} onclick={() => void handleSave()}>
      {saving ? i18n.t('memoTemplates.editor.savingButton') : i18n.t('memoTemplates.editor.saveButton')}
    </button>
    {#if template && !template.isDefault}
      <button type="button" class="button-danger-soft" disabled={deleting} onclick={startDelete}>
        {i18n.t('memoTemplates.editor.deleteButton')}
      </button>
    {:else if template}
      <DisabledHint reason={i18n.t('memoTemplates.editor.deleteDisabledHint')}>
        <span class="button-danger-soft disabled-look">{i18n.t('memoTemplates.editor.deleteButton')}</span>
      </DisabledHint>
    {/if}
  </div>

  <BannerStack banners={saveBanners} />

  {#if deleteConfirming}
    <div class="inline-confirm">
      <p>{i18n.t('memoTemplates.editor.deleteConfirm')}</p>
      <div class="inline-confirm-actions">
        <button type="button" class="button-danger-soft" disabled={deleting} onclick={() => void confirmDelete()}>
          {i18n.t('memoTemplates.editor.deleteConfirmButton')}
        </button>
        <button type="button" disabled={deleting} onclick={cancelDelete}>
          {i18n.t('memoTemplates.editor.deleteCancelButton')}
        </button>
      </div>
      <BannerStack banners={deleteBanners} />
    </div>
  {/if}
</div>

<style>
  .template-editor {
    display: grid;
    gap: var(--space-4);
  }

  .editor-field {
    display: grid;
    gap: var(--space-1);
  }

  .editor-label {
    color: var(--color-text-secondary);
    font-size: var(--text-label-size);
    font-weight: 600;
  }

  .editor-help {
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
  }

  .editor-name-input {
    min-height: 36px;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
  }

  .editor-prompt-input {
    min-height: 320px;
    padding: var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text);
    font-family: var(--font-mono);
    font-size: var(--text-option-size);
    line-height: 1.5;
    resize: vertical;
  }

  .editor-name-input:disabled,
  .editor-prompt-input:disabled {
    cursor: not-allowed;
    opacity: 0.7;
  }

  .editor-error {
    margin: 0;
    color: var(--color-danger-strong);
    font-size: var(--text-help-size);
  }

  .placeholder-checks {
    display: grid;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-help-size);
  }

  .placeholder-checks li {
    color: var(--color-text-muted);
  }

  .placeholder-checks li.placeholder-ok {
    color: var(--color-info);
  }

  .placeholder-checks li.placeholder-missing {
    color: var(--color-danger-strong);
    font-weight: 600;
  }

  .editor-actions {
    display: flex;
    align-items: center;
    gap: var(--space-3);
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

  .button-danger-soft {
    border-color: var(--color-danger-border);
    color: var(--color-danger-strong);
  }

  .disabled-look {
    display: inline-flex;
    min-height: 36px;
    align-items: center;
    padding: 0 var(--space-4);
    border: 1px solid var(--color-danger-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    font: inherit;
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
