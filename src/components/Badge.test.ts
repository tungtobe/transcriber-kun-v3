// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, render, screen } from '@testing-library/svelte';
import Badge from './Badge.svelte';
import type { BadgeVariant } from './Badge.svelte';

afterEach(() => cleanup());

const VARIANTS: BadgeVariant[] = ['memo', 'audio', 'partial', 'recover', 'live', 'file', 'token'];

describe('Badge', () => {
  it.each(VARIANTS)('renders the %s variant with its own class and the given label', (variant) => {
    render(Badge, { variant, label: variant.toUpperCase() });
    const badge = screen.getByText(variant.toUpperCase());
    expect(badge.className).toContain(`badge-${variant}`);
  });

  it('never applies a fixed width style — only the shared min-width from the class', () => {
    render(Badge, { variant: 'file', label: 'FILE' });
    const badge = screen.getByText('FILE');
    expect(badge.style.width).toBe('');
  });
});
