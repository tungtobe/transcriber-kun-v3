// Registry phím tắt tập trung (AD-13): chỉ trong app, không global shortcut OS.
// Feature stories đăng ký entry vào đây; `installKeymap` chỉ listens on the
// current WebView document and never touches an OS-wide shortcut API.
export type KeymapEntry = {
  id: string;
  combo: string;
  handler: () => void;
};

export const keymap: KeymapEntry[] = [];

export function registerKeymap(entry: KeymapEntry): () => void {
  const existingIndex = keymap.findIndex((item) => item.id === entry.id);
  if (existingIndex >= 0) {
    keymap.splice(existingIndex, 1, entry);
  } else {
    keymap.push(entry);
  }

  return () => {
    const index = keymap.findIndex((item) => item.id === entry.id);
    if (index >= 0 && keymap[index] === entry) {
      keymap.splice(index, 1);
    }
  };
}

function normalizeCombo(event: KeyboardEvent): string {
  const modifiers = [
    event.metaKey ? 'Meta' : '',
    event.ctrlKey ? 'Control' : '',
    event.altKey ? 'Alt' : '',
    event.shiftKey ? 'Shift' : '',
  ].filter(Boolean);
  const key = event.key === ' ' ? 'Space' : event.key;
  return [...modifiers, key.length === 1 ? key.toUpperCase() : key]
    .join('+');
}

export function installKeymap(target: Document = document): () => void {
  const handler = (event: KeyboardEvent) => {
    const entry = keymap.find((item) => item.combo === normalizeCombo(event));
    if (!entry) {
      return;
    }
    event.preventDefault();
    entry.handler();
  };

  target.addEventListener('keydown', handler);
  return () => target.removeEventListener('keydown', handler);
}
