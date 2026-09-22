// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { cleanup, render, screen } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import Settings from './Settings.svelte';

afterEach(() => cleanup());

describe('Settings locale rendering', () => {
  beforeEach(() => i18n.applyPreference('system'));

  it.each([
    ['en', 'Settings', 'General', 'Settings groups'],
    ['ja', '設定', '一般', '設定グループ'],
  ] as const)('renders heading, navigation, and title copy in %s', (locale, heading, group, navigationLabel) => {
    i18n.applyPreference(locale);
    render(Settings, { props: { routeParams: { group: 'general' } } });

    expect(screen.getByRole('heading', { name: heading })).toBeTruthy();
    expect(screen.getByRole('heading', { name: group })).toBeTruthy();
    expect(screen.getByRole('navigation', { name: navigationLabel })).toBeTruthy();
  });
});
