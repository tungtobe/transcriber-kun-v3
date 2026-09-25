export type SearchableSegment = { text: string };

export type TranscriptSearchMatch = {
  segmentIndex: number;
  start: number;
  end: number;
};

type NormalizedText = {
  value: string;
  starts: number[];
  ends: number[];
};

/** Trim the query, collapse whitespace runs, and compare case-insensitively. */
export function normalizeTranscriptQuery(query: string): string {
  return query.trim().replace(/\s+/gu, ' ').toLowerCase();
}

/**
 * Normalize a segment while retaining offsets into its original UTF-16 text.
 * This lets the UI wrap matches in <mark> without changing transcript text.
 */
function normalizeTextWithOffsets(text: string): NormalizedText {
  let value = '';
  const starts: number[] = [];
  const ends: number[] = [];
  let pendingWhitespaceStart: number | null = null;
  let pendingWhitespaceEnd = 0;

  for (let index = 0; index < text.length;) {
    const codePoint = String.fromCodePoint(text.codePointAt(index)!);
    const start = index;
    index += codePoint.length;

    if (/\s/u.test(codePoint)) {
      if (value.length > 0) {
        pendingWhitespaceStart ??= start;
        pendingWhitespaceEnd = index;
      }
      continue;
    }

    if (pendingWhitespaceStart !== null) {
      value += ' ';
      starts.push(pendingWhitespaceStart);
      ends.push(pendingWhitespaceEnd);
      pendingWhitespaceStart = null;
    }

    const lowered = codePoint.toLowerCase();
    value += lowered;
    for (let offset = 0; offset < lowered.length; offset += 1) {
      starts.push(start);
      ends.push(index);
    }
  }

  return { value, starts, ends };
}

/** Return every non-overlapping query occurrence, in transcript order. */
export function findTranscriptMatches(
  segments: readonly SearchableSegment[],
  query: string,
): TranscriptSearchMatch[] {
  const normalizedQuery = normalizeTranscriptQuery(query);
  if (!normalizedQuery) return [];

  const matches: TranscriptSearchMatch[] = [];
  segments.forEach((segment, segmentIndex) => {
    const normalized = normalizeTextWithOffsets(segment.text);
    let cursor = 0;
    while (cursor <= normalized.value.length - normalizedQuery.length) {
      const matchStart = normalized.value.indexOf(normalizedQuery, cursor);
      if (matchStart < 0) break;
      const matchEnd = matchStart + normalizedQuery.length;
      const start = normalized.starts[matchStart];
      const end = normalized.ends[matchEnd - 1];
      if (start !== undefined && end !== undefined && end > start) {
        matches.push({ segmentIndex, start, end });
      }
      cursor = matchStart + normalizedQuery.length;
    }
  });
  return matches;
}

/** Advance or retreat through matches, wrapping at either end. */
export function cycleTranscriptMatch(currentIndex: number, count: number, direction: -1 | 1): number {
  if (count <= 0) return -1;
  if (!Number.isInteger(currentIndex) || currentIndex < 0 || currentIndex >= count) {
    return direction === 1 ? 0 : count - 1;
  }
  return (currentIndex + direction + count) % count;
}
