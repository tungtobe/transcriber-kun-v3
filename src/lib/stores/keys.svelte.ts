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
} from '../bindings';

export type KeysStatus = 'idle' | 'loading' | 'ready' | 'error';
export type CheckStatus = 'idle' | 'checking' | 'done';

export type ApiKeyCheckOutcome =
  | { kind: 'success'; validCount: number; rejectedCount: number; modelCount: number | null }
  | { kind: 'error'; error: AppError };

/** Synthesized locally so an empty submission never reaches IPC (spec Always). */
const EMPTY_INPUT_ERROR: AppError = {
  category: 'format',
  code: 'format',
  detailRedacted: 'empty key input',
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
  let activeLoad: Promise<void> | undefined;

  const hasUsableKey = $derived.by(() => {
    if (keys.length === 0) return false;
    const allTested = keys.every((key) => testedResults.has(key.id));
    if (!allTested) return true;
    return keys.some((key) => testedResults.get(key.id) === true);
  });

  /** Home calls this to render (or skip) the missing-key banner. */
  function load(): Promise<void> {
    if (activeLoad) return activeLoad;
    status = 'loading';
    const request = (async () => {
      try {
        const result = await commands.keysList();
        if (result.status === 'ok') {
          keys = result.data;
          error = null;
          status = 'ready';
        } else {
          error = result.error;
          status = 'error';
        }
      } catch {
        error = null;
        status = 'error';
      }
    })().finally(() => {
      if (activeLoad === request) activeLoad = undefined;
    });
    activeLoad = request;
    return request;
  }

  /**
   * The Onboarding "Kiểm tra key" flow: `keysSet(input)` → `keysTest` per
   * returned id → if ≥1 valid, `modelsList('transcribe')` for the count
   * (spec Always). Returns the outcome so the caller can render it without
   * depending on this store being reactive in every consumer.
   */
  async function checkKeys(rawInput: string): Promise<ApiKeyCheckOutcome> {
    if (!hasNonEmptySegment(rawInput)) {
      const outcome: ApiKeyCheckOutcome = { kind: 'error', error: EMPTY_INPUT_ERROR };
      checkStatus = 'done';
      checkResult = outcome;
      return outcome;
    }

    checkStatus = 'checking';
    checkResult = null;

    const setResult = await commands.keysSet(rawInput);
    if (setResult.status === 'error') {
      const outcome: ApiKeyCheckOutcome = { kind: 'error', error: setResult.error };
      checkStatus = 'done';
      checkResult = outcome;
      return outcome;
    }

    keys = setResult.data;
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
      const outcome: ApiKeyCheckOutcome = {
        kind: 'error',
        error: lastError ?? EMPTY_INPUT_ERROR,
      };
      checkStatus = 'done';
      checkResult = outcome;
      return outcome;
    }

    const modelsResult = await commands.modelsList('transcribe');
    const modelCount = modelsResult.status === 'ok' ? modelsResult.data.length : null;
    const outcome: ApiKeyCheckOutcome = { kind: 'success', validCount, rejectedCount, modelCount };
    checkStatus = 'done';
    checkResult = outcome;
    return outcome;
  }

  /** Test-only seam: clears all session state between isolated test cases. */
  function reset(): void {
    keys = [];
    status = 'idle';
    error = null;
    testedResults = new Map();
    checkStatus = 'idle';
    checkResult = null;
    activeLoad = undefined;
  }

  return {
    get keys() { return keys; },
    get status() { return status; },
    get error() { return error; },
    get checkStatus() { return checkStatus; },
    get checkResult() { return checkResult; },
    get hasUsableKey() { return hasUsableKey; },
    load,
    checkKeys,
    reset,
  };
}

export const keysStore = createKeysStore();
