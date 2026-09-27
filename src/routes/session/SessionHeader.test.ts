// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import { configureRouter } from '../../lib/router';
import SessionHeader from './SessionHeader.svelte';

afterEach(() => cleanup());

beforeEach(() => {
  configureRouter();
  i18n.applyPreference('vi');
});

function props(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    sessionId: 'session-1',
    title: 'cuộc họp',
    kind: 'file',
    createdAtMs: Date.UTC(2026, 0, 15),
    durationSec: 125,
    segmentTextCount: 42,
    recovered: false,
    partial: false,
    onRenamed: vi.fn(),
    onDeleted: vi.fn(),
    ...overrides,
  };
}

describe('SessionHeader', () => {
  it('renders the title, a FILE badge, duration, and the text-segment count', () => {
    render(SessionHeader, props());

    expect(screen.getByRole('heading', { name: 'cuộc họp' })).toBeTruthy();
    expect(screen.getByText('Tệp')).toBeTruthy();
    expect(screen.getByText('02:05')).toBeTruthy();
    expect(screen.getByText('42 đoạn')).toBeTruthy();
    expect(screen.queryByText('Đã phục hồi')).toBeNull();
    expect(screen.queryByText('Chưa đầy đủ')).toBeNull();
  });

  it('shows a LIVE badge instead of FILE for a live session', () => {
    render(
      SessionHeader,
      props({
        title: 'phiên live',
        kind: 'live',
        createdAtMs: null,
        durationSec: null,
        segmentTextCount: 0,
      }),
    );

    expect(screen.getByText('Live')).toBeTruthy();
    expect(screen.queryByText('Tệp')).toBeNull();
  });

  it('shows the recover badge when recovered, and the partial badge when partial', () => {
    render(
      SessionHeader,
      props({ createdAtMs: null, durationSec: null, segmentTextCount: 0, recovered: true, partial: true }),
    );

    expect(screen.getByText('Đã phục hồi')).toBeTruthy();
    expect(screen.getByText('Chưa đầy đủ')).toBeTruthy();
  });

  it('links back to Home', () => {
    render(SessionHeader, props({ createdAtMs: null, durationSec: null, segmentTextCount: 0 }));

    const back = screen.getByRole('link', { name: 'Quay lại' });
    expect(back.getAttribute('href')).toContain('/home');
  });

  it('clicking the title opens inline rename with the current title preselected', async () => {
    render(SessionHeader, props());

    await fireEvent.click(screen.getByRole('button', { name: 'cuộc họp' }));

    const input = screen.getByRole('textbox', { name: 'Tên phiên' });
    expect((input as HTMLInputElement).value).toBe('cuộc họp');
  });

  it('the ⋯ menu exposes Đổi tên and Xoá', async () => {
    render(SessionHeader, props());

    await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));

    expect(screen.getByRole('menuitem', { name: 'Đổi tên' })).toBeTruthy();
    expect(screen.getByRole('menuitem', { name: 'Xoá' })).toBeTruthy();
  });

  it('choosing Xoá from the menu opens the delete confirmation dialog', async () => {
    render(SessionHeader, props());

    await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Xoá' }));

    expect(screen.getByRole('alertdialog')).toBeTruthy();
    expect(screen.getByText('Xoá phiên này?')).toBeTruthy();
  });
});
