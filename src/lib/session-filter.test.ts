import { describe, expect, it } from 'vitest';
import { EMPTY_TAG_FILTER, filterSessions, isTagFilterActive, type TagFilterState } from './session-filter';
import type { SessionListItem } from './bindings';

function item(sessionId: string, tagIds: string[]): SessionListItem {
  return {
    sessionId,
    kind: 'file',
    title: `phiên ${sessionId}`,
    createdAt: 0,
    durationSec: 1,
    recovered: false,
    missingGapCount: 0,
    tagIds,
  };
}

describe('filterSessions', () => {
  const sessions = [
    item('a', ['x', 'y']),
    item('b', ['x']),
    item('c', []),
  ];

  it('returns every session unfiltered when no tag is selected and untagged is off', () => {
    expect(filterSessions(sessions, EMPTY_TAG_FILTER)).toEqual(sessions);
  });

  it('AND-filters: a session must have every selected tag', () => {
    const filter: TagFilterState = { tagIds: ['x'], untagged: false };
    expect(filterSessions(sessions, filter).map((s) => s.sessionId)).toEqual(['a', 'b']);

    const both: TagFilterState = { tagIds: ['x', 'y'], untagged: false };
    expect(filterSessions(sessions, both).map((s) => s.sessionId)).toEqual(['a']);
  });

  it('a tag with no matching session yields an empty result', () => {
    const filter: TagFilterState = { tagIds: ['z'], untagged: false };
    expect(filterSessions(sessions, filter)).toEqual([]);
  });

  it('untagged=true returns only sessions with zero tags, ignoring any tagIds', () => {
    const filter: TagFilterState = { tagIds: ['x'], untagged: true };
    expect(filterSessions(sessions, filter).map((s) => s.sessionId)).toEqual(['c']);
  });
});

describe('isTagFilterActive', () => {
  it('is false only when nothing is selected', () => {
    expect(isTagFilterActive(EMPTY_TAG_FILTER)).toBe(false);
    expect(isTagFilterActive({ tagIds: [], untagged: true })).toBe(true);
    expect(isTagFilterActive({ tagIds: ['x'], untagged: false })).toBe(true);
  });
});
