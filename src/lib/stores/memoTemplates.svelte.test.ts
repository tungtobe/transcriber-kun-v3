// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  memoTemplatesList: vi.fn(),
  memoTemplateCreate: vi.fn(),
  memoTemplateUpdate: vi.fn(),
  memoTemplateDelete: vi.fn(),
  memoTemplatesRestoreDefaults: vi.fn(),
}));

vi.mock('../bindings', () => ({
  commands: {
    memoTemplatesList: (...args: unknown[]) => mocks.memoTemplatesList(...args),
    memoTemplateCreate: (...args: unknown[]) => mocks.memoTemplateCreate(...args),
    memoTemplateUpdate: (...args: unknown[]) => mocks.memoTemplateUpdate(...args),
    memoTemplateDelete: (...args: unknown[]) => mocks.memoTemplateDelete(...args),
    memoTemplatesRestoreDefaults: (...args: unknown[]) => mocks.memoTemplatesRestoreDefaults(...args),
  },
}));

function template(overrides: Partial<{
  id: string;
  name: string;
  prompt: string;
  isDefault: boolean;
  locale: string | null;
  defaultKey: string | null;
}> = {}) {
  return {
    id: 'id-1',
    name: 'Biên bản họp',
    prompt: 'Tóm tắt {transcript}',
    isDefault: true,
    locale: 'vi',
    defaultKey: 'meeting-minutes',
    ...overrides,
  };
}

