// Thin wrapper around `@tauri-apps/api/webview`'s native drag-drop event
// (story 2.8 Code Map: "bọc trong module riêng để mock trong Vitest").
// Tauri v2 windows default to `dragDropEnabled: true`, so the browser's own
// HTML5 `drop` event never carries a real filesystem path — the webview's
// own `onDragDropEvent` is the only source of absolute paths for files
// dropped from the OS (spec Design Notes).
import { getCurrentWebview } from '@tauri-apps/api/webview';

export type DragDropPayload =
  | { type: 'enter'; paths: string[]; position: { x: number; y: number } }
  | { type: 'over'; position: { x: number; y: number } }
  | { type: 'drop'; paths: string[]; position: { x: number; y: number } }
  | { type: 'leave' };

export type DragDropEvent = { payload: DragDropPayload };

export type UnlistenFn = () => void;

/** Registers the webview-wide drag-drop listener. Returns the `unlisten`
 * function `onDragDropEvent` resolves with — callers unregister it on
 * unmount (spec Code Map). */
export function onDragDropEvent(
  handler: (event: DragDropEvent) => void,
): Promise<UnlistenFn> {
  return getCurrentWebview().onDragDropEvent(handler);
}
