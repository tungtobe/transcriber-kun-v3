// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { installKeymap, keymap, registerKeymap } from './keymap';

afterEach(() => {
  keymap.splice(0, keymap.length);
});

describe('app keymap', () => {
  it('normalizes the literal Space key to the named Space combo', () => {
    const handler = vi.fn();
    const unregister = registerKeymap({ id: 'test-space', combo: 'Space', handler });
    const removeListener = installKeymap(document);

    document.dispatchEvent(new KeyboardEvent('keydown', { key: ' ' }));

    expect(handler).toHaveBeenCalledTimes(1);
    unregister();
    removeListener();
  });
});
