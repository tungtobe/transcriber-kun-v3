// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';

const mocks = vi.hoisted(() => ({
  settingsStore: { timestampOffsetSec: 3_600 },
}));

vi.mock('../../lib/stores/settings.svelte', () => ({ settingsStore: mocks.settingsStore }));

import { i18n } from '../../i18n/index.svelte';
import SegmentList from './SegmentList.svelte';

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  Element.prototype.scrollIntoView = vi.fn();
});

describe('SegmentList with a global offset', () => {
  // spec I/O Matrix "Offset": offset 3600, Segment 65 s -> hiển thị `01:01:05`;
  // click seek `currentTime = 65` (start gốc, chưa cộng offset).
  it('displays the offset timestamp but seeks to the raw start_sec', async () => {
    const onSeek = vi.fn();
    render(SegmentList, {
      segments: [{ idx: 0, startSec: 65, endSec: 70, kind: 'text', gapReason: null, text: 'câu nói' }],
      currentTime: 0,
      playing: false,
      rerunStarting: false,
      onSeek,
      onTogglePlay: vi.fn(),
      onRerun: vi.fn(),
    });

    expect(screen.getByText('01:01:05')).toBeTruthy();
    await fireEvent.click(screen.getByText('câu nói'));
    expect(onSeek).toHaveBeenCalledWith(65);
  });
});
