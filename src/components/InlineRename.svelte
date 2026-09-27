<script lang="ts">
  // Ô nhập inline dùng chung cho đổi tên phiên (story 3.1, spec Boundaries
  // Always): trim, rỗng bị từ chối tại chỗ, > 200 ký tự Unicode scalar bị
  // chặn ở cả `maxlength` lẫn thông báo lỗi; Enter lưu (chỉ khi hợp lệ), Esc
  // huỷ và gọi `onCancel` để caller khôi phục tên cũ + trả focus về nút/tên
  // đã mở (spec: component này không tự biết "nút/tên đã mở" là gì — đó là
  // việc của caller). Validate ở đây chỉ chặn *gửi đi* — Rust vẫn tự validate
  // lại (`library::store::rename_session`) vì có thể bị gọi trực tiếp.
  import { untrack } from 'svelte';
  import { i18n } from '../i18n/index.svelte';

  const MAX_TITLE_SCALARS = 200;

  let {
    value,
    label,
    saving = false,
    serverError = null,
    onSave,
    onCancel,
  }: {
    value: string;
    label: string;
    /** `true` trong lúc chờ IPC — vô hiệu input để tránh gửi trùng. */
    saving?: boolean;
    /** Lỗi từ lần lưu gần nhất phía Rust (ví dụ IPC lỗi) — hiển thị cùng chỗ
     * với lỗi validate tại chỗ, nhưng không thay thế nó. */
    serverError?: string | null;
    /** Gọi với tên đã trim, hợp lệ (không rỗng, ≤ 200 scalar) — caller chịu
     * trách nhiệm gọi IPC và đóng chế độ sửa khi thành công. */
    onSave: (title: string) => void;
    onCancel: () => void;
  } = $props();

  // Chỉ đọc `value` một lần lúc mở (bản nháp riêng, chỉnh sửa độc lập) --
  // `untrack` vì component chỉ mount đúng một lần cho mỗi lượt đổi tên
  // (`{#if renaming}` ở `SessionRow`/`SessionHeader`), không cần theo dõi
  // `value` đổi sau đó.
  let draft = $state(untrack(() => value));
  let localError = $state<string | null>(null);
  let inputEl = $state<HTMLInputElement | null>(null);

  $effect(() => {
    inputEl?.focus();
    inputEl?.select();
  });

  function scalarLength(text: string): number {
    return Array.from(text).length;
  }

  function submit(): void {
    const trimmed = draft.trim();
    if (trimmed.length === 0) {
      localError = i18n.t('sessionRename.error.empty');
      return;
    }
    if (scalarLength(trimmed) > MAX_TITLE_SCALARS) {
      localError = i18n.t('sessionRename.error.tooLong');
      return;
    }
    localError = null;
    onSave(trimmed);
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter') {
      event.preventDefault();
      submit();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      onCancel();
    }
  }

  const displayedError = $derived(localError ?? serverError);
</script>

<span class="inline-rename">
  <input
    bind:this={inputEl}
    class="inline-rename-input"
    type="text"
    aria-label={label}
    maxlength={MAX_TITLE_SCALARS}
    disabled={saving}
    bind:value={draft}
    onkeydown={handleKeydown}
  />
  {#if displayedError}
    <span class="inline-rename-error" role="alert">{displayedError}</span>
  {/if}
</span>

<style>
  /* `position: relative` + an absolutely positioned error (below) so this
   * never grows the row it sits in — callers with a fixed-height row
   * (`SessionRow`'s virtual list assumes exactly 56px per row) rely on that
   * (spec Code Map: "giữ layout 56 px"). */
  .inline-rename {
    position: relative;
    display: inline-flex;
    min-width: 0;
  }

  .inline-rename-input {
    min-width: 160px;
    max-width: 100%;
    padding: 2px var(--space-2);
    border: 1px solid var(--color-accent-border);
    border-radius: var(--radius-sm);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
    font-weight: 500;
  }

  .inline-rename-input:disabled {
    cursor: not-allowed;
    opacity: 0.7;
  }

  .inline-rename-error {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 5;
    margin-top: 2px;
    padding: 2px 6px;
    border-radius: var(--radius-sm);
    background: var(--color-danger-soft);
    white-space: nowrap;
    color: var(--color-danger-strong);
    font-size: var(--text-help-size);
  }
</style>
