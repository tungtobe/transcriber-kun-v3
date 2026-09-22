<script lang="ts">
  // Test-only harness: SettingsRow takes its control as a Snippet, which
  // @testing-library/svelte cannot supply directly to `render()`.
  import SettingsRow from './SettingsRow.svelte';

  let {
    label,
    help,
    helperText,
    fieldId,
    controlText,
  }: {
    label: string;
    help: string;
    helperText: string;
    fieldId?: string;
    controlText: string;
  } = $props();
</script>

<SettingsRow {label} {help} {helperText} {fieldId}>
  <input id={fieldId} type="text" value={controlText} readonly />
</SettingsRow>
