// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest';
import { floating, placeFloating } from './floating';

function anchorAt(top: number, bottom: number, right: number): HTMLElement {
  const el = document.createElement('button');
  el.getBoundingClientRect = () =>
    ({ top, bottom, right, left: right - 30, width: 30, height: bottom - top, x: 0, y: top }) as DOMRect;
  return el;
}

function sized(width: number, height: number): HTMLElement {
  const node = document.createElement('div');
  Object.defineProperty(node, 'offsetWidth', { value: width });
  Object.defineProperty(node, 'offsetHeight', { value: height });
  return node;
}

describe('placeFloating', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('opens below the anchor, right-aligned', () => {
    const node = sized(320, 200);
    placeFloating(node, anchorAt(100, 130, 900));
    expect(node.style.position).toBe('fixed');
    expect(node.style.top).toBe('134px');
    expect(node.style.left).toBe('580px');
  });

  it('flips above the anchor when there is no room below', () => {
    const node = sized(320, 200);
    placeFloating(node, anchorAt(window.innerHeight - 40, window.innerHeight - 10, 900));
    expect(node.style.top).toBe(`${window.innerHeight - 40 - 4 - 200}px`);
  });

  it('moves the node to document.body and removes it on destroy', () => {
    const host = document.createElement('div');
    const node = sized(100, 50);
    host.appendChild(node);
    const action = floating(node, anchorAt(0, 20, 200));
    expect(node.parentElement).toBe(document.body);
    action.destroy?.();
    expect(node.isConnected).toBe(false);
  });

  it('is a no-op without an anchor', () => {
    const host = document.createElement('div');
    const node = sized(100, 50);
    host.appendChild(node);
    floating(node, null);
    expect(node.parentElement).toBe(host);
  });
});