describe('memoTemplatesStore', () => {
  beforeEach(() => {
    vi.resetModules();
    mocks.memoTemplatesList.mockReset();
    mocks.memoTemplateCreate.mockReset();
    mocks.memoTemplateUpdate.mockReset();
    mocks.memoTemplateDelete.mockReset();
    mocks.memoTemplatesRestoreDefaults.mockReset();
  });

  it('loads the list for a locale and exposes it as ready', async () => {
    const rows = [template(), template({ id: 'id-2', defaultKey: 'bilingual-ja-vi' })];
    mocks.memoTemplatesList.mockResolvedValue({ status: 'ok', data: rows });
    const { createMemoTemplatesStore } = await import('./memoTemplates.svelte');
    const store = createMemoTemplatesStore();

    await store.load('vi');

    expect(mocks.memoTemplatesList).toHaveBeenCalledWith('vi');
    expect(store.status).toBe('ready');
    expect(store.templates).toEqual(rows);
    expect(store.currentLocale).toBe('vi');
  });

  it('surfaces a load error without throwing', async () => {
    mocks.memoTemplatesList.mockResolvedValue({
      status: 'error',
      error: { category: 'model', code: 'request', detailRedacted: 'bad locale' },
    });
    const { createMemoTemplatesStore } = await import('./memoTemplates.svelte');
    const store = createMemoTemplatesStore();

    await store.load('fr');

    expect(store.status).toBe('error');
    expect(store.error).toEqual({ category: 'model', code: 'request', detailRedacted: 'bad locale' });
  });

  /** Đổi locale liên tiếp: kết quả của lần `load` cũ bay về sau không được
   * ghi đè lần `load` mới hơn (cùng khuôn `keysStore.mutationGeneration`). */
  it('ignores a stale load that resolves after a newer one', async () => {
    let resolveFirst!: (value: unknown) => void;
    mocks.memoTemplatesList.mockImplementationOnce(
      () => new Promise((resolve) => { resolveFirst = resolve; }),
    );
    mocks.memoTemplatesList.mockResolvedValueOnce({ status: 'ok', data: [template({ locale: 'ja' })] });
    const { createMemoTemplatesStore } = await import('./memoTemplates.svelte');
    const store = createMemoTemplatesStore();

    const first = store.load('vi');
    await store.load('ja');
    resolveFirst({ status: 'ok', data: [template({ id: 'stale' })] });
    await first;

    expect(store.currentLocale).toBe('ja');
    expect(store.templates.some((t) => t.id === 'stale')).toBe(false);
  });

  it('create appends the new template to the end of the list', async () => {
    mocks.memoTemplatesList.mockResolvedValue({ status: 'ok', data: [template()] });
    const created = template({ id: 'new-1', isDefault: false, locale: null, defaultKey: null, name: 'Của tôi' });
    mocks.memoTemplateCreate.mockResolvedValue({ status: 'ok', data: created });
    const { createMemoTemplatesStore } = await import('./memoTemplates.svelte');
    const store = createMemoTemplatesStore();
    await store.load('vi');

    const outcome = await store.create('Của tôi', '{transcript}');

    expect(outcome).toEqual({ status: 'ok', template: created });
    expect(store.templates.at(-1)).toEqual(created);
    expect(store.templates).toHaveLength(2);
  });

  it('create surfaces a validation error and does not touch the list', async () => {
    const rows = [template()];
    mocks.memoTemplatesList.mockResolvedValue({ status: 'ok', data: rows });
    mocks.memoTemplateCreate.mockResolvedValue({
      status: 'error',
      error: { category: 'model', code: 'request', detailRedacted: 'missing {transcript}' },
    });
    const { createMemoTemplatesStore } = await import('./memoTemplates.svelte');
    const store = createMemoTemplatesStore();
    await store.load('vi');

    const outcome = await store.create('x', 'Tóm tắt {notes}');

    expect(outcome.status).toBe('error');
    expect(store.templates).toEqual(rows);
  });

  it('update replaces the template in place without changing its position', async () => {
    const rows = [template(), template({ id: 'id-2', defaultKey: 'bilingual-ja-vi' })];
    mocks.memoTemplatesList.mockResolvedValue({ status: 'ok', data: rows });
    const updated = { ...rows[0], name: 'Tên mới' };
    mocks.memoTemplateUpdate.mockResolvedValue({ status: 'ok', data: updated });
    const { createMemoTemplatesStore } = await import('./memoTemplates.svelte');
    const store = createMemoTemplatesStore();
    await store.load('vi');

    const outcome = await store.update('id-1', 'Tên mới', rows[0].prompt);

    expect(outcome).toEqual({ status: 'ok', template: updated });
    expect(store.templates[0]).toEqual(updated);
    expect(store.templates[1].id).toBe('id-2');
  });

  it('remove deletes a user template from the list', async () => {
    const rows = [template(), template({ id: 'user-1', isDefault: false, locale: null, defaultKey: null })];
    mocks.memoTemplatesList.mockResolvedValue({ status: 'ok', data: rows });
    mocks.memoTemplateDelete.mockResolvedValue({ status: 'ok', data: null });
    const { createMemoTemplatesStore } = await import('./memoTemplates.svelte');
    const store = createMemoTemplatesStore();
    await store.load('vi');

    const outcome = await store.remove('user-1');

    expect(outcome).toEqual({ status: 'ok' });
    expect(store.templates.map((t) => t.id)).toEqual(['id-1']);
  });

  it('remove surfaces a rejection (e.g. default template) without changing the list', async () => {
    const rows = [template()];
    mocks.memoTemplatesList.mockResolvedValue({ status: 'ok', data: rows });
    mocks.memoTemplateDelete.mockResolvedValue({
      status: 'error',
      error: { category: 'model', code: 'request', detailRedacted: 'cannot delete default' },
    });
    const { createMemoTemplatesStore } = await import('./memoTemplates.svelte');
    const store = createMemoTemplatesStore();
    await store.load('vi');

    const outcome = await store.remove('id-1');

    expect(outcome.status).toBe('error');
    expect(store.templates).toEqual(rows);
  });

  it('restoreDefaults replaces the list for the currently loaded locale', async () => {
    const rows = [template({ name: 'Đã sửa' })];
    mocks.memoTemplatesList.mockResolvedValue({ status: 'ok', data: rows });
    const restored = [template({ name: 'Biên bản họp' }), template({ id: 'id-2', defaultKey: 'bilingual-ja-vi' })];
    mocks.memoTemplatesRestoreDefaults.mockResolvedValue({ status: 'ok', data: restored });
    const { createMemoTemplatesStore } = await import('./memoTemplates.svelte');
    const store = createMemoTemplatesStore();
    await store.load('vi');

    const outcome = await store.restoreDefaults('vi');

    expect(outcome).toEqual({ status: 'ok', templates: restored });
    expect(store.templates).toEqual(restored);
  });

  it('restoreDefaults for a locale other than the loaded one does not overwrite the current list', async () => {
    const rows = [template()];
    mocks.memoTemplatesList.mockResolvedValue({ status: 'ok', data: rows });
    mocks.memoTemplatesRestoreDefaults.mockResolvedValue({
      status: 'ok',
      data: [template({ locale: 'ja', name: '議事録' })],
    });
    const { createMemoTemplatesStore } = await import('./memoTemplates.svelte');
    const store = createMemoTemplatesStore();
    await store.load('vi');

    await store.restoreDefaults('ja');

    expect(store.templates).toEqual(rows);
  });

  it('reset clears state back to idle', async () => {
    mocks.memoTemplatesList.mockResolvedValue({ status: 'ok', data: [template()] });
    const { createMemoTemplatesStore } = await import('./memoTemplates.svelte');
    const store = createMemoTemplatesStore();
    await store.load('vi');

    store.reset();

    expect(store.templates).toEqual([]);
    expect(store.status).toBe('idle');
    expect(store.currentLocale).toBeNull();
  });
});
