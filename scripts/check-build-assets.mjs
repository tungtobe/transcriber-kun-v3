#!/usr/bin/env node
/*
 * Dependency-free post-build guard for the production WebView bundle.
 * Local fonts and same-origin assets are intentional; remote CSS, font, and
 * script references would violate the privacy/CSP boundary of the app.
 */
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('..', import.meta.url));
const DEFAULT_DIST = join(ROOT, 'dist');

const defaultFileSystem = { existsSync, readdirSync, readFileSync, statSync };

export function listBuildFiles(
  directory = DEFAULT_DIST,
  fileSystem = defaultFileSystem,
  relativeDirectory = '',
) {
  if (!fileSystem.existsSync(directory)) {
    return [];
  }

  const files = [];
  for (const entry of fileSystem.readdirSync(directory)) {
    const fullPath = join(directory, entry);
    const relativePath = relativeDirectory ? `${relativeDirectory}/${entry}` : entry;
    if (fileSystem.statSync(fullPath).isDirectory()) {
      files.push(...listBuildFiles(fullPath, fileSystem, relativePath));
    } else {
      files.push({ path: relativePath, content: fileSystem.readFileSync(fullPath, 'utf8') });
    }
  }
  return files;
}

function remoteReferences(file) {
  const extension = file.path.split('.').pop()?.toLowerCase();
  const matches = [];
  const addMatches = (pattern) => {
    for (const match of file.content.matchAll(pattern)) {
      matches.push({ file: file.path, reference: match[0] });
    }
  };

  if (extension === 'html') {
    addMatches(/\b(?:src|href)\s*=\s*["'](?:https?:)?\/\//gi);
  } else if (extension === 'css') {
    addMatches(/@import\s+[^;]*(?:https?:)?\/\//gi);
    addMatches(/url\(\s*["']?(?:https?:)?\/\//gi);
  } else if (extension === 'js' || extension === 'mjs') {
    addMatches(/\b(?:import|importScripts)\s*\(\s*["'](?:https?:)?\/\//gi);
  }

  return matches;
}

export function checkBuildAssets({ distDir = DEFAULT_DIST, fileSystem = defaultFileSystem } = {}) {
  const files = listBuildFiles(distDir, fileSystem);
  const errors = [];
  if (!files.length) {
    errors.push(`missing or empty build directory: ${distDir}`);
  }

  const remote = files.flatMap(remoteReferences);
  for (const reference of remote) {
    errors.push(`remote asset reference in ${reference.file}: ${reference.reference}`);
  }

  if (!files.some(({ path }) => path === 'early-theme.js')) {
    errors.push('missing same-origin early-theme.js asset');
  }

  const fontAssets = files.filter(({ path }) => /\.woff2?$/i.test(path));
  if (!fontAssets.some(({ path }) => /(^|\/)ibm-plex-sans-/i.test(path))) {
    errors.push('missing emitted IBM Plex Sans font assets');
  }
  if (!fontAssets.some(({ path }) => /(^|\/)ibm-plex-mono-/i.test(path))) {
    errors.push('missing emitted IBM Plex Mono font assets');
  }

  return { errors, files, fontAssets, remoteReferences: remote };
}

export function runBuildAssetCheck() {
  const result = checkBuildAssets();
  if (result.errors.length) {
    console.error('check:build-assets FAIL');
    for (const error of result.errors) {
      console.error(`  ${error}`);
    }
    return 1;
  }
  console.log(`check:build-assets ok — ${result.fontAssets.length} local IBM Plex font assets; no remote CSS/font/script references.`);
  return 0;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(runBuildAssetCheck());
}
