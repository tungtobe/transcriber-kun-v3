// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  settingsStore: { timestampOffsetSec: 0 },
}));

vi.mock('./stores/settings.svelte', () => ({ settingsStore: mocks.settingsStore }));

import { displayTimestamp, formatTimestamp } from './time';

describe('formatTimestamp', () => {
  it('formats under an hour as MM:SS with no offset', () => {
    expect(formatTimestamp(0)).toBe('00:00');
    expect(formatTimestamp(65)).toBe('01:05');
    expect(formatTimestamp(3_599)).toBe('59:59');
  });

  it('formats an hour or more as HH:MM:SS', () => {
    expect(formatTimestamp(3_600)).toBe('01:00:00');
    expect(formatTimestamp(3_665)).toBe('01:01:05');
  });

  // spec I/O Matrix "Offset hiển thị": "Segment 65 s, offset 3600 -> 01:01:05;
  // offset 0 và 65 s -> 01:05".
  it('adds the offset before deciding whether to show the hour segment', () => {
    expect(formatTimestamp(65, 3_600)).toBe('01:01:05');
    expect(formatTimestamp(65, 0)).toBe('01:05');
  });

  it('rounds fractional seconds to the nearest whole second', () => {
    expect(formatTimestamp(59.6)).toBe('01:00');
  });

  it('clamps negative or non-finite results to 00:00 instead of rendering a negative timestamp', () => {
    expect(formatTimestamp(-5)).toBe('00:00');
    expect(formatTimestamp(Number.NaN)).toBe('00:00');
    expect(formatTimestamp(5, -100)).toBe('00:00');
  });
});

describe('displayTimestamp', () => {
  it('reads the offset reactively from settingsStore', () => {
    mocks.settingsStore.timestampOffsetSec = 0;
    expect(displayTimestamp(65)).toBe('01:05');

    mocks.settingsStore.timestampOffsetSec = 3_600;
    expect(displayTimestamp(65)).toBe('01:01:05');
  });
});
