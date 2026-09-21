import { describe, expect, it, afterEach } from 'vitest';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { scanSources, scanRepo } from './check-forbidden-deps.mjs';

const SCRIPT_PATH = join(dirname(fileURLToPath(import.meta.url)), 'check-forbidden-deps.mjs');

// Bốn dòng của I/O & Edge-Case Matrix (spec 1.1):
//   1. Gọi version            -- không liên quan file này.
//   2. Binding lệch           -- không liên quan file này.
//   3. Dependency cấm         -- covered below (Cargo + npm + conf).
//   4. Manifest sạch          -- covered below.

describe('scanSources (hàm thuần)', () => {
  it('Dependency cấm: Cargo.toml chứa tauri-plugin-http bị bắt', () => {
    const violations = scanSources([
      {
        file: 'src-tauri/Cargo.toml',
        kind: 'rust',
        content: '[dependencies]\ntauri-plugin-http = "2"\n',
      },
    ]);
    expect(violations).toEqual([{ file: 'src-tauri/Cargo.toml', name: 'tauri-plugin-http' }]);
  });

  it('Dependency cấm: package.json chứa @tauri-apps/plugin-fs bị bắt', () => {
    const violations = scanSources([
      {
        file: 'package.json',
        kind: 'js',
        content: JSON.stringify({ dependencies: { '@tauri-apps/plugin-fs': '2.0.0' } }),
      },
    ]);
    expect(violations).toEqual([{ file: 'package.json', name: '@tauri-apps/plugin-fs' }]);
  });

  it('Dependency cấm: conf có externalBin bị bắt', () => {
    const violations = scanSources([
      {
        file: 'src-tauri/tauri.conf.json',
        kind: 'conf',
        content: JSON.stringify({ bundle: { externalBin: ['./sidecar'] } }),
      },
    ]);
    expect(violations).toEqual([{ file: 'src-tauri/tauri.conf.json', name: 'externalBin' }]);
  });

  it('Manifest sạch: không tên cấm nào -> mảng rỗng', () => {
    const violations = scanSources([
      {
        file: 'src-tauri/Cargo.toml',
        kind: 'rust',
        content: '[dependencies]\ntauri = "=2.11.5"\ntauri-specta = "=2.0.0-rc.25"\n',
      },
      {
        file: 'package.json',
        kind: 'js',
        content: JSON.stringify({ dependencies: { '@tauri-apps/api': '2.11.1' } }),
      },
      {
        file: 'src-tauri/tauri.conf.json',
        kind: 'conf',
        content: JSON.stringify({ productName: 'trans-kun' }),
      },
    ]);
    expect(violations).toEqual([]);
  });

  it('không khớp một phần tên dài hơn (không false-positive)', () => {
    const violations = scanSources([
      {
        file: 'src-tauri/Cargo.toml',
        kind: 'rust',
        content: '# tauri-plugin-shell-community-fork = "0.1"\nmy-tauri-plugin-shell-wrapper = "0.1"\n',
      },
    ]);
    expect(violations).toEqual([]);
  });
});

describe('scanRepo (đọc file thật qua fixture tạm)', () => {
  let dir;

  afterEach(() => {
    if (dir) {
      rmSync(dir, { recursive: true, force: true });
      dir = undefined;
    }
  });

  it('Dependency cấm: phát hiện qua Cargo.lock + báo đúng file', () => {
    dir = mkdtempSync(join(tmpdir(), 'check-forbidden-deps-'));
    mkdirSync(join(dir, 'src-tauri'), { recursive: true });
    writeFileSync(
      join(dir, 'src-tauri', 'Cargo.lock'),
      '[[package]]\nname = "tauri-plugin-store"\nversion = "2.0.0"\n',
    );

    const violations = scanRepo(dir);

    expect(violations).toEqual([
      { file: 'src-tauri/Cargo.lock', name: 'tauri-plugin-store' },
    ]);
  });

  it('Manifest sạch: repo fixture của story 1.1 không có vi phạm', () => {
    dir = mkdtempSync(join(tmpdir(), 'check-forbidden-deps-'));
    mkdirSync(join(dir, 'src-tauri'), { recursive: true });
    writeFileSync(
      join(dir, 'src-tauri', 'Cargo.toml'),
      '[dependencies]\ntauri = { version = "=2.11.5", features = ["specta"] }\n',
    );
    writeFileSync(
      join(dir, 'package.json'),
      JSON.stringify({ dependencies: { '@tauri-apps/api': '2.11.1' } }),
    );
    writeFileSync(join(dir, 'src-tauri', 'tauri.conf.json'), JSON.stringify({ productName: 'trans-kun' }));

    expect(scanRepo(dir)).toEqual([]);
  });

  it('bỏ qua yên lặng các file không tồn tại (Cargo.lock chưa sinh)', () => {
    dir = mkdtempSync(join(tmpdir(), 'check-forbidden-deps-'));
    expect(scanRepo(dir)).toEqual([]);
  });
});

describe('CLI (subprocess thật — quyết định exit code mà CI dựa vào)', () => {
  let dir;

  afterEach(() => {
    if (dir) {
      rmSync(dir, { recursive: true, force: true });
      dir = undefined;
    }
  });

  it('Manifest sạch: exit 0', () => {
    dir = mkdtempSync(join(tmpdir(), 'check-forbidden-deps-cli-'));
    mkdirSync(join(dir, 'src-tauri'), { recursive: true });
    writeFileSync(
      join(dir, 'src-tauri', 'Cargo.toml'),
      '[dependencies]\ntauri = { version = "=2.11.5", features = ["specta"] }\n',
    );
    writeFileSync(
      join(dir, 'package.json'),
      JSON.stringify({ dependencies: { '@tauri-apps/api': '2.11.1' } }),
    );
    writeFileSync(join(dir, 'src-tauri', 'tauri.conf.json'), JSON.stringify({ productName: 'trans-kun' }));

    const result = spawnSync(process.execPath, [SCRIPT_PATH], { cwd: dir, encoding: 'utf8' });

    expect(result.status).toBe(0);
  });

  it('Dependency cấm: exit 1 và in đúng file + tên vi phạm', () => {
    dir = mkdtempSync(join(tmpdir(), 'check-forbidden-deps-cli-'));
    mkdirSync(join(dir, 'src-tauri'), { recursive: true });
    writeFileSync(
      join(dir, 'src-tauri', 'Cargo.toml'),
      '[dependencies]\ntauri-plugin-http = "2"\n',
    );

    const result = spawnSync(process.execPath, [SCRIPT_PATH], { cwd: dir, encoding: 'utf8' });

    expect(result.status).toBe(1);
    expect(result.stderr).toContain('src-tauri/Cargo.toml');
    expect(result.stderr).toContain('tauri-plugin-http');
  });
});
