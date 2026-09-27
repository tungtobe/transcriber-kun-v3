// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  notesGet: vi.fn(),
  notesSave: vi.fn(),
}));

vi.mock('../bindings', () => ({
  commands: {
    notesGet: (...args: unknown[]) => mocks.notesGet(...args),
    notesSave: (...args: unknown[]) => mocks.notesSave(...args),
  },
}));

const SESSION_A = 'session-a';
const SESSION_B = 'session-b';

function saved(revision: number, updatedAt: number) {
  return { status: 'ok', data: { kind: 'saved', revision, updatedAt } };
}

function stale(revision: number) {
  return { status: 'ok', data: { kind: 'stale', revision } };
}

function notFound() {
  return { status: 'ok', data: { kind: 'notFound' } };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

/** Xả hết microtask đang chờ -- an toàn hơn đếm số lần `await Promise.resolve()`
 * cần thiết cho một chuỗi `await` lồng nhau cụ thể. */
async function flushMicrotasks(): Promise<void> {
  for (let i = 0; i < 10; i += 1) {
    await Promise.resolve();
  }
}

afterEach(() => {
  vi.useRealTimers();
});

beforeEach(() => {
  vi.resetModules();
  mocks.notesGet.mockReset().mockResolvedValue({ status: 'ok', data: null });
  mocks.notesSave.mockReset();
});

describe('notesStore', () => {
  it('load(): không có ghi chú -> view rỗng, status idle', async () => {
    const { notesStore } = await import('./notes.svelte');
    await notesStore.load(SESSION_A);

    const view = notesStore.view(SESSION_A);
    expect(view.body).toBe('');
    expect(view.status).toBe('idle');
    expect(view.loadError).toBe(false);
  });

  it('load(): có ghi chú -> view hiện đúng body/revision/updatedAt (bền qua đóng đột ngột)', async () => {
    mocks.notesGet.mockResolvedValue({
      status: 'ok',
      data: { body: 'bản rev 3', revision: 3, updatedAt: 3_000 },
    });
    const { notesStore } = await import('./notes.svelte');
    await notesStore.load(SESSION_A);

    const view = notesStore.view(SESSION_A);
    expect(view.body).toBe('bản rev 3');
    expect(view.status).toBe('saved');
    expect(view.savedAtMs).toBe(3_000);
  });

  it('load(): notesGet lỗi -> loadError true, textarea coi như khoá', async () => {
    mocks.notesGet.mockResolvedValue({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'x' },
    });
    const { notesStore } = await import('./notes.svelte');
    await notesStore.load(SESSION_A);

    expect(notesStore.view(SESSION_A).loadError).toBe(true);
  });

  // Spec I/O Matrix "Gõ rồi dừng".
  it('gõ rồi dừng: đúng 1 lần notesSave rev 1 sau 800ms; "saved" sau ACK', async () => {
    vi.useFakeTimers();
    mocks.notesSave.mockResolvedValueOnce(saved(1, 1_000));
    const { notesStore } = await import('./notes.svelte');
    await notesStore.load(SESSION_A);

    notesStore.setBody(SESSION_A, 'abc');
    expect(mocks.notesSave).not.toHaveBeenCalled();
    expect(notesStore.view(SESSION_A).status).toBe('dirty');

    await vi.advanceTimersByTimeAsync(800);

    expect(mocks.notesSave).toHaveBeenCalledTimes(1);
    expect(mocks.notesSave).toHaveBeenCalledWith(SESSION_A, 'abc', 1);
    expect(notesStore.view(SESSION_A).status).toBe('saved');
    expect(notesStore.view(SESSION_A).savedAtMs).toBe(1_000);
  });

  // Spec I/O Matrix "Gõ liên tục".
  it('gõ liên tục: không lưu trong lúc gõ, lưu đúng 1 lần sau khi dừng', async () => {
    vi.useFakeTimers();
    mocks.notesSave.mockResolvedValueOnce(saved(1, 1_000));
    const { notesStore } = await import('./notes.svelte');
    await notesStore.load(SESSION_A);

    notesStore.setBody(SESSION_A, 'a');
    await vi.advanceTimersByTimeAsync(300);
    notesStore.setBody(SESSION_A, 'ab');
    await vi.advanceTimersByTimeAsync(300);
    notesStore.setBody(SESSION_A, 'abc');
    await vi.advanceTimersByTimeAsync(300);
    expect(mocks.notesSave).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(500);

    expect(mocks.notesSave).toHaveBeenCalledTimes(1);
    expect(mocks.notesSave).toHaveBeenCalledWith(SESSION_A, 'abc', 1);
  });

  // Spec I/O Matrix "Lưu lỗi".
  it('lưu lỗi: "error" + buffer giữ nguyên; retry() lưu lại thành công', async () => {
    vi.useFakeTimers();
    mocks.notesSave.mockResolvedValueOnce({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'boom' },
    });
    const { notesStore } = await import('./notes.svelte');
    await notesStore.load(SESSION_A);

    notesStore.setBody(SESSION_A, 'giữ nguyên');
    await vi.advanceTimersByTimeAsync(800);

    expect(notesStore.view(SESSION_A).status).toBe('error');
    expect(notesStore.view(SESSION_A).body).toBe('giữ nguyên');

    // Thử lại gửi revision 2 (revision 1 đã "đã gửi" dù thất bại -- spec
    // Boundaries Always: "mỗi lần lưu gửi revision = revision cao nhất đã
    // gửi + 1", không phân biệt lần gửi trước thành công hay không).
    mocks.notesSave.mockResolvedValueOnce(saved(2, 5_000));
    const ok = await notesStore.retry(SESSION_A);

    expect(ok).toBe(true);
    expect(mocks.notesSave).toHaveBeenCalledTimes(2);
    expect(mocks.notesSave).toHaveBeenNthCalledWith(2, SESSION_A, 'giữ nguyên', 2);
    expect(notesStore.view(SESSION_A).status).toBe('saved');
  });

  it('load() lại sau khi lưu lỗi không nạp đè buffer chưa lưu', async () => {
    vi.useFakeTimers();
    mocks.notesSave.mockResolvedValueOnce({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'boom' },
    });
    const { notesStore } = await import('./notes.svelte');
    await notesStore.load(SESSION_A);
    notesStore.setBody(SESSION_A, 'chưa lưu');
    await vi.advanceTimersByTimeAsync(800);
    expect(notesStore.view(SESSION_A).status).toBe('error');

    mocks.notesGet.mockResolvedValueOnce({
      status: 'ok',
      data: { body: 'bản cũ', revision: 0, updatedAt: 1 },
    });
    await notesStore.load(SESSION_A);

    expect(notesStore.view(SESSION_A).body).toBe('chưa lưu');
    expect(notesStore.view(SESSION_A).status).toBe('error');
  });

  it('gõ trong lúc load() chưa xong: giữ chữ đã gõ và lưu vượt revision DB', async () => {
    vi.useFakeTimers();
    const pending = deferred<unknown>();
    mocks.notesGet.mockReturnValueOnce(pending.promise);
    mocks.notesSave.mockResolvedValueOnce(saved(4, 9_000));
    const { notesStore } = await import('./notes.svelte');
    const loading = notesStore.load(SESSION_A);

    notesStore.setBody(SESSION_A, 'gõ sớm');
    pending.resolve({ status: 'ok', data: { body: 'trong DB', revision: 3, updatedAt: 1 } });
    await loading;
    expect(notesStore.view(SESSION_A).body).toBe('gõ sớm');

    await vi.advanceTimersByTimeAsync(800);
    expect(mocks.notesSave).toHaveBeenCalledWith(SESSION_A, 'gõ sớm', 4);
  });

  // Spec I/O Matrix "Rời màn trước debounce".
  it('flush() gửi ngay nội dung mới nhất không chờ debounce', async () => {
    mocks.notesSave.mockResolvedValueOnce(saved(1, 1_000));
    const { notesStore } = await import('./notes.svelte');
    await notesStore.load(SESSION_A);

    notesStore.setBody(SESSION_A, 'gửi ngay');
    const ok = await notesStore.flush(SESSION_A);

    expect(ok).toBe(true);
    expect(mocks.notesSave).toHaveBeenCalledTimes(1);
    expect(mocks.notesSave).toHaveBeenCalledWith(SESSION_A, 'gửi ngay', 1);
  });

  it('flush() không gửi gì khi không có thay đổi', async () => {
    const { notesStore } = await import('./notes.svelte');
    await notesStore.load(SESSION_A);

    const ok = await notesStore.flush(SESSION_A);

    expect(ok).toBe(true);
    expect(mocks.notesSave).not.toHaveBeenCalled();
  });

  // Spec I/O Matrix "ACK sai thứ tự": gửi rev 6 rồi 7 (ở đây 1 rồi 2), ACK 2
  // về trước ACK 1 -- trạng thái theo rev 2, không lùi.
  it('ACK sai thứ tự không làm lùi trạng thái/thời gian đã lưu', async () => {
    const first = deferred<unknown>();
    const second = deferred<unknown>();
    mocks.notesSave.mockImplementationOnce(() => first.promise);
    mocks.notesSave.mockImplementationOnce(() => second.promise);
    const { notesStore } = await import('./notes.svelte');
    await notesStore.load(SESSION_A);

    notesStore.setBody(SESSION_A, 'a');
    const flushA = notesStore.flush(SESSION_A); // gửi rev 1 ("a"), đang bay

    notesStore.setBody(SESSION_A, 'ab');
    const flushB = notesStore.flush(SESSION_A); // gửi rev 2 ("ab") song song, không chờ rev 1

    expect(mocks.notesSave).toHaveBeenCalledTimes(2);
    expect(mocks.notesSave).toHaveBeenNthCalledWith(1, SESSION_A, 'a', 1);
    expect(mocks.notesSave).toHaveBeenNthCalledWith(2, SESSION_A, 'ab', 2);

    // ACK rev 2 về trước.
    second.resolve(saved(2, 2_000));
    await flushMicrotasks();

    // ACK rev 1 về sau, revision thấp hơn -- không được lùi trạng thái.
    first.resolve(saved(1, 1_000));
    await Promise.all([flushA, flushB]);

    const view = notesStore.view(SESSION_A);
    expect(view.status).toBe('saved');
    expect(view.savedAtMs).toBe(2_000);
  });

  // Spec I/O Matrix "Stale" (khía cạnh frontend, spec Design Notes): buffer
  // không đổi kể từ lần gửi -> nạp lại từ DB.
  it('Stale với buffer không đổi: nạp lại bản trong DB', async () => {
    mocks.notesSave.mockResolvedValueOnce(stale(9));
    mocks.notesGet.mockResolvedValueOnce({ status: 'ok', data: null }); // load() ban đầu
    const { notesStore } = await import('./notes.svelte');
    await notesStore.load(SESSION_A);

    mocks.notesGet.mockResolvedValueOnce({
      status: 'ok',
      data: { body: 'bản trên DB', revision: 9, updatedAt: 9_000 },
    });
    notesStore.setBody(SESSION_A, 'a');
    await notesStore.flush(SESSION_A);

    const view = notesStore.view(SESSION_A);
    expect(view.body).toBe('bản trên DB');
    expect(view.status).toBe('saved');
    expect(view.savedAtMs).toBe(9_000);
  });

  // Spec Design Notes: buffer đã đổi kể từ lần gửi -> gửi lại với revision =
  // stale + 1.
  it('Stale với buffer đã đổi: gửi lại với revision = stale + 1', async () => {
    const { notesStore } = await import('./notes.svelte');
    await notesStore.load(SESSION_A);

    mocks.notesSave.mockImplementationOnce(async () => {
      // Trong lúc lần gửi đầu (rev 1, body "a") còn "đang bay", buffer đổi
      // thành "ab" trước khi outcome Stale được xử lý.
      notesStore.setBody(SESSION_A, 'ab');
      return stale(5);
    });
    mocks.notesSave.mockResolvedValueOnce(saved(6, 6_000));

    notesStore.setBody(SESSION_A, 'a');
    await notesStore.flush(SESSION_A);

    expect(mocks.notesSave).toHaveBeenNthCalledWith(1, SESSION_A, 'a', 1);
    expect(mocks.notesSave).toHaveBeenNthCalledWith(2, SESSION_A, 'ab', 6);
    expect(notesStore.view(SESSION_A).status).toBe('saved');
  });

  it('NotFound (Phiên bị xoá đồng thời) hiện như lỗi lưu, buffer giữ nguyên', async () => {
    mocks.notesSave.mockResolvedValueOnce(notFound());
    const { notesStore } = await import('./notes.svelte');
    await notesStore.load(SESSION_A);

    notesStore.setBody(SESSION_A, 'còn nguyên');
    await notesStore.flush(SESSION_A);

    const view = notesStore.view(SESSION_A);
    expect(view.status).toBe('error');
    expect(view.body).toBe('còn nguyên');
  });

  it('flushAll(): true khi mọi Phiên lưu ok, false khi có Phiên lỗi', async () => {
    mocks.notesSave.mockImplementation((sessionId: string) =>
      sessionId === SESSION_A ? Promise.resolve(saved(1, 1_000)) : Promise.resolve({
        status: 'error',
        error: { category: 'storage', code: 'storage', detailRedacted: 'x' },
      }),
    );
    const { notesStore } = await import('./notes.svelte');
    await notesStore.load(SESSION_A);
    await notesStore.load(SESSION_B);
    notesStore.setBody(SESSION_A, 'ok');
    notesStore.setBody(SESSION_B, 'lỗi');

    const ok = await notesStore.flushAll();

    expect(ok).toBe(false);
    expect(notesStore.view(SESSION_A).status).toBe('saved');
    expect(notesStore.view(SESSION_B).status).toBe('error');
  });
});
