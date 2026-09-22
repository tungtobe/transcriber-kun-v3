import { describe, expect, it } from 'vitest';
import { checkBuildAssets } from './check-build-assets.mjs';

function fixtureFileSystem(contents, assetEntries = [
  'ibm-plex-sans-latin.woff2',
  'ibm-plex-mono-latin.woff2',
  'index.css',
  'index.js',
]) {
  const directories = new Map([
    ['/fixture/dist', ['index.html', 'early-theme.js', 'assets']],
    ['/fixture/dist/assets', assetEntries],
  ]);
  return {
    existsSync: (path) => directories.has(path) || contents.has(path),
    readdirSync: (path) => directories.get(path) ?? [],
    statSync: (path) => ({ isDirectory: () => directories.has(path) }),
    readFileSync: (path) => contents.get(path) ?? '',
  };
}

describe('production build asset guard', () => {
  it('accepts same-origin scripts, CSS, and emitted IBM Plex fonts', () => {
    const contents = new Map([
      ['/fixture/dist/index.html', '<script src="/early-theme.js"></script>'],
      ['/fixture/dist/early-theme.js', 'document.documentElement.dataset.theme = "light";'],
      ['/fixture/dist/assets/ibm-plex-sans-latin.woff2', 'font'],
      ['/fixture/dist/assets/ibm-plex-mono-latin.woff2', 'font'],
      ['/fixture/dist/assets/index.css', '@font-face { src: url(/assets/ibm-plex-sans-latin.woff2); }'],
      ['/fixture/dist/assets/index.js', 'import("/assets/chunk.js");'],
    ]);

    expect(checkBuildAssets({ distDir: '/fixture/dist', fileSystem: fixtureFileSystem(contents) }).errors)
      .toEqual([]);
  });

  it('rejects remote asset references and missing local font families', () => {
    const contents = new Map([
      ['/fixture/dist/index.html', '<script src="https://cdn.example.test/app.js"></script>'],
      ['/fixture/dist/early-theme.js', ''],
      ['/fixture/dist/assets/index.css', '@import url("https://fonts.example.test/font.css");'],
      ['/fixture/dist/assets/index.js', 'import("https://cdn.example.test/chunk.js");'],
    ]);

    const result = checkBuildAssets({
      distDir: '/fixture/dist',
      fileSystem: fixtureFileSystem(contents, ['index.css', 'index.js']),
    });
    expect(result.errors).toEqual(expect.arrayContaining([
      expect.stringContaining('remote asset reference'),
      'missing emitted IBM Plex Sans font assets',
      'missing emitted IBM Plex Mono font assets',
    ]));
  });
});
