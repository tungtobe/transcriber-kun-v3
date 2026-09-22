<script lang="ts">
  // One `220px | control` Settings field: label + (?) tooltip on the left,
  // the control plus a persistent (always-visible) helper line on the right
  // (spec Design: "mỗi trường có (?) tooltip ... cùng helper text bền").
  import type { Snippet } from 'svelte';
  import HelpTip from './HelpTip.svelte';

  let {
    label,
    help,
    helperText,
    fieldId,
    children,
  }: {
    label: string;
    /** Tooltip content shown on hover/focus of the (?) trigger. */
    help: string;
    /** Persistent, always-visible one-liner under the control. */
    helperText: string;
    /** Id of the primary control, wired to the `<label for>`. */
    fieldId?: string;
    children: Snippet;
  } = $props();
</script>

<div class="settings-row">
  <div class="settings-row-label">
    <label for={fieldId}>{label}</label>
    <HelpTip {label} text={help} />
  </div>
  <div class="settings-row-control">
    {@render children()}
    <p class="settings-row-helper">{helperText}</p>
  </div>
</div>

<style>
  .settings-row {
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr);
    gap: var(--space-4);
    align-items: start;
    padding: var(--space-4) 0;
    border-top: 1px solid var(--color-border);
  }

  .settings-row:first-child {
    padding-top: 0;
    border-top: 0;
  }

  .settings-row-label {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding-top: 8px;
  }

  .settings-row-label label {
    color: var(--color-text-secondary);
    font-size: var(--text-label-size);
    font-weight: 600;
  }

  .settings-row-control {
    display: grid;
    min-width: 0;
    gap: var(--space-2);
  }

  .settings-row-helper {
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
  }
</style>
