// Domain store `keys`: the only frontend owner of `keysList`/`keysSet`/
// `keysTest`/`modelsList`. Shared by Onboarding (the "Kiểm tra key" flow) and
// Home (the missing-key banner) so both read the same session-scoped view.
//
// "Có key hợp lệ" is never persisted — validity depends on the moment it was
// checked, so it is inferred purely from this run's data (spec Design
// Notes): no key -> missing; every key tested this session was rejected ->
// missing; any key not yet tested -> treated as present (no background
// network call).
import {
  commands,
  type AppError,
  type KeyId,
  type KeyMetadata,
  type ModelInfo,
  type ModelKind,
} from '../bindings';

export type KeysStatus = 'idle' | 'loading' | 'ready' | 'error';
export type CheckStatus = 'idle' | 'checking' | 'done';
export type ModelListStatus = 'idle' | 'loading' | 'ready' | 'error';

export type ApiKeyCheckOutcome =
  | { kind: 'success'; validCount: number; rejectedCount: number; modelCount: number | null }
  | { kind: 'error'; error: AppError };


/** Synthesized locally so an empty submission never reaches IPC (spec Always). */
const EMPTY_INPUT_ERROR: AppError = {
  category: 'format',
  code: 'format',
  detailRedacted: 'empty key input',
};
const KEY_CHECK_UNAVAILABLE_ERROR: AppError = {
  category: 'network',
  code: 'network',
  detailRedacted: 'key check unavailable',
};
const KEY_DELETE_UNAVAILABLE_ERROR: AppError = {
  category: 'storage',
  code: 'storage',
  detailRedacted: 'key delete unavailable',
};

function hasNonEmptySegment(raw: string): boolean {
  return raw.split(',').some((item) => item.trim().length > 0);
}

