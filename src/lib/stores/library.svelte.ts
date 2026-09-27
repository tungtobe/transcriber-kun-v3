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
import {
  commands,
  type AppError,
  type SessionDeleteOutcome,
  type SessionListItem,
  type TagSummary,
  type TagWithCount,
  type WipeAllOutcome,
} from '../bindings';
import { EMPTY_TAG_FILTER, filterSessions, type TagFilterState } from '../session-filter';

export type LibraryStatus = 'loading' | 'ready' | 'error';

const LOAD_UNAVAILABLE_ERROR: AppError = {
  category: 'network',
  code: 'network',
  detailRedacted: 'library sessions list unavailable',
};

const RENAME_UNAVAILABLE_ERROR: AppError = {
  category: 'network',
  code: 'network',
  detailRedacted: 'library session rename unavailable',
};

const DELETE_UNAVAILABLE_ERROR: AppError = {
  category: 'network',
  code: 'network',
  detailRedacted: 'library session delete unavailable',
};

const CREATE_TAG_UNAVAILABLE_ERROR: AppError = {
  category: 'network',
  code: 'network',
  detailRedacted: 'tag create unavailable',
};

const ATTACH_TAG_UNAVAILABLE_ERROR: AppError = {
  category: 'network',
  code: 'network',
  detailRedacted: 'tag attach unavailable',
};

const DETACH_TAG_UNAVAILABLE_ERROR: AppError = {
  category: 'network',
  code: 'network',
  detailRedacted: 'tag detach unavailable',
};

const DELETE_TAG_UNAVAILABLE_ERROR: AppError = {
  category: 'network',
  code: 'network',
  detailRedacted: 'tag delete unavailable',
};

const WIPE_ALL_UNAVAILABLE_ERROR: AppError = {
  category: 'network',
  code: 'network',
  detailRedacted: 'library wipe all unavailable',
};

export type RenameResult =
  | { status: 'ok'; title: string | null }
  | { status: 'error'; error: AppError };

export type RemoveResult =
  | { status: 'ok'; outcome: SessionDeleteOutcome }
  | { status: 'error'; error: AppError };

export type CreateTagResult =
  | { status: 'ok'; tag: TagSummary }
  | { status: 'error'; error: AppError };

export type TagActionResult = { status: 'ok' } | { status: 'error'; error: AppError };

export type WipeAllResult =
  | { status: 'ok'; outcome: WipeAllOutcome }
  | { status: 'error'; error: AppError };

