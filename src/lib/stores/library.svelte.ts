// Store domain `library` (story 2.9): the only frontend owner of
// `librarySessionsList` for Home's session list. Auto-reloads whenever
// `jobsStore.resultSeq` changes (spec Code Map: "tự tải lại khi
// `jobsStore.resultSeq` đổi") — a Job committing (Transcribe or Chạy lại)
// makes the new Phiên appear at the top and the job card disappear without
// the user doing anything (spec Always).
//
// `status` only ever reflects the *first* load (spec I/O Matrix "Đang tải" /
// "Commit"): a background reload triggered by a Job result failing must keep
// the previously loaded list on screen, not blank it into an error state —
// that failure surfaces through `reloadError` instead, which `Home.svelte`
// renders as a transient notice next to the (still-visible) old list.
import { jobsStore } from './jobs.svelte';
import { commands, type AppError, type SessionListItem } from '../bindings';

export type LibraryStatus = 'loading' | 'ready' | 'error';

const LOAD_UNAVAILABLE_ERROR: AppError = {
  category: 'network',
  code: 'network',
  detailRedacted: 'library sessions list unavailable',
};

export function createLibraryStore() {
  let sessions = $state<SessionListItem[]>([]);
  let status = $state<LibraryStatus>('loading');
  let error = $state<AppError | null>(null);
  // `true` right after a background reload (triggered by a Job commit, not
  // the first load) fails — the old `sessions` list stays on screen (spec
  // I/O Matrix "Commit": "Tải lại lỗi → giữ danh sách cũ + thông báo").
  let reloadError = $state(false);

  let hasLoadedOnce = false;
  let activeLoad: Promise<void> | undefined;

  // Một lần tải lại được yêu cầu trong khi lần tải trước còn bay (ví dụ Job
  // commit ngay lúc tải lần đầu) — kết quả đang bay có thể đã cũ, nên chạy
  // thêm đúng một lần sau khi nó xong thay vì trả chung promise cũ.
  let reloadQueued: Promise<void> | undefined;

  function load(): Promise<void> {
    if (activeLoad) {
      reloadQueued ??= activeLoad.then(() => {
        reloadQueued = undefined;
        return load();
      });
      return reloadQueued;
    }
    const isFirstLoad = !hasLoadedOnce;
    if (isFirstLoad) {
      status = 'loading';
      error = null;
    }
    const request = (async () => {
      try {
        const result = await commands.librarySessionsList();
        if (result.status === 'ok') {
          sessions = result.data;
          status = 'ready';
          error = null;
          reloadError = false;
          hasLoadedOnce = true;
        } else if (isFirstLoad) {
          status = 'error';
          error = result.error;
        } else {
          reloadError = true;
        }
      } catch {
        if (isFirstLoad) {
          status = 'error';
          error = LOAD_UNAVAILABLE_ERROR;
        } else {
          reloadError = true;
        }
      }
    })().finally(() => {
      if (activeLoad === request) activeLoad = undefined;
    });
    activeLoad = request;
    return request;
  }

  // Theo dõi `jobsStore.resultSeq` (một Job vừa commit) và tự tải lại — bộ
  // đếm bắt đầu từ giá trị hiện tại của `jobsStore` để việc mount store này
  // không tự kích một lần tải lại giả (chỉ `load()` gọi tường minh từ
  // `Home.svelte` mới tải lần đầu).
  let lastHandledResultSeq = jobsStore.resultSeq;
  $effect.root(() => {
    $effect(() => {
      const seq = jobsStore.resultSeq;
      if (seq !== lastHandledResultSeq) {
        lastHandledResultSeq = seq;
        void load();
      }
    });
  });

  /** Test-only seam: resets every field without touching a live request. */
  function reset(): void {
    sessions = [];
    status = 'loading';
    error = null;
    reloadError = false;
    hasLoadedOnce = false;
    activeLoad = undefined;
    reloadQueued = undefined;
    lastHandledResultSeq = jobsStore.resultSeq;
  }

  return {
    get sessions() {
      return sessions;
    },
    get status() {
      return status;
    },
    get error() {
      return error;
    },
    get reloadError() {
      return reloadError;
    },
    load,
    reset,
  };
}

export const libraryStore = createLibraryStore();
