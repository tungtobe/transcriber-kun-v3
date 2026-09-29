import { describe, expect, it } from 'vitest';
import {
  EMPTY_SESSION_FILTER,
  EMPTY_TAG_FILTER,
  filterSessions,
  isSessionFilterActive,
  isTagFilterActive,
  normalizeSearchText,
  type SessionFilterState,
  type TagFilterState,
} from './session-filter';
import type { SessionListItem } from './bindings';

function item(sessionId: string, tagIds: string[], title = `phiên ${sessionId}`): SessionListItem {
  return {
    sessionId,
    kind: 'file',
    status: 'complete',
    recordingAvailable: false,
    title,
    createdAt: 0,
    durationSec: 1,
    recovered: false,
    missingGapCount: 0,
    tagIds,
  };
}

function filter(overrides: Partial<SessionFilterState> = {}): SessionFilterState {
  return { ...EMPTY_SESSION_FILTER, ...overrides };
}

describe('filterSessions', () => {
  const sessions = [
    item('a', ['x', 'y']),
    item('b', ['x']),
    item('c', []),
  ];

  it('returns every session unfiltered when no tag/query is set', () => {
    expect(filterSessions(sessions, EMPTY_SESSION_FILTER)).toEqual(sessions);
  });

  it('AND-filters: a session must have every selected tag', () => {
    expect(filterSessions(sessions, filter({ tagIds: ['x'] })).map((s) => s.sessionId)).toEqual(['a', 'b']);
    expect(filterSessions(sessions, filter({ tagIds: ['x', 'y'] })).map((s) => s.sessionId)).toEqual(['a']);
  });

  it('a tag with no matching session yields an empty result', () => {
    expect(filterSessions(sessions, filter({ tagIds: ['z'] }))).toEqual([]);
  });

  it('untagged=true returns only sessions with zero tags, ignoring any tagIds', () => {
    expect(filterSessions(sessions, filter({ tagIds: ['x'], untagged: true })).map((s) => s.sessionId)).toEqual(['c']);
  });

  // Story 3.3: I/O Matrix "Hoa thường + khoảng trắng", "Query + tag", "Xoá query".
  it('matches by name, case-insensitively and ignoring extra whitespace', () => {
    const named = [item('a', [], 'Họp sprint 12'), item('b', [], 'Review code')];
    expect(filterSessions(named, filter({ query: '  HỌP   sprint ' })).map((s) => s.sessionId)).toEqual(['a']);
  });

  it('an empty query (after normalizing) does not filter by name', () => {
    expect(filterSessions(sessions, filter({ query: '   ' }))).toEqual(sessions);
  });

  it('ANDs the name query with the tag filter', () => {
    const named = [
      item('a', ['x'], 'họp sprint'),
      item('b', ['x'], 'review code'),
      item('c', [], 'họp sprint khác'),
    ];
    expect(filterSessions(named, filter({ tagIds: ['x'], query: 'họp' })).map((s) => s.sessionId)).toEqual(['a']);
  });

  it('clearing the query keeps the tag filter untouched', () => {
    const named = [item('a', ['x'], 'họp sprint'), item('b', ['x'], 'review code')];
    const withQuery = filterSessions(named, filter({ tagIds: ['x'], query: 'họp' }));
    expect(withQuery.map((s) => s.sessionId)).toEqual(['a']);

    const cleared = filterSessions(named, filter({ tagIds: ['x'], query: '' }));
    expect(cleared.map((s) => s.sessionId)).toEqual(['a', 'b']);
  });

  it('a query with no matching name yields an empty result', () => {
    expect(filterSessions(sessions, filter({ query: 'zzz' }))).toEqual([]);
  });

  // Story 3.3, spec Boundaries Always: "tính toán phải ≤ 200 ms với 500 phiên".
  it('filters 500 sessions by a single-character query in under 200ms', () => {
    const many = Array.from({ length: 500 }, (_, i) => item(`s${i}`, [], `phiên số ${i}`));
    const start = performance.now();
    filterSessions(many, filter({ query: '5' }));
    expect(performance.now() - start).toBeLessThanOrEqual(200);
  });
});

describe('normalizeSearchText', () => {
  it('normalizes NFC, lowercases, trims, and collapses internal whitespace', () => {
    expect(normalizeSearchText('  HỌP   sprint ')).toBe('họp sprint');
  });
});

describe('isTagFilterActive', () => {
  it('is false only when nothing is selected', () => {
    expect(isTagFilterActive(EMPTY_TAG_FILTER)).toBe(false);
    expect(isTagFilterActive({ tagIds: [], untagged: true })).toBe(true);
    expect(isTagFilterActive({ tagIds: ['x'], untagged: false })).toBe(true);
  });
});

describe('isSessionFilterActive', () => {
  it('is false only when there is no tag filter and no (normalized) query', () => {
    expect(isSessionFilterActive(EMPTY_SESSION_FILTER)).toBe(false);
    expect(isSessionFilterActive(filter({ query: '   ' }))).toBe(false);
    expect(isSessionFilterActive(filter({ query: 'x' }))).toBe(true);
    expect(isSessionFilterActive(filter({ untagged: true }))).toBe(true);
    expect(isSessionFilterActive(filter({ tagIds: ['x'] }))).toBe(true);
  });
});
