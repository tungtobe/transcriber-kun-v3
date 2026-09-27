// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { cleanup, render } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import { configureRouter } from '../../lib/router';
import SessionList from './SessionList.svelte';
import type { SessionListItem } from '../../lib/bindings';

function sessions(count: number): SessionListItem[] {
  return Array.from({ length: count }, (_, i) => ({
    sessionId: `s${i}`,
    kind: 'file',
    title: `phiên ${i}`,
    createdAt: i,
    durationSec: 10,
    recovered: false,
    missingGapCount: 0,
    tagIds: [],
  }));
}

afterEach(() => cleanup());

beforeEach(() => {
  configureRouter();
  i18n.applyPreference('vi');
});

describe('SessionList virtualization', () => {
  it('renders far fewer DOM rows than the full 500-session list at an 800px viewport', () => {
    const { container } = render(SessionList, { sessions: sessions(500), viewportHeight: 800 });

    const rows = container.querySelectorAll('.session-row');
    expect(rows.length).toBeLessThan(60);
    expect(rows.length).toBeGreaterThan(0);
  });

  it('keeps the spacer at the full list height so scrollbar size reflects the whole list', () => {
    const { container } = render(SessionList, { sessions: sessions(500), viewportHeight: 800 });
    const spacer = container.querySelector('.session-list-spacer') as HTMLElement;
    // ROW_HEIGHT (56) × 500 sessions.
    expect(spacer.style.height).toBe('28000px');
  });

  it('renders every row (no virtualization ceiling) when the list is small', () => {
    const { container } = render(SessionList, { sessions: sessions(5), viewportHeight: 800 });
    expect(container.querySelectorAll('.session-row').length).toBe(5);
  });

  it('renders nothing when the list is empty', () => {
    const { container } = render(SessionList, { sessions: [], viewportHeight: 800 });
    expect(container.querySelectorAll('.session-row').length).toBe(0);
  });

  it('falls back to a sensible default viewport when no override is given and clientHeight is 0 (jsdom)', () => {
    const { container } = render(SessionList, { sessions: sessions(500) });
    const rows = container.querySelectorAll('.session-row');
    expect(rows.length).toBeGreaterThan(0);
    expect(rows.length).toBeLessThan(60);
  });
});
