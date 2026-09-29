// @vitest-environment jsdom
// Hồi quy: `MemoPanel` gọi `memoStore.load()` trong `$effect`; `load()` đọc rồi
// ghi `entries` đồng bộ, nên nếu không `untrack` thì effect tự chạy lại vô hạn
// (`effect_update_depth_exceeded`) và làm treo điều hướng ở màn Session.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import MemoPanel from './MemoPanel.svelte';

const pending = () => new Promise(() => {});

vi.mock('../lib/bindings', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../lib/bindings')>();
  return { ...actual, commands: new Proxy({}, { get: () => pending }) };
});
vi.mock('../lib/stores/memoTemplates.svelte', () => ({
  memoTemplatesStore: {
    templates: [
      { id: 't1', name: 'T', prompt: '', isDefault: true, locale: 'vi', defaultKey: 'k' },
    ],
    load: vi.fn().mockResolvedValue(undefined),
  },
}));

afterEach(() => cleanup());

describe('MemoPanel với memoStore thật', () => {
  it('mount không rơi vào vòng lặp effect vô hạn', () => {
    const errors: unknown[] = [];
    const onError = (e: ErrorEvent) => errors.push(e.error ?? e.message);
    window.addEventListener('error', onError);
    expect(() => {
      render(MemoPanel, {
        sessionId: 's1',
        transcriptId: 'tr1',
        sourceVariant: 'primary',
        active: true,
        hasTranscript: true,
        segmentTextCount: 3,
      });
      flushSync();
    }).not.toThrow();
    window.removeEventListener('error', onError);
    expect(errors.map(String).join('\n')).not.toMatch(/effect_update_depth_exceeded/);
  });
});
