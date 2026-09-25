import { afterEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({ settingsStore: { timestampOffsetSec: 0 } }));
vi.mock('./stores/settings.svelte', () => ({ settingsStore: mocks.settingsStore }));

import { formatTranscriptCopyText } from './transcript-export';

afterEach(() => {
  mocks.settingsStore.timestampOffsetSec = 0;
});

describe('formatTranscriptCopyText', () => {
  const labels = {
    chunkFailed: 'Missing after a transcription error',
    disconnected: 'Connection lost',
    unknown: 'Missing section',
  };

  it('copies every text segment and describes gaps using display offset', () => {
    expect(formatTranscriptCopyText([
      { startSec: 10, endSec: 12, kind: 'text', gapReason: null, text: 'Xin chào' },
      { startSec: 12, endSec: 20, kind: 'gap', gapReason: 'chunk_failed', text: '' },
      { startSec: 20, endSec: 25, kind: 'gap', gapReason: 'disconnected', text: '' },
    ], 3_600, labels)).toBe(
      '[01:00:10] Xin chào\n[01:00:12–01:00:20] Missing after a transcription error\n[01:00:20–01:00:25] Connection lost',
    );
  });

  it('clamps displayed negative times without mutating the segment values', () => {
    const segment = { startSec: -5, endSec: -1, kind: 'gap', gapReason: null, text: '' };
    expect(formatTranscriptCopyText([segment], 0, labels)).toBe('[00:00–00:00] Missing section');
    expect(segment.startSec).toBe(-5);
    expect(segment.endSec).toBe(-1);
  });
});
