// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import SegmentList from './SegmentList.svelte';
import type { SegmentDetailView } from './SegmentList.svelte';

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  // jsdom does not implement `scrollIntoView` — stub it so the component's
  // auto-scroll effect can run without throwing.
  Element.prototype.scrollIntoView = vi.fn();
});

function segments(): SegmentDetailView[] {
  return [
    { idx: 0, startSec: 0, endSec: 10, kind: 'text', gapReason: null, text: 'đoạn một' },
    { idx: 1, startSec: 10, endSec: 20, kind: 'text', gapReason: null, text: 'đoạn hai' },
  ];
}

describe('SegmentList', () => {
  it('highlights only the segment whose [start, end) contains currentTime (acceptance: currentTime=12)', () => {
    render(SegmentList, {
      segments: segments(),
      currentTime: 12,
      playing: false,
      rerunStarting: false,
      onSeek: vi.fn(),
      onTogglePlay: vi.fn(),
      onRerun: vi.fn(),
    });

    const first = screen.getByText('đoạn một').closest('[role="button"]')!;
    const second = screen.getByText('đoạn hai').closest('[role="button"]')!;
    expect(first.getAttribute('aria-current')).toBeNull();
    expect(second.getAttribute('aria-current')).toBe('true');
    expect(second.className).toContain('segment-row-active');
    expect(first.className).not.toContain('segment-row-active');
  });

  it('highlights nothing when currentTime falls exactly on a boundary end (half-open [start, end))', () => {
    render(SegmentList, {
      segments: segments(),
      currentTime: 10,
      playing: false,
      rerunStarting: false,
      onSeek: vi.fn(),
      onTogglePlay: vi.fn(),
      onRerun: vi.fn(),
    });

    const first = screen.getByText('đoạn một').closest('[role="button"]')!;
    const second = screen.getByText('đoạn hai').closest('[role="button"]')!;
    expect(first.getAttribute('aria-current')).toBeNull();
    expect(second.getAttribute('aria-current')).toBe('true');
  });

  it('seeks to the raw start_sec of a text segment on click', async () => {
    const onSeek = vi.fn();
    render(SegmentList, {
      segments: segments(),
      currentTime: 0,
      playing: false,
      rerunStarting: false,
      onSeek,
      onTogglePlay: vi.fn(),
      onRerun: vi.fn(),
    });

    await fireEvent.click(screen.getByText('đoạn hai'));
    expect(onSeek).toHaveBeenCalledWith(10);
  });

  it('marks the active search match and scrolls its segment into view', () => {
    render(SegmentList, {
      segments: segments(),
      currentTime: 0,
      playing: false,
      rerunStarting: false,
      matches: [{ segmentIndex: 1, start: 0, end: 5 }],
      activeMatchIndex: 0,
      onSeek: vi.fn(),
      onTogglePlay: vi.fn(),
      onRerun: vi.fn(),
    });

    const mark = screen.getByText('đoạn');
    expect(mark.tagName).toBe('MARK');
    expect(mark.className).toContain('search-match-current');
    expect(Element.prototype.scrollIntoView).toHaveBeenCalledWith({ block: 'center', behavior: 'smooth' });
  });

  it('does not scroll when there is no active search match', () => {
    render(SegmentList, {
      segments: segments(),
      currentTime: 0,
      playing: false,
      rerunStarting: false,
      matches: [],
      activeMatchIndex: -1,
      onSeek: vi.fn(),
      onTogglePlay: vi.fn(),
      onRerun: vi.fn(),
    });

    expect(Element.prototype.scrollIntoView).not.toHaveBeenCalled();
  });

  it('toggles play on Space when focus is on a segment row, without scrolling the page', async () => {
    const onTogglePlay = vi.fn();
    render(SegmentList, {
      segments: segments(),
      currentTime: 0,
      playing: false,
      rerunStarting: false,
      onSeek: vi.fn(),
      onTogglePlay,
      onRerun: vi.fn(),
    });

    const row = screen.getByText('đoạn một').closest('[role="button"]')!;
    const event = await fireEvent.keyDown(row, { key: ' ' });
    expect(onTogglePlay).toHaveBeenCalledTimes(1);
    expect(event).toBe(false); // fireEvent returns false when preventDefault() was called
  });

  it('does not swallow Space when focus is on a nested button (the gap rerun button)', async () => {
    const onTogglePlay = vi.fn();
    const onRerun = vi.fn();
    render(SegmentList, {
      segments: [
        { idx: 0, startSec: 0, endSec: 5, kind: 'gap', gapReason: 'chunk_failed', text: '' },
      ],
      currentTime: 0,
      playing: false,
      rerunStarting: false,
      onSeek: vi.fn(),
      onTogglePlay,
      onRerun,
    });

    const button = screen.getByRole('button', { name: /Chạy lại khoảng này/ });
    await fireEvent.keyDown(button, { key: ' ' });
    expect(onTogglePlay).not.toHaveBeenCalled();
  });

  it('a chunk_failed gap row has a warning background and a rerun button with the gap idx', async () => {
    const onRerun = vi.fn();
    render(SegmentList, {
      segments: [
        { idx: 3, startSec: 30, endSec: 40, kind: 'gap', gapReason: 'chunk_failed', text: '' },
      ],
      currentTime: 0,
      playing: false,
      rerunStarting: false,
      onSeek: vi.fn(),
      onTogglePlay: vi.fn(),
      onRerun,
    });

    expect(screen.getByText('Đoạn này bị lỗi khi transcribe')).toBeTruthy();
    const button = screen.getByRole('button', { name: /Chạy lại khoảng này/ });
    await fireEvent.click(button);
    expect(onRerun).toHaveBeenCalledWith({ kind: 'gap', gapId: 3 });
  });

  it('a disconnected gap row is neutral with no button', () => {
    render(SegmentList, {
      segments: [
        { idx: 2, startSec: 20, endSec: 25, kind: 'gap', gapReason: 'disconnected', text: '' },
      ],
      currentTime: 0,
      playing: false,
      rerunStarting: false,
      onSeek: vi.fn(),
      onTogglePlay: vi.fn(),
      onRerun: vi.fn(),
    });

    expect(screen.getByText('Mất kết nối')).toBeTruthy();
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('stops auto-scroll on a user wheel event while playing, and shows the jump-to-active button', async () => {
    render(SegmentList, {
      segments: segments(),
      currentTime: 12,
      playing: true,
      rerunStarting: false,
      onSeek: vi.fn(),
      onTogglePlay: vi.fn(),
      onRerun: vi.fn(),
    });

    expect(screen.queryByRole('button', { name: 'Xuống dòng đang phát' })).toBeNull();

    const region = screen.getByRole('region', { name: 'Danh sách đoạn transcript' });
    await fireEvent.wheel(region);

    const jumpButton = await screen.findByRole('button', { name: 'Xuống dòng đang phát' });
    expect(jumpButton).toBeTruthy();

    await fireEvent.click(jumpButton);
    expect(screen.queryByRole('button', { name: 'Xuống dòng đang phát' })).toBeNull();
  });

  it('stops auto-scroll on ArrowDown while playing, and shows the jump-to-active button', async () => {
    render(SegmentList, {
      segments: segments(),
      currentTime: 12,
      playing: true,
      rerunStarting: false,
      onSeek: vi.fn(),
      onTogglePlay: vi.fn(),
      onRerun: vi.fn(),
    });

    const region = screen.getByRole('region', { name: 'Danh sách đoạn transcript' });
    expect(screen.queryByRole('button', { name: 'Xuống dòng đang phát' })).toBeNull();

    await fireEvent.keyDown(region, { key: 'ArrowDown' });

    expect(await screen.findByRole('button', { name: 'Xuống dòng đang phát' })).toBeTruthy();
  });

  it('stops auto-scroll on PageDown while playing, and shows the jump-to-active button', async () => {
    render(SegmentList, {
      segments: segments(),
      currentTime: 12,
      playing: true,
      rerunStarting: false,
      onSeek: vi.fn(),
      onTogglePlay: vi.fn(),
      onRerun: vi.fn(),
    });

    const region = screen.getByRole('region', { name: 'Danh sách đoạn transcript' });
    expect(screen.queryByRole('button', { name: 'Xuống dòng đang phát' })).toBeNull();

    await fireEvent.keyDown(region, { key: 'PageDown' });

    expect(await screen.findByRole('button', { name: 'Xuống dòng đang phát' })).toBeTruthy();
  });

  it('does not stop auto-scroll on ArrowDown/PageDown while paused', async () => {
    render(SegmentList, {
      segments: segments(),
      currentTime: 12,
      playing: false,
      rerunStarting: false,
      onSeek: vi.fn(),
      onTogglePlay: vi.fn(),
      onRerun: vi.fn(),
    });

    const region = screen.getByRole('region', { name: 'Danh sách đoạn transcript' });
    await fireEvent.keyDown(region, { key: 'ArrowDown' });
    await fireEvent.keyDown(region, { key: 'PageDown' });

    expect(screen.queryByRole('button', { name: 'Xuống dòng đang phát' })).toBeNull();
  });

  it('does not react to wheel scrolling while paused', async () => {
    render(SegmentList, {
      segments: segments(),
      currentTime: 12,
      playing: false,
      rerunStarting: false,
      onSeek: vi.fn(),
      onTogglePlay: vi.fn(),
      onRerun: vi.fn(),
    });

    const region = screen.getByRole('region', { name: 'Danh sách đoạn transcript' });
    await fireEvent.wheel(region);

    expect(screen.queryByRole('button', { name: 'Xuống dòng đang phát' })).toBeNull();
  });

  it('disables every gap rerun button while a rerun is starting', () => {
    render(SegmentList, {
      segments: [
        { idx: 0, startSec: 0, endSec: 5, kind: 'gap', gapReason: 'chunk_failed', text: '' },
      ],
      currentTime: 0,
      playing: false,
      rerunStarting: true,
      onSeek: vi.fn(),
      onTogglePlay: vi.fn(),
      onRerun: vi.fn(),
    });

    expect((screen.getByRole('button', { name: /Chạy lại khoảng này/ }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('shows an empty-state message when there are no segments', () => {
    render(SegmentList, {
      segments: [],
      currentTime: 0,
      playing: false,
      rerunStarting: false,
      onSeek: vi.fn(),
      onTogglePlay: vi.fn(),
      onRerun: vi.fn(),
    });

    expect(screen.getByText('Chưa có đoạn nào')).toBeTruthy();
  });
});
