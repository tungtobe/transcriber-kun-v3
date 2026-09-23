// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { cleanup, render, screen } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import DropOverlay from './DropOverlay.svelte';

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
});

describe('DropOverlay', () => {
  it('renders nothing when inactive', () => {
    render(DropOverlay, { active: false });

    expect(screen.queryByText('Thả file vào đây để bắt đầu transcribe')).toBeNull();
  });

  it('renders the drop-zone message when active', () => {
    render(DropOverlay, { active: true });

    expect(screen.getByText('Thả file vào đây để bắt đầu transcribe')).toBeTruthy();
  });

  it.each([
    ['en', 'Drop the file here to start transcribing'],
    ['ja', 'ここにファイルをドロップして文字起こしを開始'],
  ] as const)('renders the message in %s', (locale, message) => {
    i18n.applyPreference(locale);
    render(DropOverlay, { active: true });

    expect(screen.getByText(message)).toBeTruthy();
  });
});
