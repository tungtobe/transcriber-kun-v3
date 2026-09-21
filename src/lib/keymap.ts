// Registry phím tắt tập trung (AD-13): chỉ trong app, không global shortcut OS.
// Story 1.1 chỉ dựng khung; các epic sau đăng ký entry vào đây.
export type KeymapEntry = {
  id: string;
  combo: string;
  handler: () => void;
};

export const keymap: KeymapEntry[] = [];
