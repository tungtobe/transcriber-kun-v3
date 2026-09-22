// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, render, screen } from '@testing-library/svelte';
import BannerStack, { type BannerItem } from './BannerStack.svelte';

afterEach(() => cleanup());

const banners: BannerItem[] = [
  { id: 'info-1', variant: 'info', title: 'Info title', message: 'Info message' },
  { id: 'danger-1', variant: 'danger', title: 'Danger title', message: 'Danger message' },
  { id: 'warning-1', variant: 'warning', title: 'Warning title', message: 'Warning message' },
];

describe('BannerStack', () => {
  it('shows only danger and warning, in that order, capped at two when three variants are present', () => {
    render(BannerStack, { banners });

    const statuses = screen.getAllByRole('status');
    expect(statuses).toHaveLength(2);
    expect(statuses[0].textContent).toContain('Danger title');
    expect(statuses[1].textContent).toContain('Warning title');
    expect(screen.queryByText('Info title')).toBeNull();
  });

  it('renders nothing when there are no banners', () => {
    const { container } = render(BannerStack, { banners: [] });

    expect(container.querySelector('.banner-stack')).toBeNull();
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('renders a single banner unchanged', () => {
    render(BannerStack, {
      banners: [{ id: 'only', variant: 'warning', title: 'Solo', message: 'Just one' }],
    });

    expect(screen.getAllByRole('status')).toHaveLength(1);
    expect(screen.getByText('Solo')).toBeTruthy();
  });
});
