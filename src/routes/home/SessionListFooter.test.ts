// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import SessionListFooter from './SessionListFooter.svelte';

const mocks = vi.hoisted(() => ({
  clearAllFilters: vi.fn(),
  sessions: [] as unknown[],
  filteredSessions: [] as unknown[],
  tagFilter: { tagIds: [] as string[], untagged: false },
  nameQuery: '',
}));

vi.mock('../../lib/stores/library.svelte', () => ({
  libraryStore: {
    clearAllFilters: (...args: unknown[]) => mocks.clearAllFilters(...args),
    get sessions() {
      return mocks.sessions;
    },
    get filteredSessions() {
      return mocks.filteredSessions;
    },
    get tagFilter() {
      return mocks.tagFilter;
    },
    get nameQuery() {
      return mocks.nameQuery;
    },
  },
}));

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  mocks.clearAllFilters.mockReset();
  mocks.sessions = [];
  mocks.filteredSessions = [];
  mocks.tagFilter = { tagIds: [], untagged: false };
  mocks.nameQuery = '';
});

describe('SessionListFooter', () => {
  it('renders nothing when there are no sessions at all', () => {
    render(SessionListFooter);
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('shows a plain total with no clear button when not filtering', () => {
    mocks.sessions = [1, 2, 3];
    mocks.filteredSessions = [1, 2, 3];
    render(SessionListFooter);

    expect(screen.getByText('3 phiên')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Xoá bộ lọc' })).toBeNull();
  });

  it('shows "shown / total" + a tag-count note + a clear button when a tag filter is active', async () => {
    mocks.sessions = Array.from({ length: 38 }, (_, i) => i);
    mocks.filteredSessions = Array.from({ length: 6 }, (_, i) => i);
    mocks.tagFilter = { tagIds: ['a', 'b'], untagged: false };
    render(SessionListFooter);

    expect(screen.getByText(/6 \/ 38 phiên/)).toBeTruthy();
    expect(screen.getByText(/2 tag/)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Xoá bộ lọc' }));
    expect(mocks.clearAllFilters).toHaveBeenCalledTimes(1);
  });

  it('shows the "Chưa gắn tag" note when untagged filtering is active', () => {
    mocks.sessions = [1, 2];
    mocks.filteredSessions = [1];
    mocks.tagFilter = { tagIds: [], untagged: true };
    render(SessionListFooter);

    expect(screen.getByText(/Chưa gắn tag/)).toBeTruthy();
  });

  it('shows the query in the note when a name query is active, quoted', () => {
    mocks.sessions = [1, 2];
    mocks.filteredSessions = [1];
    mocks.nameQuery = '  họp  ';
    render(SessionListFooter);

    expect(screen.getByText(/họp/)).toBeTruthy();
  });

  it('combines the query and tag note when both are active', () => {
    mocks.sessions = [1, 2];
    mocks.filteredSessions = [1];
    mocks.nameQuery = 'họp';
    mocks.tagFilter = { tagIds: ['a'], untagged: false };
    render(SessionListFooter);

    const note = screen.getByRole('status');
    expect(note.textContent).toMatch(/họp/);
    expect(note.textContent).toMatch(/1 tag/);
  });

  it('is a live region', () => {
    mocks.sessions = [1];
    mocks.filteredSessions = [1];
    render(SessionListFooter);
    expect(screen.getByRole('status').getAttribute('aria-live')).toBe('polite');
  });
});
