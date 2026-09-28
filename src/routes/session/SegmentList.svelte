<script lang="ts">
  // Luồng Segment: grid `56px | 1fr` (spec Layout), Segment đang phát nền
  // `accent-soft` + `aria-current="true"` (spec Always), click seek tới
  // `start_sec` gốc, Space play/pause khi focus trong danh sách (không nuốt
  // Space trong input/nút lồng bên trong một dòng gap), và tự cuộn theo dòng
  // đang phát — dừng ngay khi người dùng cuộn tay lúc đang phát, hiện nút
  // "Xuống dòng đang phát" để cuộn lại + bật lại tự cuộn (spec Always). Chỉ
  // `wheel`/`touchmove`/phím cuộn đếm là cuộn tay — không nghe sự kiện
  // `scroll` chung, nên cuộn do chính `scrollIntoView` gây ra không thể tự
  // tắt tự cuộn (Design Notes), không cần cờ đánh dấu riêng.
  import type { RerunScope } from '../../lib/bindings';
  import { i18n } from '../../i18n/index.svelte';
  import { displayTimestamp } from '../../lib/time';
  import type { TranscriptSearchMatch } from '../../lib/transcript-search';
  import { RotateCcwIcon, ChevronDownIcon } from '../../components/icons';

  export type SegmentDetailView = {
    idx: number;
    startSec: number | null;
    endSec: number | null;
    kind: string;
    gapReason: string | null;
    text: string;
  };

  let {
    segments,
    currentTime,
    playing,
    matches = [],
    activeMatchIndex = -1,
    rerunStarting,
    rerunAvailable = true,
    disconnectedHint = false,
    onSeek,
    onTogglePlay,
    onRerun,
  }: {
    segments: SegmentDetailView[];
    /** `currentTime` gốc của trình phát — không cộng offset (spec Always:
     * "highlight so `currentTime` gốc với `[start, end)`"). */
    currentTime: number;
    playing: boolean;
    matches?: TranscriptSearchMatch[];
    activeMatchIndex?: number;
    rerunStarting: boolean;
    rerunAvailable?: boolean;
    disconnectedHint?: boolean;
    onSeek: (startSec: number) => void;
    onTogglePlay: () => void;
    onRerun: (scope: RerunScope) => void;
  } = $props();

  let rowEls = new Map<number, HTMLElement>();
  let autoScroll = $state(true);

  type IndexedMatch = TranscriptSearchMatch & { matchIndex: number };
  type TextPart = { text: string; matched: boolean; current: boolean };

  const matchesBySegment = $derived.by(() => {
    const grouped = new Map<number, IndexedMatch[]>();
    matches.forEach((match, matchIndex) => {
      const group = grouped.get(match.segmentIndex) ?? [];
      group.push({ ...match, matchIndex });
      grouped.set(match.segmentIndex, group);
    });
    return grouped;
  });

  function textParts(segmentIndex: number, text: string): TextPart[] {
    const segmentMatches = matchesBySegment.get(segmentIndex) ?? [];
    if (segmentMatches.length === 0) return [{ text, matched: false, current: false }];

    const parts: TextPart[] = [];
    let cursor = 0;
    for (const match of segmentMatches) {
      const start = Math.max(cursor, Math.min(text.length, match.start));
      const end = Math.max(start, Math.min(text.length, match.end));
      if (start > cursor) parts.push({ text: text.slice(cursor, start), matched: false, current: false });
      if (end > start) {
        parts.push({ text: text.slice(start, end), matched: true, current: match.matchIndex === activeMatchIndex });
        cursor = end;
      }
    }
    if (cursor < text.length) parts.push({ text: text.slice(cursor), matched: false, current: false });
    return parts;
  }

  function hasCurrentSearchMatch(segmentIndex: number): boolean {
    return (matchesBySegment.get(segmentIndex) ?? []).some((match) => match.matchIndex === activeMatchIndex);
  }

  function sec(value: number | null): number {
    return value ?? 0;
  }

  const activeIndex = $derived.by(() => {
    for (let i = 0; i < segments.length; i += 1) {
      const start = sec(segments[i].startSec);
      const end = sec(segments[i].endSec);
      if (currentTime >= start && currentTime < end) return i;
    }
    return -1;
  });

  // Chỉ `wheel`/`touchmove`/phím cuộn (bên dưới) đếm là "cuộn tay" — không
  // nghe sự kiện `scroll` chung, nên không cần đánh dấu cờ để phân biệt với
  // cuộn do `scrollIntoView` gây ra (Design Notes: tránh để `scrollIntoView`
  // tự tắt tự cuộn — không nghe `scroll` thì vấn đề đó không tồn tại).
  function scrollRowIntoView(index: number, block: ScrollLogicalPosition): void {
    const el = rowEls.get(index);
    el?.scrollIntoView?.({ block, behavior: 'smooth' });
  }

  $effect(() => {
    const index = activeIndex;
    if (!playing || !autoScroll || index < 0) return;
    scrollRowIntoView(index, 'nearest');
  });

  $effect(() => {
    const match = matches[activeMatchIndex];
    if (match) scrollRowIntoView(match.segmentIndex, 'center');
  });

  function handleUserScrollSignal(): void {
    if (!playing) return;
    autoScroll = false;
  }

  function handleJumpToActive(): void {
    autoScroll = true;
    if (activeIndex >= 0) scrollRowIntoView(activeIndex, 'center');
  }

  function isInteractiveTarget(target: EventTarget | null): boolean {
    const el = target as HTMLElement | null;
    return el !== null && (el.tagName === 'BUTTON' || el.tagName === 'INPUT');
  }

  const SCROLL_KEYS = new Set(['PageUp', 'PageDown', 'Home', 'End', 'ArrowUp', 'ArrowDown']);

  function handleListKeydown(event: KeyboardEvent): void {
    if (event.key === ' ' && !isInteractiveTarget(event.target)) {
      event.preventDefault();
      onTogglePlay();
      return;
    }
    if (SCROLL_KEYS.has(event.key)) {
      handleUserScrollSignal();
    }
  }

  function registerRow(node: HTMLElement, index: number) {
    // `current` is reassigned on every `update` so a row that shifts more
    // than once (e.g. key 5 -> 6 -> 7 as earlier rows are added/removed)
    // still deletes the index it actually holds, not the index it was
    // registered under the first time (spec Boundaries: `registerRow`).
    let current = index;
    rowEls.set(current, node);
    return {
      update(nextIndex: number) {
        rowEls.delete(current);
        current = nextIndex;
        rowEls.set(current, node);
      },
      destroy() {
        rowEls.delete(current);
      },
    };
  }