export function createLibraryStore() {
  let sessions = $state<SessionListItem[]>([]);
  let status = $state<LibraryStatus>('loading');
  let error = $state<AppError | null>(null);
  // `true` right after a background reload (triggered by a Job commit, not
  // the first load) fails — the old `sessions` list stays on screen (spec
  // I/O Matrix "Commit": "Tải lại lỗi → giữ danh sách cũ + thông báo").
  let reloadError = $state(false);

  // Story 3.2: mọi tag kèm số Phiên (dùng bởi `TagPicker`/`TagFilterBar`/
  // chip tag trong dòng) và bộ lọc tag hiện tại của Home (spec Boundaries
  // Always: "Trạng thái lọc sống trong `libraryStore` (không persist)").
  let tags = $state<TagWithCount[]>([]);
  let tagFilter = $state<TagFilterState>({ ...EMPTY_TAG_FILTER });

  // Story 3.3 (spec-3-3-tim-phien-theo-ten-ket-hop-loc-tag.md): chuỗi tìm
  // theo tên, sống cạnh `tagFilter` -- không persist (spec Always: "Query
  // sống trong `libraryStore` cạnh `tagFilter` (không persist)"). Chưa
  // chuẩn hoá ở đây -- `filterSessions` tự chuẩn hoá qua `normalizeSearchText`
  // khi so khớp, để `nameQuery` phản ánh đúng những gì người dùng đã gõ.
  let nameQuery = $state('');

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

  // Story 3.1: đổi tên tại chỗ (spec Code Map: "rename(id, title) (cập nhật
  // sessions tại chỗ)"). Chỉ cập nhật mảng `sessions` khi Rust trả một tên đã
  // chuẩn hoá (`Ok(Some(title))`) — `Ok(None)` (Phiên vừa bị xoá đồng thời)
  // không phải lỗi nhưng cũng không có gì để cập nhật, để nguyên danh sách
  // (spec I/O Matrix: dòng đó biến mất qua đường xoá/reload bình thường).
  async function rename(id: string, title: string): Promise<RenameResult> {
    try {
      const result = await commands.librarySessionRename(id, title);
      if (result.status !== 'ok') {
        return { status: 'error', error: result.error };
      }
      if (result.data !== null) {
        const newTitle = result.data;
        sessions = sessions.map((session) =>
          session.sessionId === id ? { ...session, title: newTitle } : session,
        );
      }
      return { status: 'ok', title: result.data };
    } catch {
      return { status: 'error', error: RENAME_UNAVAILABLE_ERROR };
    }
  }

  // Story 3.1: xoá hẳn một Phiên (spec Code Map: "remove(id) (trả outcome,
  // reload khi Deleted)"). `Busy`/lỗi không đổi gì ở đây — caller
  // (`SessionRow`/`SessionHeader`) hiển thị giải thích inline, không mở lỗi
  // chung (spec Always).
  async function remove(id: string): Promise<RemoveResult> {
    try {
      const result = await commands.librarySessionDelete(id);
      if (result.status !== 'ok') {
        return { status: 'error', error: result.error };
      }
      if (result.data === 'deleted') {
        void load();
      }
      return { status: 'ok', outcome: result.data };
    } catch {
      return { status: 'error', error: DELETE_UNAVAILABLE_ERROR };
    }
  }

  // Story 3.4: xoá toàn bộ dữ liệu họp (spec Code Map: "reload phiên + tag
  // sau wipe"). Outcome `wiped` tải lại cả `sessions` lẫn `tags` -- Rust đã
  // xoá sạch mọi Phiên/Tag trong cùng một transaction nên cả hai danh sách
  // đều cần làm mới cùng lúc (spec Always: "frontend reload danh sách Phiên
  // và tag, và cập nhật số liệu"). `busy`/lỗi không đổi gì ở đây --
  // `SettingsStorage` hiển thị giải thích inline, không mở lỗi chung.
  async function wipeAll(): Promise<WipeAllResult> {
    try {
      const result = await commands.libraryWipeAll();
      if (result.status !== 'ok') {
        return { status: 'error', error: result.error };
      }
      if (result.data === 'wiped') {
        void load();
        void loadTags();
      }
      return { status: 'ok', outcome: result.data };
    } catch {
      return { status: 'error', error: WIPE_ALL_UNAVAILABLE_ERROR };
    }
  }

  // Story 3.2: tải mọi tag kèm số Phiên (spec Code Map: "thêm `tags` (list +
  // count)"). Lỗi tải chỉ giữ danh sách cũ -- không có banner riêng cho việc
  // này, `TagPicker`/`TagFilterBar` vẫn dùng được với danh sách đã có (hoặc
  // rỗng lúc đầu, hiển thị "Gõ để tạo tag đầu tiên").
  async function loadTags(): Promise<void> {
    try {
      const result = await commands.tagsList();
      if (result.status === 'ok') {
        tags = result.data;
      }
    } catch {
      // giữ nguyên `tags` hiện tại.
    }
  }

  // Tạo tag mới hoặc lấy lại tag đã có cùng tên chuẩn hoá (spec I/O Matrix
  // "Trùng hoa thường"/"Tạo đồng thời") -- thêm vào `tags` tại chỗ (count 0)
  // nếu Rust trả một id chưa có trong danh sách hiện tại, tránh phải tải lại
  // toàn bộ chỉ để hiện tag vừa tạo trong picker.
  // Story 3.2, spec Design Notes: "NFC làm ở frontend vì không được thêm
  // crate chuẩn hoá" -- Rust chuẩn hoá khoảng trắng + lowercase nhưng không
  // NFC (Unicode) mọi tổ hợp ký tự tương đương thị giác nhưng khác chuỗi
  // byte (ví dụ dấu kết hợp) thành cùng một `name_key`, nên bước NFC phải
  // chạy trước khi gửi.
  async function createTag(name: string): Promise<CreateTagResult> {
    try {
      const result = await commands.tagsCreate(name.normalize('NFC'));
      if (result.status !== 'ok') {
        return { status: 'error', error: result.error };
      }
      const tag = result.data;
      if (!tags.some((existing) => existing.id === tag.id)) {
        tags = [...tags, { id: tag.id, name: tag.name, sessionCount: 0 }];
      }
      return { status: 'ok', tag };
    } catch {
      return { status: 'error', error: CREATE_TAG_UNAVAILABLE_ERROR };
    }
  }

  // Gắn tag cho một Phiên (spec Acceptance: "gắn tag cho Phiên khi Home đang
  // lọc, khi gắn xong, bộ lọc Home không đổi" -- hàm này không chạm
  // `tagFilter`). Cập nhật `sessions`/`tags` tại chỗ chỉ khi liên kết thật sự
  // mới (Rust coi gắn lại một tag đã có là no-op -- không tăng đếm hai lần).
  async function attachTag(sessionId: string, tagId: string): Promise<TagActionResult> {
    try {
      const result = await commands.sessionTagsAttach(sessionId, tagId);
      if (result.status !== 'ok') {
        return { status: 'error', error: result.error };
      }
      const alreadyAttached = sessions.some(
        (session) => session.sessionId === sessionId && session.tagIds.includes(tagId),
      );
      if (!alreadyAttached) {
        sessions = sessions.map((session) =>
          session.sessionId === sessionId
            ? { ...session, tagIds: [...session.tagIds, tagId] }
            : session,
        );
        tags = tags.map((tag) =>
          tag.id === tagId ? { ...tag, sessionCount: tag.sessionCount + 1 } : tag,
        );
      }
      return { status: 'ok' };
    } catch {
      return { status: 'error', error: ATTACH_TAG_UNAVAILABLE_ERROR };
    }
  }

  /** Gỡ một tag khỏi một Phiên -- cập nhật `sessions`/`tags` tại chỗ. */
  async function detachTag(sessionId: string, tagId: string): Promise<TagActionResult> {
    try {
      const result = await commands.sessionTagsDetach(sessionId, tagId);
      if (result.status !== 'ok') {
        return { status: 'error', error: result.error };
      }
      const hadTag = sessions.some(
        (session) => session.sessionId === sessionId && session.tagIds.includes(tagId),
      );
      if (hadTag) {
        sessions = sessions.map((session) =>
          session.sessionId === sessionId
            ? { ...session, tagIds: session.tagIds.filter((id) => id !== tagId) }
            : session,
        );
        tags = tags.map((tag) =>
          tag.id === tagId ? { ...tag, sessionCount: Math.max(0, tag.sessionCount - 1) } : tag,
        );
      }
      return { status: 'ok' };
    } catch {
      return { status: 'error', error: DETACH_TAG_UNAVAILABLE_ERROR };
    }
  }

  // Xoá hẳn một tag toàn cục (spec I/O Matrix "Xoá tag toàn cục"): gỡ khỏi
  // `tags`, khỏi `tagIds` của mọi Phiên trong `sessions`, và khỏi bộ lọc nếu
  // đang lọc theo nó (spec Boundaries Always: "gỡ khỏi mọi Phiên trong một
  // transaction" ở Rust; "gỡ khỏi bộ lọc" ở I/O Matrix).
  async function deleteTagGlobally(tagId: string): Promise<TagActionResult> {
    try {
      const result = await commands.tagsDelete(tagId);
      if (result.status !== 'ok') {
        return { status: 'error', error: result.error };
      }
      tags = tags.filter((tag) => tag.id !== tagId);
      sessions = sessions.map((session) =>
        session.tagIds.includes(tagId)
          ? { ...session, tagIds: session.tagIds.filter((id) => id !== tagId) }
          : session,
      );
      if (tagFilter.tagIds.includes(tagId)) {
        tagFilter = { ...tagFilter, tagIds: tagFilter.tagIds.filter((id) => id !== tagId) };
      }
      return { status: 'ok' };
    } catch {
      return { status: 'error', error: DELETE_TAG_UNAVAILABLE_ERROR };
    }
  }

  // Bật/tắt lọc theo một tag cụ thể (spec I/O Matrix "Chưa gắn tag": "chọn A
  // lại tắt Chưa gắn tag") -- chọn bất kỳ tag cụ thể nào luôn tắt `untagged`.
  function toggleFilterTag(tagId: string): void {
    const active = tagFilter.tagIds.includes(tagId);
    tagFilter = {
      tagIds: active ? tagFilter.tagIds.filter((id) => id !== tagId) : [...tagFilter.tagIds, tagId],
      untagged: false,
    };
  }

  // Bật/tắt "Chưa gắn tag" (spec I/O Matrix "Chưa gắn tag": "bật nó xoá tag
  // lọc") -- bật luôn xoá mọi `tagIds` đang lọc.
  function toggleUntaggedFilter(): void {
    tagFilter = tagFilter.untagged ? { tagIds: [], untagged: false } : { tagIds: [], untagged: true };
  }

  function clearTagFilter(): void {
    tagFilter = { tagIds: [], untagged: false };
  }

  // Story 3.3: gõ vào ô tìm cập nhật ngay (không debounce -- spec Always:
  // "Danh sách cập nhật khi gõ").
  function setNameQuery(query: string): void {
    nameQuery = query;
  }

  // Nút × / Esc trong ô tìm khi có query (spec Always: "giữ nguyên bộ lọc
  // tag") -- chỉ xoá `nameQuery`, không đụng `tagFilter`.
  function clearNameQuery(): void {
    nameQuery = '';
  }

  // Nút "Xoá bộ lọc" ở footer và trạng thái rỗng (spec Always: "xoá cả query
  // lẫn tag").
  function clearAllFilters(): void {
    nameQuery = '';
    tagFilter = { tagIds: [], untagged: false };
  }

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
    tags = [];
    tagFilter = { tagIds: [], untagged: false };
    nameQuery = '';
  }

  return {
    get sessions() {
      return sessions;
    },
    get filteredSessions() {
      return filterSessions(sessions, { ...tagFilter, query: nameQuery });
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
    get tags() {
      return tags;
    },
    get tagFilter() {
      return tagFilter;
    },
    get nameQuery() {
      return nameQuery;
    },
    load,
    rename,
    remove,
    wipeAll,
    loadTags,
    createTag,
    attachTag,
    detachTag,
    deleteTagGlobally,
    toggleFilterTag,
    toggleUntaggedFilter,
    clearTagFilter,
    setNameQuery,
    clearNameQuery,
    clearAllFilters,
    reset,
  };
}

export const libraryStore = createLibraryStore();
