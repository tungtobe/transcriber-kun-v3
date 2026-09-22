// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { cleanup, render, screen } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import Home from './Home.svelte';

afterEach(() => cleanup());

describe('Home locale rendering', () => {
  beforeEach(() => i18n.applyPreference('system'));

  it.each([
    ['en', 'Home', 'Drop a file here'],
    ['ja', 'ホーム', 'ここにファイルをドロップ'],
  ] as const)('renders representative copy in %s', (locale, heading, fileTitle) => {
    i18n.applyPreference(locale);
    render(Home);

    expect(screen.getByRole('heading', { name: heading })).toBeTruthy();
    expect(screen.getByRole('heading', { name: fileTitle })).toBeTruthy();
  });
});