</script>

<div class="segment-list-wrap">
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <!-- `region` chỉ là một mốc bố cục có tên (spec Layout: khối Transcript) —
       keydown/wheel/touchmove ở đây là *delegation* để bắt Space (play/pause,
       "không nuốt Space trong input/nút" của các control lồng bên trong) và
       tín hiệu cuộn tay (dừng tự cuộn), không biến khối này thành một widget
       tương tác giả; từng dòng bên trong mới là control thật (role="button"
       hay nút thật) với hành vi bàn phím riêng của nó. -->
  <div
    class="segment-list"
    role="region"
    aria-label={i18n.t('session.segment.listLabel')}
    onkeydown={handleListKeydown}
    onwheel={handleUserScrollSignal}
    ontouchmove={handleUserScrollSignal}
  >
    {#each segments as segment, index (segment.idx)}
      {#if segment.kind === 'gap'}
        <div
          class="segment-row segment-row-gap"
          class:segment-row-gap-failed={segment.gapReason === 'chunk_failed'}
          class:segment-row-active={activeIndex === index}
          class:segment-row-search-active={hasCurrentSearchMatch(index)}
          aria-current={activeIndex === index ? 'true' : undefined}
          use:registerRow={index}
        >
          <span class="segment-time">{displayTimestamp(sec(segment.startSec))}</span>
          <div class="segment-gap-body">
            <span class="segment-gap-label">
              {segment.gapReason === 'chunk_failed'
                ? i18n.t('session.segment.gapChunkFailed')
                : i18n.t('session.segment.gapDisconnected')}
            </span>
            {#if segment.gapReason === 'disconnected' && disconnectedHint}
              <span class="segment-gap-hint">{i18n.t('session.segment.gapDisconnectedHint')}</span>
            {/if}
            {#if segment.gapReason === 'chunk_failed' && rerunAvailable}
              <button
                type="button"
                class="segment-gap-rerun"
                disabled={rerunStarting}
                onclick={() => onRerun({ kind: 'gap', gapId: segment.idx })}
              >
                <RotateCcwIcon size={14} strokeWidth={1.75} />
                {i18n.t('session.segment.rerunThisGap')}
              </button>
            {/if}
          </div>
        </div>
      {:else}
        <div
          class="segment-row"
          class:segment-row-active={activeIndex === index}
          class:segment-row-search-active={hasCurrentSearchMatch(index)}
          role="button"
          tabindex="0"
          aria-current={activeIndex === index ? 'true' : undefined}
          use:registerRow={index}
          onclick={() => onSeek(sec(segment.startSec))}
          onkeydown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault();
              onSeek(sec(segment.startSec));
            }
          }}
        >
          <span class="segment-time">{displayTimestamp(sec(segment.startSec))}</span>
          <p class="segment-text">
            {#each textParts(index, segment.text) as part}
              {#if part.matched}
                <mark class:search-match-current={part.current}>{part.text}</mark>
              {:else}
                {part.text}
              {/if}
            {/each}
          </p>
        </div>
      {/if}
    {/each}
    {#if segments.length === 0}
      <p class="segment-list-empty">{i18n.t('session.segment.empty')}</p>
    {/if}
  </div>

  {#if playing && !autoScroll}
    <button type="button" class="jump-to-active" onclick={handleJumpToActive}>
      <ChevronDownIcon size={16} strokeWidth={1.75} />
      {i18n.t('session.segment.jumpToActive')}
    </button>
  {/if}
</div>

<style>
  .segment-list-wrap {
    position: relative;
    min-height: 0;
    flex: 1;
  }

  .segment-list {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow-y: auto;
  }

  .segment-row {
    display: grid;
    grid-template-columns: 56px 1fr;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
    cursor: pointer;
  }

  .segment-row[role~='button']:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: -2px;
  }

  .segment-row-active {
    background: var(--color-accent-soft);
  }

  .segment-row-search-active {
    outline: 2px solid var(--color-accent);
    outline-offset: -2px;
  }

  .segment-time {
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: var(--text-help-size);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .segment-text {
    margin: 0;
    color: var(--color-text);
    font-size: var(--text-body-size);
    line-height: 1.55;
  }

  mark {
    border-radius: 2px;
    background: var(--color-mark);
    color: var(--color-text);
  }

  mark.search-match-current {
    outline: 2px solid var(--color-accent);
    outline-offset: 1px;
  }

  .segment-row-gap {
    cursor: default;
  }

  .segment-row-gap-failed {
    background: var(--color-warning-soft);
  }

  .segment-gap-body {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-3);
  }

  .segment-gap-hint {
    color: var(--color-text-secondary);
    font-size: var(--text-help-size);
  }

  .segment-gap-label {
    color: var(--color-text-secondary);
    font-size: var(--text-body-size);
    font-style: italic;
  }

  .segment-gap-rerun {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    min-height: 28px;
    padding: 0 var(--space-2);
    border: 1px solid var(--color-warning-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-warning);
    font-size: var(--text-help-size);
    font-weight: 600;
    cursor: pointer;
  }

  .segment-gap-rerun:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }

  .segment-list-empty {
    padding: var(--space-4);
    color: var(--color-text-muted);
    font-size: var(--text-body-size);
  }

  .jump-to-active {
    position: absolute;
    bottom: var(--space-4);
    left: 50%;
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    padding: 0 var(--space-3);
    min-height: 32px;
    transform: translateX(-50%);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-full);
    background: var(--color-surface);
    color: var(--color-text);
    /* `check:ui` resolves `box-shadow` only against tokens defined in the
       same file — the literal allow-listed value (== `--shadow-floating`
       in tokens.css) matches `CloseConfirm.svelte`'s existing convention. */
    box-shadow: 0 10px 32px rgb(17 24 39 / 12%);
    font-size: var(--text-label-size);
    cursor: pointer;
  }
</style>
