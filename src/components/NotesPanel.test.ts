// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import { formatLocalTime } from '../lib/time';
import NotesPanel from './NotesPanel.svelte';
import type { NoteViewState } from '../lib/stores/notes.svelte';

const mocks = vi.hoisted(() => ({
  notesStore: {
    view: vi.fn(),
    load: vi.fn(),
    setBody: vi.fn(),
    retry: vi.fn(),
    flush: vi.fn(),
  },
}));

vi.mock('../lib/stores/notes.svelte', () => ({
  notesStore: mocks.notesStore,
  MAX_NOTE_BODY_LENGTH: 100_000,
}));

const SESSION_ID = 'session-1';

function view(overrides: Partial<NoteViewState> = {}): NoteViewState {
  return { body: '', status: 'idle', savedAtMs: null, loadError: false, ...overrides };
}

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  mocks.notesStore.view.mockReset().mockReturnValue(view());
  mocks.notesStore.load.mockReset().mockResolvedValue(undefined);
  mocks.notesStore.setBody.mockReset();
  mocks.notesStore.retry.mockReset().mockResolvedValue(true);
  mocks.notesStore.flush.mockReset().mockResolvedValue(true);
});

describe('NotesPanel', () => {
  it('loads on mount and renders the current body in the textarea', async () => {
    mocks.notesStore.view.mockReturnValue(view({ body: 'ghi chú có sẵn' }));
    render(NotesPanel, { sessionId: SESSION_ID });
    await Promise.resolve();

    expect(mocks.notesStore.load).toHaveBeenCalledWith(SESSION_ID);
    const textarea = screen.getByPlaceholderText('Ghi chú riêng cho phiên này…') as HTMLTextAreaElement;
    expect(textarea.value).toBe('ghi chú có sẵn');
  });

  it('typing forwards the new value to notesStore.setBody', async () => {
    render(NotesPanel, { sessionId: SESSION_ID });
    await Promise.resolve();

    const textarea = screen.getByPlaceholderText('Ghi chú riêng cho phiên này…');
    await fireEvent.input(textarea, { target: { value: 'gõ vào' } });

    expect(mocks.notesStore.setBody).toHaveBeenCalledWith(SESSION_ID, 'gõ vào');
  });

  it('status "saving" shows "Đang lưu…"', async () => {
    mocks.notesStore.view.mockReturnValue(view({ status: 'saving' }));
    render(NotesPanel, { sessionId: SESSION_ID });
    await Promise.resolve();

    expect(screen.getByRole('status').textContent).toContain('Đang lưu…');
  });

  it('status "saved" shows "Đã lưu hh:mm" from savedAtMs, no retry button', async () => {
    const ms = Date.UTC(2026, 0, 15, 3, 4);
    mocks.notesStore.view.mockReturnValue(view({ status: 'saved', savedAtMs: ms }));
    render(NotesPanel, { sessionId: SESSION_ID });
    await Promise.resolve();

    const expected = `Đã lưu ${formatLocalTime(ms, 'vi')}`;
    expect(screen.getByRole('status').textContent).toContain(expected);
    expect(screen.queryByRole('button', { name: 'Thử lại' })).toBeNull();
  });

  it('status "error" shows "Chưa lưu" + Thử lại, which calls notesStore.retry', async () => {
    mocks.notesStore.view.mockReturnValue(view({ status: 'error', body: 'giữ nguyên' }));
    render(NotesPanel, { sessionId: SESSION_ID });
    await Promise.resolve();

    expect(screen.getByRole('status').textContent).toContain('Chưa lưu');
    await fireEvent.click(screen.getByRole('button', { name: 'Thử lại' }));

    expect(mocks.notesStore.retry).toHaveBeenCalledWith(SESSION_ID);
  });

  it('loadError shows the locked message + Tải lại instead of the textarea', async () => {
    mocks.notesStore.view.mockReturnValue(view({ loadError: true }));
    render(NotesPanel, { sessionId: SESSION_ID });
    await Promise.resolve();

    expect(screen.getByRole('alert').textContent).toContain('Không tải được ghi chú.');
    expect(screen.queryByPlaceholderText('Ghi chú riêng cho phiên này…')).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Tải lại' }));
    expect(mocks.notesStore.load).toHaveBeenCalledTimes(2); // mount + retry click
  });

  it('exported flush() delegates to notesStore.flush(sessionId)', async () => {
    const { component } = render(NotesPanel, { sessionId: SESSION_ID }) as unknown as {
      component: { flush: () => Promise<boolean> };
    };
    await Promise.resolve();

    const ok = await component.flush();

    expect(mocks.notesStore.flush).toHaveBeenCalledWith(SESSION_ID);
    expect(ok).toBe(true);
  });

  it('unmount flushes the session it was showing', async () => {
    const { unmount } = render(NotesPanel, { sessionId: SESSION_ID });
    await Promise.resolve();

    unmount();

    expect(mocks.notesStore.flush).toHaveBeenCalledWith(SESSION_ID);
  });
});