export function createKeysStore() {
  let keys = $state<KeyMetadata[]>([]);
  let status = $state<KeysStatus>('idle');
  let error = $state<AppError | null>(null);
  // Session-scoped only: cleared whenever `keysSet` replaces the list, and
  // never written to disk.
  let testedResults = $state<Map<KeyId, boolean>>(new Map());
  let checkStatus = $state<CheckStatus>('idle');
  let checkResult = $state<ApiKeyCheckOutcome | null>(null);
  // Local time of the most recent "Kiểm tra key" completion (success or
  // error) so Settings → Gemini can render it next to the status region
  // (spec I/O Matrix: "Key hợp lệ, N model khả dụng · HH:MM").
  let lastCheckedAt = $state<Date | null>(null);
  let activeLoad: Promise<void> | undefined;
  let mutationGeneration = 0;
  // Session-only, per model kind (spec Design Notes: "Danh sách đã tải chỉ
  // giữ trong phiên chạy") — never persisted, cleared by `reset()`.
  let modelLists = $state<Record<ModelKind, ModelInfo[]>>({ transcribe: [], live: [], memo: [] });
  let modelListStatus = $state<Record<ModelKind, ModelListStatus>>({
    transcribe: 'idle',
    live: 'idle',
    memo: 'idle',
  });
  let modelListError = $state<Record<ModelKind, AppError | null>>({
    transcribe: null,
    live: null,
    memo: null,
  });

  const hasUsableKey = $derived.by(() => {
    if (keys.length === 0) return false;
    const allTested = keys.every((key) => testedResults.has(key.id));
    if (!allTested) return true;
    return keys.some((key) => testedResults.get(key.id) === true);
  });

  /** Home calls this to render (or skip) the missing-key banner. */
  function load(): Promise<void> {
    if (activeLoad) return activeLoad;
    const generation = mutationGeneration;
    status = 'loading';
    const request = (async () => {
      try {
        const result = await commands.keysList();
        if (generation !== mutationGeneration) return;
        if (result.status === 'ok') {
          keys = result.data;
          error = null;
          status = 'ready';
        } else {
          error = result.error;
          status = 'error';
        }
      } catch {
        if (generation !== mutationGeneration) return;
        error = null;
        status = 'error';
      }
    })().finally(() => {
      if (activeLoad === request) activeLoad = undefined;
    });
    activeLoad = request;
    return request;
  }

  /** Every `checkKeys` exit path goes through here so `lastCheckedAt` always
   * reflects the most recent completed check, success or error alike. */
  function finishCheck(outcome: ApiKeyCheckOutcome): ApiKeyCheckOutcome {
    checkStatus = 'done';
    checkResult = outcome;
    lastCheckedAt = new Date();
    return outcome;
  }

  /**
   * The Onboarding "Kiểm tra key" flow, reused as-is by Settings → Gemini:
   * `keysSet(input)` → `keysTest` per returned id → if ≥1 valid,
   * `modelsList('transcribe')` for the count (spec Always). Returns the
   * outcome so the caller can render it without depending on this store
   * being reactive in every consumer.
   */
  async function checkKeys(rawInput: string): Promise<ApiKeyCheckOutcome> {
    if (!hasNonEmptySegment(rawInput)) {
      return finishCheck({ kind: 'error', error: EMPTY_INPUT_ERROR });
    }

    checkStatus = 'checking';
    checkResult = null;

    try {
      const setResult = await commands.keysSet(rawInput);
      if (setResult.status === 'error') {
        return finishCheck({ kind: 'error', error: setResult.error });
      }

      keys = setResult.data;
      mutationGeneration += 1;
      status = 'ready';
      error = null;
      // `keysSet` is replace-all: a fresh list starts a fresh test round
      // (Design Notes — re-checking after an edit replaces prior results).
      testedResults = new Map();

      const outcomes = await Promise.all(
        keys.map(async (key) => ({ id: key.id, result: await commands.keysTest(key.id) })),
      );

      const nextTested = new Map<KeyId, boolean>();
      let lastError: AppError | null = null;
      for (const { id, result } of outcomes) {
        if (result.status === 'ok') {
          nextTested.set(id, true);
        } else {
          nextTested.set(id, false);
          lastError = result.error;
        }
      }
      testedResults = nextTested;

      const validCount = keys.filter((key) => nextTested.get(key.id) === true).length;
      const rejectedCount = keys.length - validCount;

      if (validCount === 0) {
        return finishCheck({ kind: 'error', error: lastError ?? EMPTY_INPUT_ERROR });
      }

      const modelsResult = await commands.modelsList('transcribe');
      const modelCount = modelsResult.status === 'ok' ? modelsResult.data.length : null;
      return finishCheck({ kind: 'success', validCount, rejectedCount, modelCount });
    } catch {
      return finishCheck({ kind: 'error', error: KEY_CHECK_UNAVAILABLE_ERROR });
    }
  }

  /**
   * Delete one stored key from the real OS key store (spec Always: "mỗi key
   * có nút xoá gọi `keysDelete` (xoá khỏi kho khoá OS thật)"). On success the
   * list and `hasUsableKey` recompute from the server's authoritative
   * remaining list; on error nothing here changes so the list "giữ nguyên"
   * (spec I/O Matrix "Xoá key").
   */
  async function deleteKey(id: KeyId): Promise<AppError | null> {
    let result: Awaited<ReturnType<typeof commands.keysDelete>>;
    try {
      result = await commands.keysDelete(id);
    } catch {
      error = KEY_DELETE_UNAVAILABLE_ERROR;
      return error;
    }
    if (result.status === 'error') {
      error = result.error;
      return result.error;
    }
    keys = result.data;
    mutationGeneration += 1;
    const nextTested = new Map(testedResults);
    nextTested.delete(id);
    testedResults = nextTested;
    error = null;
    return null;
  }

  /**
   * "Tải danh sách" gọi `modelsList(kind)` theo từng select (spec Always).
   * Busy state is scoped to the one kind so the other two selects stay
   * unaffected; a load failure never touches the currently configured model
   * value (spec I/O Matrix "Tải lỗi").
   */
  async function loadModelList(kind: ModelKind): Promise<void> {
    if (modelListStatus[kind] === 'loading') return;
    modelListStatus = { ...modelListStatus, [kind]: 'loading' };
    modelListError = { ...modelListError, [kind]: null };
    try {
      const result = await commands.modelsList(kind);
      if (result.status === 'ok') {
        modelLists = { ...modelLists, [kind]: result.data };
        modelListStatus = { ...modelListStatus, [kind]: 'ready' };
      } else {
        modelListError = { ...modelListError, [kind]: result.error };
        modelListStatus = { ...modelListStatus, [kind]: 'error' };
      }
    } catch {
      modelListError = {
        ...modelListError,
        [kind]: { category: 'network', code: 'network', detailRedacted: 'models list unavailable' },
      };
      modelListStatus = { ...modelListStatus, [kind]: 'error' };
    }
  }

  /** Test-only seam: clears all session state between isolated test cases. */
  function reset(): void {
    mutationGeneration += 1;
    keys = [];
    status = 'idle';
    error = null;
    testedResults = new Map();
    checkStatus = 'idle';
    checkResult = null;
    lastCheckedAt = null;
    activeLoad = undefined;
    modelLists = { transcribe: [], live: [], memo: [] };
    modelListStatus = { transcribe: 'idle', live: 'idle', memo: 'idle' };
    modelListError = { transcribe: null, live: null, memo: null };
  }

  return {
    get keys() { return keys; },
    get status() { return status; },
    get error() { return error; },
    get checkStatus() { return checkStatus; },
    get checkResult() { return checkResult; },
    get lastCheckedAt() { return lastCheckedAt; },
    get hasUsableKey() { return hasUsableKey; },
    get modelLists() { return modelLists; },
    get modelListStatus() { return modelListStatus; },
    get modelListError() { return modelListError; },
    load,
    checkKeys,
    deleteKey,
    loadModelList,
    reset,
  };
}

export const keysStore = createKeysStore();
