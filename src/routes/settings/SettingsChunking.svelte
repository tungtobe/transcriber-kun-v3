<script lang="ts">
  // Settings → Chunking: `chunkMinutes` (số phút mỗi Chunk khi transcribe
  // file) và `timestampOffsetSec` (offset thuần hiển thị, xem
  // `src/lib/time.ts`). Cả hai chặn tại chỗ giá trị không hợp lệ (rỗng,
  // không phải số nguyên, nhỏ hơn mức tối thiểu) và chỉ lưu khi hợp lệ, theo
  // đúng khuôn inline-validation của Settings → Gemini's model input (spec
  // Always).
  import SettingsRow from '../../components/SettingsRow.svelte';
  import { i18n } from '../../i18n/index.svelte';
  import { settingsStore } from '../../lib/stores/settings.svelte';

  /** Rỗng, không phải số nguyên, hoặc nhỏ hơn `min` -> `null` (không lưu). */
  function parseIntegerAtLeast(value: string, min: number): number | null {
    const trimmed = value.trim();
    if (!/^-?\d+$/.test(trimmed)) return null;
    const parsed = Number(trimmed);
    if (!Number.isSafeInteger(parsed) || parsed < min) return null;
    return parsed;
  }

  let chunkMinutesError = $state(false);
  let offsetError = $state(false);

  function handleChunkMinutesInput(event: Event): void {
    const value = (event.currentTarget as HTMLInputElement).value;
    chunkMinutesError = parseIntegerAtLeast(value, 1) === null;
  }

  function commitChunkMinutes(event: Event): void {
    const value = (event.currentTarget as HTMLInputElement).value;
    const parsed = parseIntegerAtLeast(value, 1);
    if (parsed === null) {
      chunkMinutesError = true;
      return;
    }
    chunkMinutesError = false;
    void settingsStore.setChunkMinutes(parsed);
  }

  function handleOffsetInput(event: Event): void {
    const value = (event.currentTarget as HTMLInputElement).value;
    offsetError = parseIntegerAtLeast(value, 0) === null;
  }

  function commitOffset(event: Event): void {
    const value = (event.currentTarget as HTMLInputElement).value;
    const parsed = parseIntegerAtLeast(value, 0);
    if (parsed === null) {
      offsetError = true;
      return;
    }
    offsetError = false;
    void settingsStore.setTimestampOffsetSec(parsed);
  }
</script>

<div class="settings-fields">
  <SettingsRow
    label={i18n.t('settings.chunking.chunkMinutesLabel')}
    help={i18n.t('settings.chunking.chunkMinutesHelp')}
    helperText={i18n.t('settings.chunking.chunkMinutesHelper')}
    fieldId="settings-chunking-minutes"
  >
    <input
      id="settings-chunking-minutes"
      inputmode="numeric"
      autocomplete="off"
      value={settingsStore.chunkMinutes}
      aria-invalid={chunkMinutesError ? 'true' : undefined}
      oninput={handleChunkMinutesInput}
      onchange={commitChunkMinutes}
    />
    {#if chunkMinutesError}
      <p class="inline-error" role="alert">{i18n.t('settings.chunking.chunkMinutesError')}</p>
    {/if}
  </SettingsRow>

  <SettingsRow
    label={i18n.t('settings.chunking.offsetLabel')}
    help={i18n.t('settings.chunking.offsetHelp')}
    helperText={i18n.t('settings.chunking.offsetHelper')}
    fieldId="settings-chunking-offset"
  >
    <input
      id="settings-chunking-offset"
      inputmode="numeric"
      autocomplete="off"
      value={settingsStore.timestampOffsetSec}
      aria-invalid={offsetError ? 'true' : undefined}
      oninput={handleOffsetInput}
      onchange={commitOffset}
    />
    {#if offsetError}
      <p class="inline-error" role="alert">{i18n.t('settings.chunking.offsetError')}</p>
    {/if}
  </SettingsRow>
</div>

<style>
  .settings-fields {
    display: grid;
  }

  .settings-fields input {
    min-height: 36px;
    max-width: 160px;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font-variant-numeric: tabular-nums;
  }

  .inline-error {
    margin: 0;
    color: var(--color-danger);
    font-size: var(--text-help-size);
  }
</style>
