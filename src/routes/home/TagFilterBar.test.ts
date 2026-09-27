// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import TagFilterBar from './TagFilterBar.svelte';

const mocks = vi.hoisted(() => ({
  toggleFilterTag: vi.fn(),
  toggleUntaggedFilter: vi.fn(),
  createTag: vi.fn(),
  deleteTagGlobally: vi.fn(),
  tags: [] as Array<{ id: string; name: string; sessionCount: number }>,
  tagFilter: { tagIds: [] as string[], untagged: false },
}));

vi.mock('../../lib/stores/library.svelte', () => ({
  libraryStore: {
    toggleFilterTag: (...args: unknown[]) => mocks.toggleFilterTag(...args),
    toggleUntaggedFilter: (...args: unknown[]) => mocks.toggleUntaggedFilter(...args),
    createTag: (...args: unknown[]) => mocks.createTag(...args),
    deleteTagGlobally: (...args: unknown[]) => mocks.deleteTagGlobally(...args),
    get tags() {
      return mocks.tags;
    },
    get tagFilter() {
      return mocks.tagFilter;
    },
  },
}));

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  mocks.toggleFilterTag.mockReset();
  mocks.toggleUntaggedFilter.mockReset();
  mocks.createTag.mockReset();
  mocks.deleteTagGlobally.mockReset();
  mocks.tags = [];
  mocks.tagFilter = { tagIds: [], untagged: false };
});

describe('TagFilterBar', () => {
  it('renders nothing when there are no tags at all', () => {
    render(TagFilterBar);
    expect(screen.queryByText('Chưa gắn tag')).toBeNull();
  });

  it('shows up to 5 most-used unfiltered tags plus a "+N tag khác" chip for the rest', () => {
    mocks.tags = Array.from({ length: 7 }, (_, i) => ({ id: `t${i}`, name: `tag${i}`, sessionCount: 7 - i }));
    render(TagFilterBar);

    for (let i = 0; i < 5; i += 1) {
      expect(screen.getByText(`tag${i}`)).toBeTruthy();
    }
    expect(screen.queryByText('tag5')).toBeNull();
    expect(screen.getByText('+ 2 tag khác')).toBeTruthy();
  });

  it('clicking a popular tag chip toggles it as a filter', async () => {
    mocks.tags = [{ id: 't1', name: 'sprint-12', sessionCount: 3 }];
    render(TagFilterBar);
    await fireEvent.click(screen.getByText('sprint-12'));
    expect(mocks.toggleFilterTag).toHaveBeenCalledWith('t1');
  });

  it('shows filtered tags first with an × to remove, separate from the popular list', async () => {
    mocks.tags = [
      { id: 't1', name: 'A', sessionCount: 5 },
      { id: 't2', name: 'B', sessionCount: 3 },
    ];
    mocks.tagFilter = { tagIds: ['t2'], untagged: false };
    render(TagFilterBar);

    const filteredChip = screen.getByText('B').closest('button')!;
    await fireEvent.click(filteredChip);
    expect(mocks.toggleFilterTag).toHaveBeenCalledWith('t2');
    // "A" still shows as an unfiltered popular chip.
    expect(screen.getByText('A')).toBeTruthy();
  });

  it('always shows the "Chưa gắn tag" chip once at least one tag exists, and clicking it toggles untagged', async () => {
    mocks.tags = [{ id: 't1', name: 'A', sessionCount: 1 }];
    render(TagFilterBar);
    await fireEvent.click(screen.getByText('Chưa gắn tag'));
    expect(mocks.toggleUntaggedFilter).toHaveBeenCalledTimes(1);
  });

  it('opens the tag picker from the "+N tag khác" chip', async () => {
    mocks.tags = Array.from({ length: 8 }, (_, i) => ({ id: `t${i}`, name: `tag${i}`, sessionCount: 1 }));
    render(TagFilterBar);
    await fireEvent.click(screen.getByText(/tag khác/));
    expect(screen.getByRole('dialog')).toBeTruthy();
  });
});
