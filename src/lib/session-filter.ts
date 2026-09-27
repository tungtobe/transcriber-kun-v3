// Lọc AND theo tag cho danh sách Home (story 3.2, spec Design Notes): chạy
// phía client trên `libraryStore.sessions` (đã tải hết, <= 500 phiên) --
// tránh round-trip IPC mỗi lần bấm chip. Hàm thuần, tách khỏi
// `library.svelte.ts` để test độc lập và để story 3.3 ghép thêm lọc theo tên
// vào cùng một hàm (spec: "được đặt tên sao cho story 3.3 thêm query tên vào
// cùng hàm lọc") mà không phải viết lại logic tag.
import type { SessionListItem } from './bindings';

export interface TagFilterState {
  /** Mọi tag phải khớp (AND) -- spec I/O Matrix "Lọc AND": chọn A, B chỉ ra
   * Phiên có cả A và B. */
  tagIds: string[];
  /** `true` = chỉ Phiên không có tag nào -- loại trừ `tagIds` khi bật (spec
   * I/O Matrix "Chưa gắn tag"). Caller (`libraryStore`) đảm bảo hai trường
   * này không bao giờ cùng "có hiệu lực" (bật `untagged` xoá `tagIds` và
   * ngược lại) -- hàm này chỉ đọc, không tự áp đặt tính loại trừ đó. */
  untagged: boolean;
}

/** Không lọc gì -- Home hiển thị mọi Phiên. */
export const EMPTY_TAG_FILTER: TagFilterState = { tagIds: [], untagged: false };

export function isTagFilterActive(filter: TagFilterState): boolean {
  return filter.untagged || filter.tagIds.length > 0;
}

/**
 * Lọc `sessions` theo `filter` (spec I/O Matrix "Lọc AND", "Chưa gắn tag").
 * `untagged` được ưu tiên khi bật (loại trừ tag cụ thể, không phải giao với
 * chúng) -- đúng ngữ nghĩa "chọn A lại tắt Chưa gắn tag" ở phía caller: khi
 * hàm này được gọi, cả hai chưa từng cùng có hiệu lực.
 */
export function filterSessions(
  sessions: SessionListItem[],
  filter: TagFilterState,
): SessionListItem[] {
  if (filter.untagged) {
    return sessions.filter((session) => session.tagIds.length === 0);
  }
  if (filter.tagIds.length === 0) {
    return sessions;
  }
  return sessions.filter((session) => filter.tagIds.every((tagId) => session.tagIds.includes(tagId)));
}
