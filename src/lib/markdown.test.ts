// @vitest-environment jsdom
// DOMPurify needs a real `window`/DOM (auto-detected from the global scope)
// to sanitize -- the project's default vitest environment is `node`.
import { describe, expect, it } from 'vitest';
import { isHttpUrl, renderMemoMarkdown } from './markdown';

describe('renderMemoMarkdown', () => {
  it('renders headings, lists and emphasis to HTML', () => {
    const html = renderMemoMarkdown('# Tiêu đề\n\n- một\n- hai\n\n**đậm**');
    expect(html).toContain('<h1>Tiêu đề</h1>');
    expect(html).toContain('<li>một</li>');
    expect(html).toContain('<strong>đậm</strong>');
  });

  // Spec I/O Matrix "XSS": "body chứa `<script>`/`onerror=` -- không chạy;
  // bị loại khi render".
  it('strips a raw script tag', () => {
    const html = renderMemoMarkdown('hello <script>alert(1)</script> world');
    expect(html).not.toContain('<script');
    expect(html).not.toContain('alert(1)');
  });

  it('strips an inline event handler attribute from an image tag', () => {
    const html = renderMemoMarkdown('<img src="x" onerror="alert(1)">');
    expect(html).not.toContain('onerror');
  });

  it('strips an iframe and a style tag', () => {
    const html = renderMemoMarkdown('<iframe src="https://evil.example"></iframe><style>body{}</style>');
    expect(html).not.toContain('<iframe');
    expect(html).not.toContain('<style');
  });

  it('neutralizes a javascript: link href', () => {
    const html = renderMemoMarkdown('[click me](javascript:alert(1))');
    expect(html).not.toContain('javascript:');
  });

  it('keeps a plain https link href intact', () => {
    const html = renderMemoMarkdown('[site](https://example.com/path)');
    expect(html).toContain('href="https://example.com/path"');
  });
});

describe('isHttpUrl', () => {
  it('accepts http and https URLs', () => {
    expect(isHttpUrl('https://example.com')).toBe(true);
    expect(isHttpUrl('http://example.com/a?b=1')).toBe(true);
  });

  it('rejects non-http schemes and unparsable strings', () => {
    expect(isHttpUrl('javascript:alert(1)')).toBe(false);
    expect(isHttpUrl('file:///etc/passwd')).toBe(false);
    expect(isHttpUrl('not a url')).toBe(false);
    expect(isHttpUrl('')).toBe(false);
  });
});
