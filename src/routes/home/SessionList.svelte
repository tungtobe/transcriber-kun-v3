<script lang="ts">
  // Virtual list tự viết (spec Never: "Không thêm dependency npm/Cargo (virtual
  // list tự viết)") — chiều cao dòng cố định `ROW_HEIGHT`: `scrollTop` ->
  // chỉ số đầu = `floor(scrollTop / ROW_HEIGHT) - overscan`, container trong
  // có chiều cao `count × ROW_HEIGHT`, dòng hiển thị dịch bằng
  // `transform: translateY` (spec Design Notes). Toàn bộ `sessions` đã tải
  // một lần trước khi tới đây (không phân trang, không infinite scroll) —
  // component này chỉ quyết định *dòng nào* render vào DOM.
  //
  // jsdom không có layout thật (`clientHeight` luôn 0) nên `viewportHeight`
  // là một override test có thể cấp trực tiếp; không cấp thì đọc
  // `clientHeight` đã đo, và nếu vẫn 0 (jsdom, hoặc container chưa có chiều
  // cao CSS) thì rơi về `DEFAULT_VIEWPORT_HEIGHT` (spec Design Notes).
  import SessionRow from './SessionRow.svelte';
  import type { SessionListItem } from '../../lib/bindings';

  const ROW_HEIGHT = 56;
  const OVERSCAN = 4;
  const DEFAULT_VIEWPORT_HEIGHT = 480;

  let {
    sessions,
    viewportHeight,
  }: {
    sessions: SessionListItem[];
    /** Test-only / caller override cho chiều cao khung nhìn (px). */
    viewportHeight?: number;
  } = $props();

  let containerEl = $state<HTMLDivElement | null>(null);
  let measuredHeight = $state(0);
  let scrollTop = $state(0);

  $effect(() => {
    if (containerEl) measuredHeight = containerEl.clientHeight;
  });

  const effectiveHeight = $derived(
    viewportHeight ?? (measuredHeight > 0 ? measuredHeight : DEFAULT_VIEWPORT_HEIGHT),
  );

  const startIndex = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN));
  const visibleCount = $derived(Math.ceil(effectiveHeight / ROW_HEIGHT) + OVERSCAN * 2);
  const endIndex = $derived(Math.min(sessions.length, startIndex + visibleCount));
  const visibleSessions = $derived(sessions.slice(startIndex, endIndex));
  const totalHeight = $derived(sessions.length * ROW_HEIGHT);
  const offsetY = $derived(startIndex * ROW_HEIGHT);

  function handleScroll(event: Event): void {
    scrollTop = (event.currentTarget as HTMLDivElement).scrollTop;
  }
</script>

<div
  class="session-list"
  bind:this={containerEl}
  onscroll={handleScroll}
  style={viewportHeight ? `height: ${viewportHeight}px;` : undefined}
>
  <div class="session-list-spacer" style={`height: ${totalHeight}px;`}>
    <div class="session-list-window" style={`transform: translateY(${offsetY}px);`}>
      {#each visibleSessions as session (session.sessionId)}
        <SessionRow {session} />
      {/each}
    </div>
  </div>
</div>

<style>
  .session-list {
    height: 100%;
    overflow-y: auto;
  }

  .session-list-spacer {
    position: relative;
    width: 100%;
  }

  .session-list-window {
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
  }
</style>
