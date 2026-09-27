// Render memo Markdown an toàn (story 3.7, spec Boundaries Always): `marked`
// rồi `DOMPurify.sanitize` (không script/style/iframe/sự kiện inline) --
// output của hàm này là HTML sẵn sàng cho `{@html}` (spec I/O Matrix "XSS":
// "body chứa `<script>`/`onerror=` -- không chạy; bị loại khi render").
//
// Chặn click trên link (chỉ mở URL http(s) qua lệnh Rust `open_external_url`)
// là việc của `MemoPanel.svelte` (nghe `click` trên container đã render) --
// module này chỉ đảm bảo HTML không tự chạy được gì và cấp [`isHttpUrl`] để
// nơi đó lọc trước khi gọi lệnh.
import { marked } from 'marked';
import DOMPurify from 'dompurify';

marked.setOptions({ gfm: true, breaks: true });

const SANITIZE_CONFIG = {
  // DOMPurify đã mặc định loại `on*`/`javascript:` -- liệt kê tường minh
  // đúng ba thẻ spec nêu tên để không phụ thuộc hoàn toàn vào mặc định của
  // một bản nâng cấp DOMPurify sau này.
  FORBID_TAGS: ['script', 'style', 'iframe', 'object', 'embed', 'form'],
  FORBID_ATTR: ['style'],
  ALLOW_DATA_ATTR: false,
};

/**
 * Chuyển Markdown memo thành HTML đã sanitize. `marked.parse` chạy đồng bộ
 * (không đăng ký extension async nào) nên ép kiểu `string` là an toàn.
 */
export function renderMemoMarkdown(markdown: string): string {
  const rawHtml = marked.parse(markdown, { async: false }) as string;
  return DOMPurify.sanitize(rawHtml, SANITIZE_CONFIG);
}

/** `true` chỉ cho URL `http`/`https` hợp lệ -- dùng để lọc link trước khi
 * gọi `commands.openExternalUrl` (spec Boundaries Always: "chỉ mở URL
 * `http(s)`"). Một chuỗi không parse được (`javascript:...`, tương đối, rỗng)
 * trả `false`. */
export function isHttpUrl(url: string): boolean {
  try {
    const parsed = new URL(url);
    return parsed.protocol === 'http:' || parsed.protocol === 'https:';
  } catch {
    return false;
  }
}
