<script lang="ts">
  // Home (story 2.9): trạng thái trống (2.8's two-card UI, giữ nguyên) khi
  // không có Phiên và không có Job; ngược lại drop-zone mỏng + card job (nếu
  // có) + danh sách Phiên virtualized (spec Boundaries/Intent). `jobsStore`
  // được subscribe suốt vòng đời màn này (đếm tham chiếu — sidebar giữ
  // subscribe riêng của nó, không đụng nhau) và `libraryStore` tự tải lại
  // khi một Job commit (`jobsStore.resultSeq` đổi) — không cần thao tác gì ở
  // đây ngoài `load()` lúc mount.
  import { onMount } from 'svelte';
  import { link } from '@keenmate/svelte-spa-router';
  import { InfoIcon, RadioIcon, UploadIcon } from '../components/icons';
  import DisabledHint from '../components/DisabledHint.svelte';
  import BannerStack, { type BannerItem } from '../components/BannerStack.svelte';
  import IntakeNotices from '../components/IntakeNotices.svelte';
  import JobCard from '../components/JobCard.svelte';
  import SessionList from './home/SessionList.svelte';
  import SessionListFooter from './home/SessionListFooter.svelte';
  import SessionListSkeleton from './home/SessionListSkeleton.svelte';
  import SessionSearch from './home/SessionSearch.svelte';
  import TagFilterBar from './home/TagFilterBar.svelte';
  import { i18n } from '../i18n/index.svelte';
  import { registerKeymap } from '../lib/keymap';
  import { keysStore } from '../lib/stores/keys.svelte';
  import { intakeStore } from '../lib/stores/intake.svelte';
  import { jobsStore } from '../lib/stores/jobs.svelte';
  import { libraryStore } from '../lib/stores/library.svelte';

  // Test-only seam (spec Design Notes: "jsdom không có layout nên test cần
  // cấp chiều cao khung nhìn qua prop/biến có thể ghi đè"): jsdom's
  // `clientHeight` is always 0, so a real viewport can never be measured
  // there — a test renders `Home` with this prop to pin the list's viewport
  // height instead. Production never passes it (`SessionList` auto-measures
  // `.session-list-wrap`'s real `clientHeight`).
  let { sessionListViewportHeight }: { sessionListViewportHeight?: number } = $props();

  let subscribedJobs = false;
  let cancellingJobId = $state<string | null>(null);

  // Story 3.3: ref tới `SessionSearch` để `⌘F`/`Ctrl+F` focus + chọn nội dung
  // ô tìm (spec Always) -- `focusAndSelect` là hàm export của component đó.
  let sessionSearchRef = $state<{ focusAndSelect: () => void } | null>(null);

  onMount(() => {
    let active = true;
    let unlistenRecovery: (() => void) | null = null;
    void keysStore.load();
    subscribedJobs = true;
    void jobsStore.subscribe();
    void libraryStore
      .listenForRecoveryCompleted(() => void libraryStore.load())
      .then((unlisten) => {
        if (!active) {
          unlisten();
          return;
        }
        unlistenRecovery = unlisten;
        // Subscribe before the first list request. A recovery event emitted
        // during setup then either precedes this read (so the row is included)
        // or follows it and queues a reactive refresh.
        void libraryStore.load();
      })
      .catch(() => {
        if (active) void libraryStore.load();
      });
    void libraryStore.loadTags();
    return () => {
      active = false;
      unlistenRecovery?.();
      if (subscribedJobs) jobsStore.unsubscribe();
    };
  });

  // Loading is treated conservatively (button stays disabled) the same way
  // `showKeyBanner` below does — a file button that flickers enabled then
  // disabled as `keysList` resolves is worse than a brief extra disabled
  // moment (spec Always: "khi chưa có key dùng được thì cả hai vô hiệu").
  const canChooseFile = $derived(keysStore.status === 'ready' && keysStore.hasUsableKey);

  function chooseFile(): void {
    if (!canChooseFile) return;
    void intakeStore.pick();
  }

  async function handleCancelJob(jobId: string): Promise<void> {
    cancellingJobId = jobId;
    try {
      await jobsStore.cancel(jobId);
    } finally {
      cancellingJobId = null;
    }
  }

  // Job đang chạy, hoặc Job đầu hàng nếu chưa có Job chạy (spec Always: "Card
  // job ... cho Job đang chạy (hoặc Job đầu hàng nếu chưa có Job chạy)") —
  // `jobsStore.jobs` là một `Map` được dựng/cập nhật theo đúng thứ tự
  // snapshot/registry nên phần tử đầu tiên còn `queued` là job đầu hàng.
  const activeJob = $derived.by(() => {
    const jobs = Array.from(jobsStore.jobs.values());
    return jobs.find((job) => job.state === 'running') ?? jobs.find((job) => job.state === 'queued') ?? null;
  });

  // Trạng thái trống thật sự (spec Boundaries: "không có Phiên và không có
  // Job") chỉ được khẳng định sau khi cả `libraryStore` tải xong — trong lúc
  // tải (hay lỗi tải) luôn hiện bố cục "có phiên" (drop-zone mỏng + skeleton/
  // lỗi), không phải hai card lớn (spec I/O Matrix "Đang tải").
  // `jobsStore.synced` (hoặc lỗi subscribe) cũng phải chắc trước khi khẳng
  // định "không có Job" — tránh chớp hai card lớn khi danh sách đã tải xong
  // mà snapshot Job đầu tiên chưa tới.
  const jobsKnown = $derived(jobsStore.synced || jobsStore.status === 'error');
  const isEmpty = $derived(
    libraryStore.status === 'ready' && libraryStore.sessions.length === 0 && jobsKnown && !activeJob,
  );

  // Story 3.3, spec Always: "`⌘F` (Meta+F) và `Ctrl+F` focus ... ô tìm khi
  // đang ở Home" -- đăng ký lúc mount, gỡ lúc destroy/khi Home rơi vào trạng
  // thái trống thật sự (không có ô tìm để focus), theo đúng mẫu
  // `Session.svelte` (đăng ký trong `$effect`, cleanup trả về từ đó). Vì mỗi
  // route chỉ mount một component tại một thời điểm, `Session.svelte` gỡ
  // entry của nó khi rời màn nên hai màn không giành phím của nhau (Acceptance:
  // "đang ở route khác Home ... Home không chiếm phím").
  function focusSessionSearch(): void {
    sessionSearchRef?.focusAndSelect();
  }

  $effect(() => {
    if (isEmpty) return;
    const removeMetaFind = registerKeymap({ id: 'home-session-search-meta', combo: 'Meta+F', handler: focusSessionSearch });
    const removeControlFind = registerKeymap({ id: 'home-session-search-control', combo: 'Control+F', handler: focusSessionSearch });
    return () => {
      removeMetaFind();
      removeControlFind();
    };
  });

  // Loading is treated as "not yet confirmed missing" to avoid a flash of
  // the banner while `keysList` is still in flight; a load error is treated
  // conservatively as missing (spec I/O Matrix: no banner only once a usable
  // key is confirmed present).
  const showKeyBanner = $derived(
    keysStore.status !== 'idle'
      && keysStore.status !== 'loading'
      && (keysStore.status === 'error' || !keysStore.hasUsableKey),
  );

  const banners = $derived.by((): BannerItem[] =>
    showKeyBanner
      ? [
          {
            id: 'missing-key',
            variant: 'warning',
            title: i18n.t('home.banner.keyMissingTitle'),
            message: i18n.t('home.banner.keyMissingMessage'),
            actionLabel: i18n.t('home.banner.keyMissingAction'),
            actionHref: '/settings/gemini',
          },
        ]
      : [],
  );
