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

  it('ignores repeated keydown events and does not run shortcuts through a dialog', () => {
    const handler = vi.fn();
    const unregister = registerKeymap({ id: 'live-toggle', combo: 'Meta+Shift+L', handler });
    const removeListener = installKeymap(document);

    document.dispatchEvent(new KeyboardEvent('keydown', {
      key: 'l', metaKey: true, shiftKey: true, repeat: true,
    }));
    expect(handler).not.toHaveBeenCalled();

    const dialog = document.createElement('div');
    dialog.setAttribute('role', 'dialog');
    document.body.append(dialog);
    document.dispatchEvent(new KeyboardEvent('keydown', {
      key: 'l', metaKey: true, shiftKey: true,
    }));
    expect(handler).not.toHaveBeenCalled();

    dialog.remove();
    document.dispatchEvent(new KeyboardEvent('keydown', {
      key: 'l', metaKey: true, shiftKey: true,
    }));
    expect(handler).toHaveBeenCalledTimes(1);
    unregister();
    removeListener();
  });
});
