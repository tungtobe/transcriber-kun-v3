// Lọc AND theo tag cho danh sách Home (story 3.2, spec Design Notes): chạy
// phía client trên `libraryStore.sessions` (đã tải hết, <= 500 phiên) --
// tránh round-trip IPC mỗi lần bấm chip. Hàm thuần, tách khỏi
// `library.svelte.ts` để test độc lập.
//
// Story 3.3 (spec-3-3-tim-phien-theo-ten-ket-hop-loc-tag.md): ghép thêm lọc
// theo tên vào cùng hàm này (`SessionFilterState = TagFilterState & { query
// }`) thay vì viết lại logic tag -- AND giữa tag và tên (spec Always: "Query
// + tag là AND"). `query` sống cạnh `tagFilter` trong `libraryStore`, không
// làm thay đổi `TagFilterState`/hành vi lọc tag của 3.2 (spec Never).
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

/** Story 3.3: `TagFilterState` + chuỗi tìm theo tên (chưa chuẩn hoá -- hàm
 * `filterSessions` tự chuẩn hoá qua `normalizeSearchText`). */
export type SessionFilterState = TagFilterState & { query: string };

/** Không lọc gì -- Home hiển thị mọi Phiên. */
export const EMPTY_TAG_FILTER: TagFilterState = { tagIds: [], untagged: false };

/** Story 3.3: giá trị "không lọc gì" cho cả tag lẫn tên. */
export const EMPTY_SESSION_FILTER: SessionFilterState = { ...EMPTY_TAG_FILTER, query: '' };

export function isTagFilterActive(filter: TagFilterState): boolean {
  return filter.untagged || filter.tagIds.length > 0;
}

/**
 * Chuẩn hoá chuỗi để so khớp tìm kiếm (spec Boundaries Always): `NFC`,
 * lowercase không phụ thuộc locale, trim, và gộp mọi khoảng trắng liên tiếp
 * (kể cả tab/newline, qua `\s+`) thành một dấu cách. Dùng cho cả `query` lẫn
 * tên Phiên trước khi so khớp chuỗi con, để "  HỌP   sprint " khớp
 * "Họp sprint 12".
 */
export function normalizeSearchText(value: string): string {
  return value.normalize('NFC').toLowerCase().trim().replace(/\s+/g, ' ');
}

/** Story 3.3: đang có ít nhất một điều kiện lọc (tag hoặc tên) có hiệu lực --
 * dùng để quyết định hiển thị footer "đang lọc" / nút "Xoá bộ lọc". */
export function isSessionFilterActive(filter: SessionFilterState): boolean {
  return isTagFilterActive(filter) || normalizeSearchText(filter.query).length > 0;
}

/**
 * Lọc `sessions` theo `filter` (spec I/O Matrix "Lọc AND", "Chưa gắn tag",
 * "Query + tag"). `untagged` được ưu tiên khi bật (loại trừ tag cụ thể,
 * không phải giao với chúng) -- đúng ngữ nghĩa "chọn A lại tắt Chưa gắn tag"
 * ở phía caller: khi hàm này được gọi, cả hai chưa từng cùng có hiệu lực.
 * Sau đó, nếu `query` chuẩn hoá khác rỗng, thu hẹp tiếp theo tên (AND với
 * kết quả lọc tag ở trên) -- query rỗng sau chuẩn hoá = không lọc tên (spec
 * Always).
 */
export function filterSessions(
  sessions: SessionListItem[],
  filter: SessionFilterState,
): SessionListItem[] {
  let result: SessionListItem[];
  if (filter.untagged) {
    result = sessions.filter((session) => session.tagIds.length === 0);
  } else if (filter.tagIds.length === 0) {
    result = sessions;
  } else {
    result = sessions.filter((session) => filter.tagIds.every((tagId) => session.tagIds.includes(tagId)));
  }

  const query = normalizeSearchText(filter.query);
  if (query.length === 0) {
    return result;
  }
  return result.filter((session) => normalizeSearchText(session.title).includes(query));
}
