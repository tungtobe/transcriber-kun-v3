// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { cleanup, render, screen } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import SessionHeader from './SessionHeader.svelte';

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
});

describe('SessionHeader', () => {
  it('renders the title, a FILE badge, duration, and the text-segment count', () => {
    render(SessionHeader, {
      title: 'cuộc họp',
      kind: 'file',
      createdAtMs: Date.UTC(2026, 0, 15),
      durationSec: 125,
      segmentTextCount: 42,
      recovered: false,
      partial: false,
    });

    expect(screen.getByRole('heading', { name: 'cuộc họp' })).toBeTruthy();
    expect(screen.getByText('Tệp')).toBeTruthy();
    expect(screen.getByText('02:05')).toBeTruthy();
    expect(screen.getByText('42 đoạn')).toBeTruthy();
    expect(screen.queryByText('Đã phục hồi')).toBeNull();
    expect(screen.queryByText('Chưa đầy đủ')).toBeNull();
  });

  it('shows a LIVE badge instead of FILE for a live session', () => {
    render(SessionHeader, {
      title: 'phiên live',
      kind: 'live',
      createdAtMs: null,
      durationSec: null,
      segmentTextCount: 0,
      recovered: false,
      partial: false,
    });

    expect(screen.getByText('Live')).toBeTruthy();
    expect(screen.queryByText('Tệp')).toBeNull();
  });

  it('shows the recover badge when recovered, and the partial badge when partial', () => {
    render(SessionHeader, {
      title: 'cuộc họp',
      kind: 'file',
      createdAtMs: null,
      durationSec: null,
      segmentTextCount: 0,
      recovered: true,
      partial: true,
    });

    expect(screen.getByText('Đã phục hồi')).toBeTruthy();
    expect(screen.getByText('Chưa đầy đủ')).toBeTruthy();
  });

  it('links back to Home', () => {
    render(SessionHeader, {
      title: 'cuộc họp',
      kind: 'file',
      createdAtMs: null,
      durationSec: null,
      segmentTextCount: 0,
      recovered: false,
      partial: false,
    });

    const back = screen.getByRole('link', { name: 'Quay lại' });
    expect(back.getAttribute('href')).toContain('/home');
  });
});
