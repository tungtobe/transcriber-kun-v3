#!/usr/bin/env node
// Cổng chặn "dependency cấm" (AR-3, NFR-4): không plugin framework chính thức
// cho shell/fs/updater/process/http/store (Rust lẫn JS), không externalBin
// trong bất kỳ conf Tauri nào. Hàm ở đây thuần (nhận nội dung, trả vi phạm)
// để test bằng fixture không đụng filesystem thật; phần CLI ở cuối file mới
// đọc file và quyết định exit code.

import { readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const PLUGIN_NAMES = ['shell', 'fs', 'updater', 'process', 'http', 'store'];

export const FORBIDDEN_RUST_PLUGINS = PLUGIN_NAMES.map((n) => `tauri-plugin-${n}`);
export const FORBIDDEN_JS_PLUGINS = PLUGIN_NAMES.map((n) => `@tauri-apps/plugin-${n}`);

function escapeRegExp(literal) {
  return literal.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Tìm các tên bị cấm xuất hiện nguyên vẹn trong `content` (không khớp một
 * phần của tên dài hơn, ví dụ "tauri-plugin-shell-extra" không tính).
 */
export function findForbiddenNames(content, names) {
  const found = [];
  for (const name of names) {
    const re = new RegExp(`(?<![\\w@/-])${escapeRegExp(name)}(?![\\w-])`);
    if (re.test(content)) {
      found.push(name);
    }
  }
  return found;
}

/** `true` nếu conf JSON có khoá `externalBin` (sidecar bị cấm — AD-17). */
export function hasExternalBin(content) {
  return /"externalBin"/.test(content);
}

/**
 * `sources`: mảng `{ file, content, kind }` với `kind` là
 * `'rust' | 'js' | 'conf'`. Trả về mảng vi phạm `{ file, name }`.
 */
export function scanSources(sources) {
  const violations = [];
  for (const { file, content, kind } of sources) {
    if (kind === 'rust') {
      for (const name of findForbiddenNames(content, FORBIDDEN_RUST_PLUGINS)) {
        violations.push({ file, name });
      }
    } else if (kind === 'js') {
      for (const name of findForbiddenNames(content, FORBIDDEN_JS_PLUGINS)) {
        violations.push({ file, name });
      }
    } else if (kind === 'conf') {
      if (hasExternalBin(content)) {
        violations.push({ file, name: 'externalBin' });
      }
    }
  }
  return violations;
}

/** Danh sách file được quét, tương đối với gốc repo. */
export const SCANNED_FILES = [
  { file: 'src-tauri/Cargo.toml', kind: 'rust' },
  { file: 'src-tauri/Cargo.lock', kind: 'rust' },
  { file: 'package.json', kind: 'js' },
  { file: 'package-lock.json', kind: 'js' },
  { file: 'src-tauri/tauri.conf.json', kind: 'conf' },
  { file: 'src-tauri/tauri.appstore.conf.json', kind: 'conf' },
  { file: 'src-tauri/tauri.msix.conf.json', kind: 'conf' },
];

/** Đọc các file thật dưới `cwd` (bỏ qua file không tồn tại) rồi quét. */
export function scanRepo(cwd) {
  const sources = [];
  for (const { file, kind } of SCANNED_FILES) {
    const fullPath = join(cwd, file);
    if (!existsSync(fullPath)) {
      continue;
    }
    sources.push({ file, kind, content: readFileSync(fullPath, 'utf8') });
  }
  return scanSources(sources);
}

function main() {
  const violations = scanRepo(process.cwd());
  if (violations.length === 0) {
    console.log('check:deps ok — không thấy dependency cấm hoặc externalBin.');
    process.exit(0);
  }

  console.error('check:deps FAIL — dependency/cấu hình cấm:');
  for (const { file, name } of violations) {
    console.error(`  ${file}: ${name}`);
  }
  process.exit(1);
}

const isMain = process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url;
if (isMain) {
  main();
}
