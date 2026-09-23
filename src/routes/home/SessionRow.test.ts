// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { cleanup, render, screen } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import { configureRouter } from '../../lib/router';
import SessionRow from './SessionRow.svelte';
import type { SessionListItem } from '../../lib/bindings';

function item(overrides: Partial<SessionListItem> = {}): SessionListItem {
  return {
    sessionId: 'abc',
    kind: 'file',
    title: 'cuộc họp',
    createdAt: Date.UTC(2026, 0, 15),
    durationSec: 65,
    recovered: false,
    missingGapCount: 0,
    ...overrides,
  };
}

afterEach(() => cleanup());

beforeEach(() => {
  configureRouter();
  i18n.applyPreference('vi');
});

describe('SessionRow', () => {
  it('is a link to /session/:id with the title', () => {
    render(SessionRow, { session: item({ sessionId: 'xyz', title: 'buổi họp nhóm' }) });
    const row = screen.getByRole('link', { name: /buổi họp nhóm/ });
    expect(row.getAttribute('href')).toBe('/session/xyz');
  });

  it('never shows the "Thiếu N khoảng" badge when missingGapCount is 0', () => {
    render(SessionRow, { session: item({ missingGapCount: 0 }) });
    expect(screen.queryByText(/Thiếu/)).toBeNull();
  });

  it('shows "Thiếu N khoảng" right after the title when missingGapCount > 0', () => {
    render(SessionRow, { session: item({ missingGapCount: 3 }) });
    expect(screen.getByText('Thiếu 3 khoảng')).toBeTruthy();
  });

  it('shows "Phục hồi" when recovered', () => {
    render(SessionRow, { session: item({ recovered: true }) });
    expect(screen.getByText('Phục hồi')).toBeTruthy();
  });

  it('does not show "Phục hồi" when not recovered', () => {
    render(SessionRow, { session: item({ recovered: false }) });
    expect(screen.queryByText('Phục hồi')).toBeNull();
  });

  it('shows a file badge for kind=file and a live badge for kind=live', () => {
    const { unmount } = render(SessionRow, { session: item({ kind: 'file' }) });
    expect(screen.getByText('Tệp')).toBeTruthy();
    unmount();

    render(SessionRow, { session: item({ kind: 'live' }) });
    expect(screen.getByText('Live')).toBeTruthy();
  });

  it('formats duration as MM:SS under an hour and HH:MM:SS at/over an hour', () => {
    const { unmount } = render(SessionRow, { session: item({ durationSec: 65 }) });
    expect(screen.getByText('01:05')).toBeTruthy();
    unmount();

    render(SessionRow, { session: item({ durationSec: 3665 }) });
    expect(screen.getByText('01:01:05')).toBeTruthy();
  });

  it('never shows model, segment count, or a status column', () => {
    render(SessionRow, { session: item() });
    expect(screen.queryByText(/gemini/i)).toBeNull();
    expect(screen.queryByText(/segment/i)).toBeNull();
    expect(screen.queryByText(/status|trạng thái/i)).toBeNull();
  });
});
