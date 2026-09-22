<script lang="ts">
  // The (?) tooltip trigger reused by every `SettingsRow` field (spec Code
  // Map): reachable by Tab, opens on focus/hover, closes on Escape. Visibility
  // is driven by JS state (not bare CSS :hover/:focus) so Escape can actually
  // dismiss it while the pointer is still hovering.
  import { i18n } from '../i18n/index.svelte';

  let {
    label,
    text,
  }: {
    /** The field this tooltip belongs to, folded into the trigger's accessible name. */
    label: string;
    /** Part shown to every user in the tooltip bubble on hover/focus. */
    text: string;
  } = $props();

  const tooltipId = `help-tip-${Math.random().toString(36).slice(2)}`;
  let open = $state(false);

  function show(): void {
    open = true;
  }

  function hide(): void {
    open = false;
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape' && open) {
      event.stopPropagation();
      open = false;
    }
  }
</script>

<span class="help-tip">
  <button
    type="button"
    class="help-tip-trigger"
    aria-label={`${label} — ${i18n.t('settings.help.tooltipToggle')}`}
    aria-describedby={tooltipId}
    onmouseenter={show}
    onmouseleave={hide}
    onfocus={show}
    onblur={hide}
    onkeydown={onKeydown}
  >
    <span aria-hidden="true">?</span>
  </button>
  <span id={tooltipId} class="help-tip-bubble" class:visible={open} role="tooltip">
    {text}
  </span>
</span>

<style>
  .help-tip {
    position: relative;
    display: inline-flex;
  }

  .help-tip-trigger {
    display: grid;
    width: 18px;
    height: 18px;
    flex: 0 0 auto;
    place-items: center;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-full);
    background: var(--color-surface);
    color: var(--color-text-muted);
    cursor: help;
    font-size: var(--text-badge-size);
    font-weight: 600;
    line-height: 1;
  }

  .help-tip-trigger:hover,
  .help-tip-trigger:focus-visible {
    border-color: var(--color-accent-border);
    color: var(--color-accent);
  }

  .help-tip-bubble {
    position: absolute;
    bottom: calc(100% + 6px);
    left: 0;
    z-index: 10;
    width: max-content;
    max-width: 260px;
    padding: 6px var(--space-3);
    border-radius: var(--radius-md);
    background: var(--color-text);
    color: var(--color-surface);
    font-size: var(--text-help-size);
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.1s ease;
  }

  .help-tip-bubble.visible {
    opacity: 1;
  }
</style>
