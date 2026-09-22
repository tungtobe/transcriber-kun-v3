// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  keysList: vi.fn(),
  keysSet: vi.fn(),
  keysTest: vi.fn(),
  modelsList: vi.fn(),
}));

vi.mock('../bindings', () => ({
  commands: {
    keysList: (...args: unknown[]) => mocks.keysList(...args),
    keysSet: (...args: unknown[]) => mocks.keysSet(...args),
    keysTest: (...args: unknown[]) => mocks.keysTest(...args),
    modelsList: (...args: unknown[]) => mocks.modelsList(...args),
  },
}));

function key(id: string, label = `AIza••••${id}`) {
  return { id, label };
}

describe('keysStore', () => {
  beforeEach(() => {
    vi.resetModules();
    mocks.keysList.mockReset();
    mocks.keysSet.mockReset();
    mocks.keysTest.mockReset();
    mocks.modelsList.mockReset();
  });

  it('blocks a check locally when the input is empty and never calls IPC', async () => {
    const { createKeysStore } = await import('./keys.svelte');
    const store = createKeysStore();

    const outcome = await store.checkKeys('   ');

    expect(outcome).toEqual({ kind: 'error', error: { category: 'format', code: 'format', detailRedacted: 'empty key input' } });
    expect(mocks.keysSet).not.toHaveBeenCalled();
    expect(store.hasUsableKey).toBe(false);
  });

  it('surfaces a keysSet format error without calling keysTest', async () => {
    mocks.keysSet.mockResolvedValue({
      status: 'error',
      error: { category: 'format', code: 'format', detailRedacted: 'bad format' },
    });
    const { createKeysStore } = await import('./keys.svelte');
    const store = createKeysStore();

    const outcome = await store.checkKeys('not-a-key');

    expect(outcome).toEqual({ kind: 'error', error: { category: 'format', code: 'format', detailRedacted: 'bad format' } });
    expect(mocks.keysTest).not.toHaveBeenCalled();
    expect(mocks.modelsList).not.toHaveBeenCalled();
  });

  it('reports a valid key and the transcribe model count', async () => {
    mocks.keysSet.mockResolvedValue({ status: 'ok', data: [key('k1')] });
    mocks.keysTest.mockResolvedValue({ status: 'ok', data: { keyId: 'k1', valid: true } });
    mocks.modelsList.mockResolvedValue({
      status: 'ok',
      data: [{ name: 'models/gemini-1', displayName: 'Gemini 1', supportedGenerationMethods: [] }],
    });
    const { createKeysStore } = await import('./keys.svelte');
    const store = createKeysStore();

    const outcome = await store.checkKeys('AIzaSomeValidKey');

    expect(outcome).toEqual({ kind: 'success', validCount: 1, rejectedCount: 0, modelCount: 1 });
    expect(mocks.modelsList).toHaveBeenCalledWith('transcribe');
    expect(store.hasUsableKey).toBe(true);
  });

  it('reports a partial rejection but still counts as usable and does not block continuing', async () => {
    mocks.keysSet.mockResolvedValue({ status: 'ok', data: [key('k1'), key('k2')] });
    mocks.keysTest.mockImplementation(async (id: string) =>
      id === 'k1'
        ? { status: 'ok', data: { keyId: 'k1', valid: true } }
        : { status: 'error', error: { category: 'auth', code: 'auth', detailRedacted: 'rejected' } },
    );
    mocks.modelsList.mockResolvedValue({ status: 'ok', data: [] });
    const { createKeysStore } = await import('./keys.svelte');
    const store = createKeysStore();

    const outcome = await store.checkKeys('AIzaOne,AIzaTwo');

    expect(outcome).toEqual({ kind: 'success', validCount: 1, rejectedCount: 1, modelCount: 0 });
    expect(store.hasUsableKey).toBe(true);
  });

  it('reports a category error and stays without a usable key when every key fails', async () => {
    mocks.keysSet.mockResolvedValue({ status: 'ok', data: [key('k1')] });
    mocks.keysTest.mockResolvedValue({
      status: 'error',
      error: { category: 'quota', code: 'quota', detailRedacted: 'quota exceeded' },
    });
    const { createKeysStore } = await import('./keys.svelte');
    const store = createKeysStore();

    const outcome = await store.checkKeys('AIzaSomeKey');

    expect(outcome).toEqual({ kind: 'error', error: { category: 'quota', code: 'quota', detailRedacted: 'quota exceeded' } });
    expect(mocks.modelsList).not.toHaveBeenCalled();
    expect(store.hasUsableKey).toBe(false);
  });

  it('treats a not-yet-tested existing key as usable without any network call', async () => {
    mocks.keysList.mockResolvedValue({ status: 'ok', data: [key('k1')] });
    const { createKeysStore } = await import('./keys.svelte');
    const store = createKeysStore();

    await store.load();

    expect(store.hasUsableKey).toBe(true);
    expect(mocks.keysTest).not.toHaveBeenCalled();
    expect(mocks.modelsList).not.toHaveBeenCalled();
  });

  it('treats an empty key list as not usable', async () => {
    mocks.keysList.mockResolvedValue({ status: 'ok', data: [] });
    const { createKeysStore } = await import('./keys.svelte');
    const store = createKeysStore();

    await store.load();

    expect(store.hasUsableKey).toBe(false);
    expect(store.status).toBe('ready');
  });

  it('resets tested results after a replace-all keysSet so a fresh list needs a fresh check', async () => {
    mocks.keysSet
      .mockResolvedValueOnce({ status: 'ok', data: [key('k1')] })
      .mockResolvedValueOnce({ status: 'ok', data: [key('k2')] });
    mocks.keysTest.mockResolvedValue({
      status: 'error',
      error: { category: 'auth', code: 'auth', detailRedacted: 'rejected' },
    });
    const { createKeysStore } = await import('./keys.svelte');
    const store = createKeysStore();

    await store.checkKeys('AIzaOne');
    expect(store.hasUsableKey).toBe(false);

    await store.checkKeys('AIzaTwo');
    // k2 has not been tested in this round yet at the moment keysSet replaces
    // the list; by the time checkKeys resolves it has been tested and
    // rejected, so hasUsableKey stays false — but k1's stale result must not
    // leak into this round.
    expect(store.keys).toEqual([key('k2')]);
    expect(store.hasUsableKey).toBe(false);
  });
});
