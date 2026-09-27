// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  memoGet: vi.fn(),
  memoGenerate: vi.fn(),
  memoCancel: vi.fn(),
}));

vi.mock('../bindings', () => ({
  commands: {
    memoGet: (...args: unknown[]) => mocks.memoGet(...args),
    memoGenerate: (...args: unknown[]) => mocks.memoGenerate(...args),
    memoCancel: (...args: unknown[]) => mocks.memoCancel(...args),
  },
}));

const SESSION = 'session-a';
const TEMPLATE = 'template-a';

function memoView(body = '# Memo') {
  return {
    body,
    createdAt: 1_000,
    transcriptStatus: 'complete',
    model: 'gemini-flash-lite-latest',
    templateName: 'Biên bản họp',
    usesNotes: false,
    fromPreviousTranscript: false,
    notesChanged: false,
  };
}

function generated(body = '# Memo') {
  return { status: 'ok', data: { kind: 'generated', memo: memoView(body) } };
}

function cancelled() {
  return { status: 'ok', data: { kind: 'cancelled' } };
}

function alreadyRunning() {
  return { status: 'ok', data: { kind: 'alreadyRunning' } };
}

const quotaError = { category: 'quota', code: 'quota', detailRedacted: 'x' };

beforeEach(() => {
  vi.resetModules();
  mocks.memoGet.mockReset().mockResolvedValue({ status: 'ok', data: null });
  mocks.memoGenerate.mockReset();
  mocks.memoCancel.mockReset().mockResolvedValue({ status: 'ok', data: null });
});

afterEach(() => {
  vi.useRealTimers();
});

describe('memoStore', () => {
  it('load(): chưa từng sinh -> status idle, memo null', async () => {
    const { memoStore } = await import('./memo.svelte');
    await memoStore.load(SESSION, TEMPLATE);
    const view = memoStore.view(SESSION, TEMPLATE);
    expect(view.status).toBe('idle');
    expect(view.memo).toBeNull();
  });

  it('load(): memo đã cache -> status ready, không gọi memoGenerate', async () => {
    mocks.memoGet.mockResolvedValue({ status: 'ok', data: memoView('nội dung cũ') });
    const { memoStore } = await import('./memo.svelte');
    await memoStore.load(SESSION, TEMPLATE);
    const view = memoStore.view(SESSION, TEMPLATE);
    expect(view.status).toBe('ready');
    expect(view.memo?.body).toBe('nội dung cũ');
    expect(mocks.memoGenerate).not.toHaveBeenCalled();
  });

  it('generate(): thành công -> status ready với memo mới', async () => {
    mocks.memoGenerate.mockResolvedValue(generated('# Bản mới'));
    const { memoStore } = await import('./memo.svelte');
    await memoStore.generate(SESSION, TEMPLATE, 'vi');
    const view = memoStore.view(SESSION, TEMPLATE);
    expect(view.status).toBe('ready');
    expect(view.memo?.body).toBe('# Bản mới');
    expect(view.error).toBeNull();
  });

  it('generate(): lỗi Gemini giữ memo cũ và ghi lại lỗi', async () => {
    mocks.memoGet.mockResolvedValue({ status: 'ok', data: memoView('cũ') });
    const { memoStore } = await import('./memo.svelte');
    await memoStore.load(SESSION, TEMPLATE);

    mocks.memoGenerate.mockResolvedValue({ status: 'error', error: quotaError });
    await memoStore.generate(SESSION, TEMPLATE, 'vi');

    const view = memoStore.view(SESSION, TEMPLATE);
    expect(view.status).toBe('ready');
    expect(view.memo?.body).toBe('cũ');
    expect(view.error).toEqual(quotaError);
  });

  it('generate(): Cancelled giữ nguyên memo cũ, không lỗi', async () => {
    mocks.memoGet.mockResolvedValue({ status: 'ok', data: memoView('cũ') });
    const { memoStore } = await import('./memo.svelte');
    await memoStore.load(SESSION, TEMPLATE);

    mocks.memoGenerate.mockResolvedValue(cancelled());
    await memoStore.generate(SESSION, TEMPLATE, 'vi');

    const view = memoStore.view(SESSION, TEMPLATE);
    expect(view.status).toBe('ready');
    expect(view.memo?.body).toBe('cũ');
    expect(view.error).toBeNull();
  });

  it('generate(): AlreadyRunning không đổi memo hiện có', async () => {
    const { memoStore } = await import('./memo.svelte');
    mocks.memoGenerate.mockResolvedValue(alreadyRunning());
    await memoStore.generate(SESSION, TEMPLATE, 'vi');
    const view = memoStore.view(SESSION, TEMPLATE);
    expect(view.status).toBe('idle');
    expect(view.memo).toBeNull();
  });

  it('cancel() gọi commands.memoCancel với đúng cặp id', async () => {
    const { memoStore } = await import('./memo.svelte');
    await memoStore.cancel(SESSION, TEMPLATE);
    expect(mocks.memoCancel).toHaveBeenCalledWith(SESSION, TEMPLATE);
  });

  it('setVisible(): toast chỉ hiện khi panel không còn hiển thị lúc generate xong', async () => {
    const { memoStore } = await import('./memo.svelte');
    const { toastStore } = await import('./toast.svelte');
    toastStore.reset();

    // Đang thấy panel -- không toast.
    memoStore.setVisible(SESSION, TEMPLATE);
    mocks.memoGenerate.mockResolvedValue(generated());
    await memoStore.generate(SESSION, TEMPLATE, 'vi');
    expect(toastStore.items).toHaveLength(0);

    // Rời panel trước khi generate lần hai xong -- toast xuất hiện.
    memoStore.setVisible(null, null);
    mocks.memoGenerate.mockResolvedValue(generated('# Lại nữa'));
    await memoStore.generate(SESSION, TEMPLATE, 'vi');
    expect(toastStore.items).toHaveLength(1);
    toastStore.reset();
  });
});
