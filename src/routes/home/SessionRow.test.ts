// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import { configureRouter } from '../../lib/router';
import SessionRow from './SessionRow.svelte';
import type { SessionListItem } from '../../lib/bindings';

const mocks = vi.hoisted(() => ({
  rename: vi.fn(),
  remove: vi.fn(),
  attachTag: vi.fn(),
  detachTag: vi.fn(),
  createTag: vi.fn(),
  deleteTagGlobally: vi.fn(),
  tags: [] as Array<{ id: string; name: string; sessionCount: number }>,
}));

vi.mock('../../lib/stores/library.svelte', () => ({
  libraryStore: {
    rename: (...args: unknown[]) => mocks.rename(...args),
    remove: (...args: unknown[]) => mocks.remove(...args),
    attachTag: (...args: unknown[]) => mocks.attachTag(...args),
    detachTag: (...args: unknown[]) => mocks.detachTag(...args),
    createTag: (...args: unknown[]) => mocks.createTag(...args),
    deleteTagGlobally: (...args: unknown[]) => mocks.deleteTagGlobally(...args),
    get tags() {
      return mocks.tags;
    },
  },
}));

function item(overrides: Partial<SessionListItem> = {}): SessionListItem {
  return {
    sessionId: 'abc',
    kind: 'file',
    status: 'complete',
    recordingAvailable: false,
    title: 'cuộc họp',
    createdAt: Date.UTC(2026, 0, 15),
    durationSec: 65,
    recovered: false,
    missingGapCount: 0,
    tagIds: [],
    ...overrides,
  };
}

afterEach(() => cleanup());

beforeEach(() => {
  configureRouter();
  i18n.applyPreference('vi');
  mocks.rename.mockReset().mockResolvedValue({ status: 'ok', title: 'tên mới' });
  mocks.remove.mockReset().mockResolvedValue({ status: 'ok', outcome: 'deleted' });
  mocks.attachTag.mockReset().mockResolvedValue({ status: 'ok' });
  mocks.detachTag.mockReset().mockResolvedValue({ status: 'ok' });
  mocks.createTag.mockReset();
  mocks.deleteTagGlobally.mockReset();
  mocks.tags = [];
});

