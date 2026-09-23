// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { cleanup, render, screen } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import SessionListSkeleton from './SessionListSkeleton.svelte';

afterEach(() => cleanup());
beforeEach(() => i18n.applyPreference('vi'));

describe('SessionListSkeleton', () => {
  it('renders exactly 6 skeleton rows matching the real row layout', () => {
    const { container } = render(SessionListSkeleton);
    expect(container.querySelectorAll('.skeleton-row').length).toBe(6);
    for (const row of container.querySelectorAll('.skeleton-row')) {
      expect(row.querySelectorAll('.skeleton-bar').length).toBe(4);
    }
  });

  it('exposes a status region so loading is announced', () => {
    render(SessionListSkeleton);
    expect(screen.getByRole('status')).toBeTruthy();
  });
});
