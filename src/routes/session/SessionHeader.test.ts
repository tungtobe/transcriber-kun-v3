// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import { configureRouter } from '../../lib/router';
import SessionHeader from './SessionHeader.svelte';

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

afterEach(() => cleanup());

beforeEach(() => {
  configureRouter();
  i18n.applyPreference('vi');
  mocks.rename.mockReset();
  mocks.remove.mockReset();
  mocks.attachTag.mockReset().mockResolvedValue({ status: 'ok' });
  mocks.detachTag.mockReset().mockResolvedValue({ status: 'ok' });
  mocks.createTag.mockReset();
  mocks.deleteTagGlobally.mockReset();
  mocks.tags = [];
});

function props(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    sessionId: 'session-1',
    title: 'cuộc họp',
    kind: 'file',
    status: 'complete',
    recordingAvailable: false,
    createdAtMs: Date.UTC(2026, 0, 15),
    durationSec: 125,
    segmentTextCount: 42,
    recovered: false,
    partial: false,
    hasMemo: false,
    tags: [],
    onRenamed: vi.fn(),
    onDeleted: vi.fn(),
    onTagsChanged: vi.fn(),
    ...overrides,
  };
}

describe('SessionHeader', () => {
  it('renders the title, a FILE badge, duration, and the text-segment count', () => {
    render(SessionHeader, props());

    expect(screen.getByRole('heading', { name: 'cuộc họp' })).toBeTruthy();
    expect(screen.getByText('Tệp')).toBeTruthy();
    expect(screen.getByText('02:05')).toBeTruthy();
    expect(screen.getByText('42 đoạn')).toBeTruthy();
    expect(screen.queryByText('Đã phục hồi')).toBeNull();
    expect(screen.queryByText('Chưa đầy đủ')).toBeNull();
  });

  it('shows a LIVE badge instead of FILE for a live session', () => {
    render(
      SessionHeader,
      props({
        title: 'phiên live',
        kind: 'live',
        createdAtMs: null,
        durationSec: null,
        segmentTextCount: 0,
      }),
    );

    expect(screen.getByText('Live')).toBeTruthy();
    expect(screen.queryByText('Tệp')).toBeNull();
  });

  it('offers Download Recording for a saved Live Recording and disables it while active', async () => {
    const { unmount } = render(
      SessionHeader,
      props({ kind: 'live', recordingAvailable: true }),
    );
    await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
    expect(screen.getByRole('menuitem', { name: 'Tải Recording' })).toBeTruthy();

    unmount();
    render(SessionHeader, props({ kind: 'live', status: 'finalizing', recordingAvailable: true }));
    await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
    const download = screen.getByRole('menuitem', { name: /Tải Recording/ }) as HTMLButtonElement;
    expect(download.disabled).toBe(true);
    expect(download.textContent).toContain('Chỉ tải sau khi phiên đã lưu xong.');
  });

  it('shows the recover badge when recovered, and the partial badge when partial', () => {
    render(
      SessionHeader,
      props({ createdAtMs: null, durationSec: null, segmentTextCount: 0, recovered: true, partial: true }),
    );

    expect(screen.getByText('Đã phục hồi')).toBeTruthy();
    expect(screen.getByText('Chưa đầy đủ')).toBeTruthy();
  });

  // Story 3.7 spec Boundaries Always: "Badge `memo` ... khi Phiên có ít
  // nhất một memo".
  it('shows the memo badge only when hasMemo is true', () => {
    render(SessionHeader, props({ hasMemo: false }));
    expect(screen.queryByText('Memo')).toBeNull();

    render(SessionHeader, props({ sessionId: 'session-2', hasMemo: true }));
    expect(screen.getByText('Memo')).toBeTruthy();
  });

  it('links back to Home', () => {
    render(SessionHeader, props({ createdAtMs: null, durationSec: null, segmentTextCount: 0 }));

    const back = screen.getByRole('link', { name: 'Quay lại' });
    expect(back.getAttribute('href')).toContain('/home');
  });

  it('clicking the title opens inline rename with the current title preselected', async () => {
    render(SessionHeader, props());

    await fireEvent.click(screen.getByRole('button', { name: 'cuộc họp' }));

    const input = screen.getByRole('textbox', { name: 'Tên phiên' });
    expect((input as HTMLInputElement).value).toBe('cuộc họp');
  });

  it('the ⋯ menu exposes Đổi tên and Xoá', async () => {
    render(SessionHeader, props());

    await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));

    expect(screen.getByRole('menuitem', { name: 'Đổi tên' })).toBeTruthy();
    expect(screen.getByRole('menuitem', { name: 'Xoá' })).toBeTruthy();
  });

  it('choosing Xoá from the menu opens the delete confirmation dialog', async () => {
    render(SessionHeader, props());

    await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Xoá' }));

    expect(screen.getByRole('alertdialog')).toBeTruthy();
    expect(screen.getByText('Xoá phiên này?')).toBeTruthy();
  });

  describe('tags (story 3.2)', () => {
    it('shows a chip for each tag on this session, and a "+ Tag" button', () => {
      render(SessionHeader, props({ tags: [{ id: 't1', name: 'khách A' }] }));
      expect(screen.getByText('khách A')).toBeTruthy();
      expect(screen.getByRole('button', { name: '+ Tag' })).toBeTruthy();
    });

    it('clicking "+ Tag" opens the tag picker in assign mode', async () => {
      mocks.tags = [{ id: 't1', name: 'khách A', sessionCount: 1 }];
      render(SessionHeader, props());
      await fireEvent.click(screen.getByRole('button', { name: '+ Tag' }));

      expect(screen.getByRole('dialog', { name: 'Chọn tag cho phiên' })).toBeTruthy();
    });

    it('selecting a tag in the picker attaches it and reports the new tags via onTagsChanged', async () => {
      mocks.tags = [{ id: 't1', name: 'khách A', sessionCount: 1 }];
      const onTagsChanged = vi.fn();
      render(SessionHeader, props({ sessionId: 's1', tags: [], onTagsChanged }));
      await fireEvent.click(screen.getByRole('button', { name: '+ Tag' }));
      await fireEvent.click(screen.getByRole('checkbox', { name: /khách A/ }));

      expect(mocks.attachTag).toHaveBeenCalledWith('s1', 't1');
      expect(onTagsChanged).toHaveBeenCalledWith([{ id: 't1', name: 'khách A' }]);
    });

    it('clicking a chip in the row detaches that tag and reports the remaining tags', async () => {
      const onTagsChanged = vi.fn();
      render(SessionHeader, props({ sessionId: 's1', tags: [{ id: 't1', name: 'A' }, { id: 't2', name: 'B' }], onTagsChanged }));

      await fireEvent.click(screen.getByText('A').closest('button')!);

      expect(mocks.detachTag).toHaveBeenCalledWith('s1', 't1');
      expect(onTagsChanged).toHaveBeenCalledWith([{ id: 't2', name: 'B' }]);
    });
  });
});
