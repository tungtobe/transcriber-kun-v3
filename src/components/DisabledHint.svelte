<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    reason,
    shortcut,
    children,
  }: {
    /** Why the action is unavailable right now. */
    reason: string;
    /** The way out — a shortcut/next step, spoken together with `reason`. */
    shortcut?: string;
    /**
     * The visible control, e.g. `<span class="button button-primary">Label</span>`.
     * `children` is rendered literally inside the trigger `<button>` so the
     * caller's own scoped styles (defined on that literal element) still
     * apply; this component only resets the outer button chrome.
     */
    children: Snippet;
  } = $props();

  const tooltipId = `disabled-hint-${Math.random().toString(36).slice(2)}`;

  // aria-disabled (not the native `disabled` attribute) keeps the control
  // reachable by Tab; the click is still swallowed so it never activates
  // (spec Always: aria-disabled, focus được bằng Tab, opacity .45, tooltip
  // lý do + lối tắt, không ẩn).
  function swallow(event: Event): void {
    event.preventDefault();
  }
</script>

<span class="disabled-hint">
  <button
    type="button"
    class="disabled-hint-trigger"
    aria-disabled="true"
    aria-describedby={tooltipId}
    title={shortcut ? `${reason} — ${shortcut}` : reason}
    onclick={swallow}
  >
    {@render children()}
  </button>
  <span id={tooltipId} class="disabled-hint-tooltip" role="tooltip">
    {reason}{#if shortcut}<span class="disabled-hint-shortcut"> — {shortcut}</span>{/if}
  </span>
</span>

<style>
  .disabled-hint {
    position: relative;
    display: inline-flex;
  }

  .disabled-hint-trigger {
    display: inline-flex;
    padding: 0;
    border: 0;
    background: transparent;
    font: inherit;
    color: inherit;
    text-align: inherit;
  }

  .disabled-hint-trigger[aria-disabled='true'] {
    cursor: not-allowed;
    opacity: 0.45;
  }

  .disabled-hint-tooltip {
    position: absolute;
    bottom: calc(100% + 6px);
    left: 0;
    z-index: 10;
    width: max-content;
    max-width: 240px;
    padding: 6px var(--space-3);
    border-radius: var(--radius-md);
    background: var(--color-text);
    color: var(--color-surface);
    font-size: var(--text-help-size);
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.1s ease;
  }

  .disabled-hint-trigger:hover + .disabled-hint-tooltip,
  .disabled-hint-trigger:focus-visible + .disabled-hint-tooltip {
    opacity: 1;
  }
</style>
