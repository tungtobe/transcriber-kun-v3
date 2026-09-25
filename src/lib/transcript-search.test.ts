import { describe, expect, it } from 'vitest';
import { cycleTranscriptMatch, findTranscriptMatches, normalizeTranscriptQuery } from './transcript-search';

describe('transcript search', () => {
  it('trims and collapses query whitespace and compares without case sensitivity', () => {
    const matches = findTranscriptMatches(
      [{ text: 'Trước đó' }, { text: 'Giao diện  đang tải' }],
      '  giao   diện  ',
    );

    expect(normalizeTranscriptQuery('  giao   diện  ')).toBe('giao diện');
    expect(matches).toEqual([{ segmentIndex: 1, start: 0, end: 9 }]);
  });

  it('maps collapsed transcript whitespace back to the original text range', () => {
    expect(findTranscriptMatches([{ text: 'A  giao\n\t  diện B' }], 'giao diện')).toEqual([
      { segmentIndex: 0, start: 3, end: 15 },
    ]);
  });

  it('returns all non-overlapping occurrences in order', () => {
    expect(findTranscriptMatches([{ text: 'Ha ha ha' }], 'ha')).toEqual([
      { segmentIndex: 0, start: 0, end: 2 },
      { segmentIndex: 0, start: 3, end: 5 },
      { segmentIndex: 0, start: 6, end: 8 },
    ]);
  });

  it('matches a segment in NFD against a query in NFC (spec I/O Matrix "NFD text")', () => {
    // "Việt" decomposed to NFD: 'V','i','e' + combining circumflex (U+0302)
    // + combining dot below (U+0323) + 't' — a different UTF-16 length than
    // the NFC form the query is typed in.
    const nfdSegment = 'Việt Nam';
    expect(nfdSegment.normalize('NFC')).toBe('Việt Nam');
    expect(nfdSegment.length).not.toBe('Việt Nam'.length);

    const matches = findTranscriptMatches([{ text: nfdSegment }], 'việt');
    expect(matches).toHaveLength(1);
    expect(matches[0].segmentIndex).toBe(0);
    // The highlight must cover the whole original (NFD) "Việt" span, not a
    // truncated/misaligned slice.
    expect(nfdSegment.slice(matches[0].start, matches[0].end)).toBe('Việt');
  });

  it('matches a query typed in NFD against segment text already in NFC', () => {
    const nfdQuery = 'việt';
    const matches = findTranscriptMatches([{ text: 'Việt Nam' }], nfdQuery);
    expect(matches).toEqual([{ segmentIndex: 0, start: 0, end: 4 }]);
  });

  it('returns no matches for empty or absent queries', () => {
    expect(findTranscriptMatches([{ text: 'Giao diện' }], '   \t  ')).toEqual([]);
    expect(findTranscriptMatches([{ text: 'Giao diện' }], 'không có')).toEqual([]);
  });

  it('cycles forward and backward with wraparound and handles an empty result', () => {
    expect(cycleTranscriptMatch(0, 3, 1)).toBe(1);
    expect(cycleTranscriptMatch(2, 3, 1)).toBe(0);
    expect(cycleTranscriptMatch(0, 3, -1)).toBe(2);
    expect(cycleTranscriptMatch(-1, 3, 1)).toBe(0);
    expect(cycleTranscriptMatch(-1, 3, -1)).toBe(2);
    expect(cycleTranscriptMatch(0, 0, 1)).toBe(-1);
  });

  it('searches a transcript with 700 segments', () => {
    const segments = Array.from({ length: 700 }, (_, index) => ({
      text: `Transcript segment ${index}: a longer discussion containing the TARGET phrase and additional words for realistic search work.`,
    }));

    // Warm the function before recording five passes so startup/JIT noise is
    // less likely to dominate this simple acceptance check.
    findTranscriptMatches(segments, 'TARGET phrase');
    const durations: number[] = [];
    for (let iteration = 0; iteration < 5; iteration += 1) {
      const start = performance.now();
      const matches = findTranscriptMatches(segments, 'TARGET phrase');
      durations.push(performance.now() - start);
      expect(matches).toHaveLength(700);
      expect(matches[699]).toEqual({ segmentIndex: 699, start: 59, end: 72 });
    }
    durations.sort((left, right) => left - right);
    expect(durations[2]).toBeLessThan(100);
  });
});