</script>

<svelte:head>
  <title>{i18n.t('app.meta.homeTitle')}</title>
</svelte:head>

<section class="route-screen" aria-labelledby="home-title">
  <div class="screen-header">
    <div>
      <p class="route-kicker">{i18n.t('home.header.kicker')}</p>
      <h1 id="home-title">{i18n.t('home.header.title')}</h1>
    </div>
    {#if !isEmpty}
      <SessionSearch bind:this={sessionSearchRef} />
    {/if}
    <div class="header-actions">
      <a href="/live" use:link class="button button-primary">{i18n.t('home.live.action')}</a>
      {#if canChooseFile}
        <button type="button" class="button button-secondary" onclick={chooseFile}>
          {i18n.t('home.file.action')}
        </button>
      {:else}
        <DisabledHint
          reason={i18n.t('home.file.unavailable')}
          shortcut={i18n.t('home.banner.keyMissingAction')}
        >
          <span class="button button-secondary">{i18n.t('home.file.action')}</span>
        </DisabledHint>
      {/if}
      {#if isEmpty}
        <span class="status-pill status-off"><span class="status-dot" aria-hidden="true"></span> {i18n.t('home.header.emptyStatus')}</span>
      {/if}
    </div>
  </div>

  <div class="key-banner-wrap">
    <BannerStack {banners} />
  </div>

  <div class="intake-notices-wrap">
    <IntakeNotices />
  </div>

  {#if isEmpty}
    <div class="empty-grid">
      <article class="empty-card">
        <div class="empty-icon" aria-hidden="true"><UploadIcon size={18} strokeWidth={1.75} /></div>
        <h2>{i18n.t('home.file.title')}</h2>
        <p>{i18n.t('home.file.description')}</p>
        {#if canChooseFile}
          <button type="button" class="button button-secondary" onclick={chooseFile}>
            {i18n.t('home.file.action')}
          </button>
        {:else}
          <DisabledHint
            reason={i18n.t('home.file.unavailable')}
            shortcut={i18n.t('home.banner.keyMissingAction')}
          >
            <span class="button button-secondary">{i18n.t('home.file.action')}</span>
          </DisabledHint>
        {/if}
      </article>

      <article class="empty-card">
        <div class="empty-icon" aria-hidden="true"><RadioIcon size={18} strokeWidth={1.75} /></div>
        <h2>{i18n.t('home.live.title')}</h2>
        <p>{i18n.t('home.live.description')}</p>
        <a href="/live" use:link class="button button-primary">{i18n.t('home.live.action')}</a>
      </article>
    </div>

    <div class="empty-note" role="status">
      <InfoIcon size={18} strokeWidth={1.75} aria-hidden="true" />
      <p>{i18n.t('home.note.ready')}</p>
    </div>
  {:else}
    <div class="drop-zone-thin">
      <span class="drop-zone-hint">{i18n.t('home.dropZone.hint')}</span>
      {#if canChooseFile}
        <button type="button" class="button button-secondary" onclick={chooseFile}>
          {i18n.t('home.file.action')}
        </button>
      {:else}
        <DisabledHint
          reason={i18n.t('home.file.unavailable')}
          shortcut={i18n.t('home.banner.keyMissingAction')}
        >
          <span class="button button-secondary">{i18n.t('home.file.action')}</span>
        </DisabledHint>
      {/if}
    </div>

    {#if activeJob}
      <JobCard
        variant="full"
        job={activeJob}
        cancelling={cancellingJobId === activeJob.jobId}
        onCancel={handleCancelJob}
      />
    {/if}

    {#if libraryStore.status === 'loading'}
      <SessionListSkeleton />
    {:else if libraryStore.status === 'error'}
      <div class="list-error" role="status">
        <p>{i18n.t('home.list.loadError')}</p>
        <button type="button" class="button button-secondary" onclick={() => void libraryStore.load()}>
          {i18n.t('home.list.retryAction')}
        </button>
      </div>
    {:else}
      {#if libraryStore.reloadError}
        <p class="reload-error" role="status">{i18n.t('home.list.reloadError')}</p>
      {/if}
      <TagFilterBar />
      {#if libraryStore.sessions.length > 0 && libraryStore.filteredSessions.length === 0}
        <div class="filter-empty" role="status">
          <p>{i18n.t('home.list.filterEmpty')}</p>
          <button type="button" class="button button-secondary" onclick={() => libraryStore.clearAllFilters()}>
            {i18n.t('home.footer.clearFilters')}
          </button>
        </div>
      {:else}
        <div class="session-list-wrap">
          <SessionList sessions={libraryStore.filteredSessions} viewportHeight={sessionListViewportHeight} />
        </div>
      {/if}
      <SessionListFooter />
    {/if}
  {/if}
</section>

<style>
  .route-screen {
    max-width: 980px;
    margin: 0 auto;
    padding: var(--space-8);
  }

  .screen-header {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-4);
    margin-bottom: var(--space-6);
  }

  .route-kicker {
    margin: 0 0 var(--space-1);
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  h1 {
    margin: 0;
    font-size: var(--text-screen-title-size);
    line-height: 1.35;
  }

  h2 {
    margin: 0 0 var(--space-2);
    font-size: var(--text-h2-size);
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .intake-notices-wrap:not(:empty) {
    margin-bottom: var(--space-5);
  }

  .status-pill {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 30px;
    padding: 0 var(--space-3);
    border-radius: var(--radius-full);
    font-size: var(--text-label-size);
    font-weight: 600;
    white-space: nowrap;
  }

  .status-off {
    background: var(--color-surface-sunken);
    color: var(--color-text-secondary);
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: var(--radius-full);
    background: var(--color-text-muted);
  }

  .key-banner-wrap:not(:empty) {
    margin-bottom: var(--space-5);
  }

  .empty-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-4);
  }

  .empty-card {
    min-height: 216px;
    padding: var(--space-6);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
    background: var(--color-surface);
  }

  .empty-card p {
    min-height: 44px;
    margin: 0 0 var(--space-5);
    color: var(--color-text-secondary);
  }

  .empty-icon {
    display: grid;
    width: 32px;
    height: 32px;
    margin-bottom: var(--space-3);
    place-items: center;
    border-radius: var(--radius-md);
    background: var(--color-accent-soft);
    color: var(--color-accent);
    font-size: 20px;
    font-weight: 600;
  }

  .button {
    display: inline-flex;
    min-height: 36px;
    align-items: center;
    justify-content: center;
    padding: 0 14px;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    font-family: inherit;
    font-size: var(--text-body-size);
    font-weight: 500;
    cursor: pointer;
  }

  .button-primary {
    background: var(--color-primary-action);
    color: var(--color-on-primary);
  }

  .button-secondary {
    border-color: var(--color-border-strong);
    background: var(--color-surface);
    color: var(--color-text);
  }

  .empty-note {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    margin-top: var(--space-5);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    color: var(--color-text-secondary);
  }

  .empty-note :global(svg) {
    flex: 0 0 auto;
    color: var(--color-info);
  }

  .empty-note p {
    margin: 0;
  }

  .drop-zone-thin {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-3);
    margin-bottom: var(--space-4);
    padding: var(--space-4);
    border: 1px dashed var(--color-border-strong);
    border-radius: var(--radius-2xl);
  }

  .drop-zone-hint {
    color: var(--color-text-secondary);
    font-size: var(--text-body-size);
  }

  .list-error {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-6);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
    color: var(--color-text-secondary);
  }

  .list-error p {
    margin: 0;
  }

  .reload-error {
    margin: 0 0 var(--space-3);
    color: var(--color-warning);
    font-size: var(--text-help-size);
  }

  .filter-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-6);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
    color: var(--color-text-secondary);
    text-align: center;
  }

  .filter-empty p {
    margin: 0;
  }

  .session-list-wrap {
    height: 480px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
    overflow: hidden;
  }
</style>