describe('SessionRow', () => {
  it('is a link to /session/:id with the title', () => {
    render(SessionRow, { session: item({ sessionId: 'xyz', title: 'buổi họp nhóm' }) });
    const row = screen.getByRole('link', { name: /buổi họp nhóm/ });
    expect(row.getAttribute('href')).toBe('/session/xyz');
  });

  it('never shows the "Thiếu N khoảng" badge when missingGapCount is 0', () => {
    render(SessionRow, { session: item({ missingGapCount: 0 }) });
    expect(screen.queryByText(/Thiếu/)).toBeNull();
  });

  it('shows "Thiếu N khoảng" right after the title when missingGapCount > 0', () => {
    render(SessionRow, { session: item({ missingGapCount: 3 }) });
    expect(screen.getByText('Thiếu 3 khoảng')).toBeTruthy();
  });

  it('shows "Phục hồi" when recovered', () => {
    render(SessionRow, { session: item({ recovered: true }) });
    expect(screen.getByText('Phục hồi')).toBeTruthy();
  });

  it('does not show "Phục hồi" when not recovered', () => {
    render(SessionRow, { session: item({ recovered: false }) });
    expect(screen.queryByText('Phục hồi')).toBeNull();
  });

  it('shows a file badge for kind=file and a live badge for kind=live', () => {
    const { unmount } = render(SessionRow, { session: item({ kind: 'file' }) });
    expect(screen.getByText('Tệp')).toBeTruthy();
    unmount();

    render(SessionRow, { session: item({ kind: 'live' }) });
    expect(screen.getByText('Live')).toBeTruthy();
  });

  it('formats duration as MM:SS under an hour and HH:MM:SS at/over an hour', () => {
    const { unmount } = render(SessionRow, { session: item({ durationSec: 65 }) });
    expect(screen.getByText('01:05')).toBeTruthy();
    unmount();

    render(SessionRow, { session: item({ durationSec: 3665 }) });
    expect(screen.getByText('01:01:05')).toBeTruthy();
  });

  it('never shows model, segment count, or a status column', () => {
    render(SessionRow, { session: item() });
    expect(screen.queryByText(/gemini/i)).toBeNull();
    expect(screen.queryByText(/segment/i)).toBeNull();
    expect(screen.queryByText(/status|trạng thái/i)).toBeNull();
  });

  describe('menu, rename, and delete (story 3.1)', () => {
    it('the ⋯ menu is reachable and exposes Đổi tên and Xoá', async () => {
      render(SessionRow, { session: item() });

      const trigger = screen.getByRole('button', { name: 'Thao tác khác' });
      await fireEvent.click(trigger);

      expect(screen.getByRole('menuitem', { name: 'Đổi tên' })).toBeTruthy();
      expect(screen.getByRole('menuitem', { name: 'Xoá' })).toBeTruthy();
    });

    it('offers Download Recording only for persisted Live recordings and explains active sessions', async () => {
      const { unmount } = render(SessionRow, {
        session: item({ kind: 'live', status: 'complete', recordingAvailable: true }),
      });
      await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
      expect(screen.getByRole('menuitem', { name: 'Tải Recording' })).toBeTruthy();

      unmount();
      render(SessionRow, {
        session: item({ kind: 'live', status: 'recording', recordingAvailable: true }),
      });
      await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
      const itemButton = screen.getByRole('menuitem', { name: /Tải Recording/ }) as HTMLButtonElement;
      expect(itemButton.disabled).toBe(true);
      expect(itemButton.textContent).toContain('Chỉ tải sau khi phiên đã lưu xong.');
    });

    it('does not offer Download Recording for file sessions', async () => {
      render(SessionRow, { session: item({ kind: 'file', status: 'complete', recordingAvailable: false }) });
      await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
      expect(screen.queryByText('Tải Recording')).toBeNull();
    });

    it('choosing Đổi tên replaces the link with an inline input preselected to the current title', async () => {
      render(SessionRow, { session: item({ title: 'cuộc họp cũ' }) });

      await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
      await fireEvent.click(screen.getByRole('menuitem', { name: 'Đổi tên' }));

      expect(screen.queryByRole('link', { name: /cuộc họp cũ/ })).toBeNull();
      const input = screen.getByRole('textbox', { name: 'Tên phiên' }) as HTMLInputElement;
      expect(input.value).toBe('cuộc họp cũ');
    });

    it('Escape while renaming cancels without calling rename and restores the link', async () => {
      render(SessionRow, { session: item({ title: 'cuộc họp cũ' }) });

      await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
      await fireEvent.click(screen.getByRole('menuitem', { name: 'Đổi tên' }));
      const input = screen.getByRole('textbox', { name: 'Tên phiên' });
      await fireEvent.keyDown(input, { key: 'Escape' });

      expect(mocks.rename).not.toHaveBeenCalled();
      expect(screen.getByRole('link', { name: /cuộc họp cũ/ })).toBeTruthy();
    });

    it('an empty name is rejected inline without calling rename', async () => {
      render(SessionRow, { session: item({ title: 'cuộc họp' }) });

      await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
      await fireEvent.click(screen.getByRole('menuitem', { name: 'Đổi tên' }));
      const input = screen.getByRole('textbox', { name: 'Tên phiên' }) as HTMLInputElement;
      await fireEvent.input(input, { target: { value: '   ' } });
      await fireEvent.keyDown(input, { key: 'Enter' });

      expect(mocks.rename).not.toHaveBeenCalled();
      expect(screen.getByText('Tên không được để trống')).toBeTruthy();
    });

    it('Enter with a valid trimmed name calls libraryStore.rename and exits edit mode on success', async () => {
      render(SessionRow, { session: item({ sessionId: 'abc', title: 'cuộc họp' }) });

      await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
      await fireEvent.click(screen.getByRole('menuitem', { name: 'Đổi tên' }));
      const input = screen.getByRole('textbox', { name: 'Tên phiên' }) as HTMLInputElement;
      await fireEvent.input(input, { target: { value: '  Họp sprint 12  ' } });
      await fireEvent.keyDown(input, { key: 'Enter' });
      await Promise.resolve();
      await Promise.resolve();

      expect(mocks.rename).toHaveBeenCalledWith('abc', 'Họp sprint 12');
      expect(screen.queryByRole('textbox', { name: 'Tên phiên' })).toBeNull();
    });

    it('choosing Xoá opens the confirm dialog', async () => {
      render(SessionRow, { session: item() });

      await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
      await fireEvent.click(screen.getByRole('menuitem', { name: 'Xoá' }));

      expect(screen.getByRole('alertdialog')).toBeTruthy();
      expect(screen.getByText('Xoá phiên này?')).toBeTruthy();
    });

    it('confirming delete calls libraryStore.remove', async () => {
      render(SessionRow, { session: item({ sessionId: 'abc' }) });

      await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
      await fireEvent.click(screen.getByRole('menuitem', { name: 'Xoá' }));
      await fireEvent.click(screen.getByRole('button', { name: 'Xoá phiên' }));

      expect(mocks.remove).toHaveBeenCalledWith('abc');
    });

    it('cancelling the delete dialog calls remove nothing and closes it', async () => {
      render(SessionRow, { session: item() });

      await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
      await fireEvent.click(screen.getByRole('menuitem', { name: 'Xoá' }));
      await fireEvent.click(screen.getByRole('button', { name: 'Huỷ' }));

      expect(mocks.remove).not.toHaveBeenCalled();
      expect(screen.queryByRole('alertdialog')).toBeNull();
    });

    it('a Busy outcome shows an inline explanation instead of deleting anything', async () => {
      mocks.remove.mockResolvedValueOnce({ status: 'ok', outcome: 'busy' });
      render(SessionRow, { session: item() });

      await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
      await fireEvent.click(screen.getByRole('menuitem', { name: 'Xoá' }));
      await fireEvent.click(screen.getByRole('button', { name: 'Xoá phiên' }));
      await Promise.resolve();
      await Promise.resolve();

      expect(screen.getByText('Phiên đang có tác vụ chạy, thử lại khi xong.')).toBeTruthy();
      expect(screen.queryByRole('alertdialog')).toBeNull();
    });
  });

  describe('tags (story 3.2)', () => {
    it('shows chips for tags already on this session, resolved by name from libraryStore.tags', () => {
      mocks.tags = [{ id: 't1', name: 'khách A', sessionCount: 1 }];
      render(SessionRow, { session: item({ tagIds: ['t1'] }) });
      expect(screen.getByText('khách A')).toBeTruthy();
    });

    it('shows no tag chips when the session has none', () => {
      render(SessionRow, { session: item({ tagIds: [] }) });
      expect(screen.queryByText(/^\+\d/)).toBeNull();
    });

    it('the ⋯ menu exposes Gắn tag, which opens the tag picker', async () => {
      mocks.tags = [{ id: 't1', name: 'khách A', sessionCount: 1 }];
      render(SessionRow, { session: item() });

      await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
      expect(screen.getByRole('menuitem', { name: 'Gắn tag' })).toBeTruthy();
      await fireEvent.click(screen.getByRole('menuitem', { name: 'Gắn tag' }));

      expect(screen.getByRole('dialog')).toBeTruthy();
      expect(screen.getByText('khách A')).toBeTruthy();
    });

    it('checking a tag in the picker calls libraryStore.attachTag with the session id', async () => {
      mocks.tags = [{ id: 't1', name: 'khách A', sessionCount: 0 }];
      render(SessionRow, { session: item({ sessionId: 'abc', tagIds: [] }) });

      await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
      await fireEvent.click(screen.getByRole('menuitem', { name: 'Gắn tag' }));
      await fireEvent.click(screen.getByRole('checkbox', { name: /khách A/ }));

      expect(mocks.attachTag).toHaveBeenCalledWith('abc', 't1');
    });
  });
});
