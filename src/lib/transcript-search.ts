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

/** Trim the query, collapse whitespace runs, normalize to NFC, and compare
 * case-insensitively — so a query typed/pasted in NFD (e.g. some IME/OS
 * clipboard paths) still matches transcript text normalized the same way
 * (spec Always: "normalize both the query and the segment text with
 * `normalize('NFC')` before case-folding"). */
export function normalizeTranscriptQuery(query: string): string {
  return query.normalize('NFC').trim().replace(/\s+/gu, ' ').toLowerCase();
}

/** Unicode combining marks (general category M) — grouped with the
 * preceding base character below so a cluster is normalized as one unit,
 * matching how NFC would compose the same sequence. */
const COMBINING_MARK = /^\p{M}$/u;

/**
 * Normalize a segment while retaining offsets into its original UTF-16 text.
 * This lets the UI wrap matches in <mark> without changing transcript text.
 *
 * NFC normalization can change codepoint *count* (e.g. NFD "Việt" — 'e' +
 * combining circumflex + combining dot below — composes to one 'ệ'
 * codepoint), so a straight per-codepoint offset map would go stale wherever
 * that happens. Instead each base character is grouped with any combining
 * marks immediately following it into one cluster, that cluster alone is
 * `normalize('NFC')`-d, and every output character produced from it shares
 * the cluster's original `[start, end)` span — keeping offsets valid against
 * the original text even when NFC shortens the cluster (spec Code Map:
 * "build the offset map from the NFC-normalized text back to the original
 * indices").
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

    let cluster = codePoint;
    let clusterEnd = index;
    while (index < text.length) {
      const next = String.fromCodePoint(text.codePointAt(index)!);
      if (!COMBINING_MARK.test(next)) break;
      cluster += next;
      index += next.length;
      clusterEnd = index;
    }

    const normalizedCluster = cluster.normalize('NFC').toLowerCase();
    value += normalizedCluster;
    for (let offset = 0; offset < normalizedCluster.length; offset += 1) {
      starts.push(start);
      ends.push(clusterEnd);
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
