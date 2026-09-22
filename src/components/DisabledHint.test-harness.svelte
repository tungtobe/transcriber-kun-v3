<script lang="ts">
  // Test-only harness: DisabledHint takes its visible control as a Snippet,
  // which @testing-library/svelte cannot supply directly to `render()`.
  import DisabledHint from './DisabledHint.svelte';

  let {
    reason,
    shortcut,
    label,
  }: {
    reason: string;
    shortcut?: string;
    label: string;
  } = $props();
</script>

<DisabledHint {reason} {shortcut}>
  {label}
</DisabledHint>
