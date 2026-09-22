#!/usr/bin/env node
/*
 * Small dependency-free UI guard. It deliberately checks the source token CSS
 * rather than rendered pixels so CI catches accessibility/elevation regressions
 * before a WebView is involved.
 */
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('..', import.meta.url));
const STYLES_DIR = join(ROOT, 'src', 'styles');
const TOKENS_FILE = join(STYLES_DIR, 'tokens.css');

export const MIN_CONTRAST = 4.5;
export const ALLOWED_SHADOWS = new Set([
  'none',
  '0 1px 2px rgb(17 24 39 / 8%)',
  '0 10px 32px rgb(17 24 39 / 12%)',
]);

export const CONTRAST_PAIRS = {
  light: [
    ['text', 'surface'],
    ['text-secondary', 'surface'],
    ['text-muted', 'surface'],
    ['accent', 'surface'],
    ['accent', 'accent-soft'],
    ['on-primary', 'primary-action'],
    ['danger', 'danger-soft'],
    ['danger-strong', 'danger-soft'],
    ['warning', 'warning-soft'],
    ['info', 'info-soft'],
    ['recover', 'recover-soft'],
    ['memo', 'memo-soft'],
    ['text', 'mark'],
  ],
  dark: [
    ['text', 'bg'],
    ['text-secondary', 'surface'],
    ['text-muted', 'surface'],
    ['accent', 'bg'],
    ['accent', 'accent-soft'],
    ['on-primary', 'primary-action'],
    ['danger', 'danger-soft'],
    ['danger-strong', 'danger-soft'],
    ['warning', 'warning-soft'],
    ['info', 'info-soft'],
    ['recover', 'recover-soft'],
    ['memo', 'memo-soft'],
    ['text', 'mark'],
  ],
};

function stripComments(css) {
  return css.replace(/\/\*[\s\S]*?\*\//g, '');
}

export function parseTokens(css) {
  const tokens = new Map();
  for (const match of stripComments(css).matchAll(/(--[\w-]+)\s*:\s*([^;}]+?)\s*(?:;|(?=}))/g)) {
    tokens.set(match[1], match[2].trim());
  }
  return tokens;
}

function parseColor(value) {
  const normalized = value.trim().toLowerCase();
  if (normalized === 'transparent') {
    return [0, 0, 0, 0];
  }

  const hex = normalized.match(/^#([\da-f]{3}|[\da-f]{6})$/i);
  if (hex) {
    const raw = hex[1].length === 3
      ? hex[1].split('').map((part) => part + part).join('')
      : hex[1];
    return [
      Number.parseInt(raw.slice(0, 2), 16),
      Number.parseInt(raw.slice(2, 4), 16),
      Number.parseInt(raw.slice(4, 6), 16),
      1,
    ];
  }

  const rgb = normalized.match(/^rgba?\((.+)\)$/);
  if (rgb) {
    const parts = rgb[1].replaceAll(',', ' ').split('/').map((part) => part.trim());
    const channels = parts[0].split(/\s+/).map(Number);
    const alpha = parts[1] === undefined
      ? 1
      : parts[1].endsWith('%')
        ? Number.parseFloat(parts[1]) / 100
        : Number.parseFloat(parts[1]);
    if (channels.length === 3 && channels.every(Number.isFinite) && Number.isFinite(alpha)) {
      return [...channels, alpha];
    }
  }

  return null;
}

function blend(foreground, background) {
  const alpha = foreground[3];
  return [
    foreground[0] * alpha + background[0] * (1 - alpha),
    foreground[1] * alpha + background[1] * (1 - alpha),
    foreground[2] * alpha + background[2] * (1 - alpha),
    1,
  ];
}

function luminance(rgb) {
  const linear = rgb.slice(0, 3).map((channel) => {
    const value = channel / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
}

export function contrastRatio(foregroundValue, backgroundValue, baseValue = '#ffffff') {
  const foreground = parseColor(foregroundValue);
  const background = parseColor(backgroundValue);
  const base = parseColor(baseValue);
  if (!foreground || !background || !base) {
    return null;
  }

  const compositedBackground = blend(background, base);
  const compositedForeground = blend(foreground, compositedBackground);
  const foregroundLum = luminance(compositedForeground);
  const backgroundLum = luminance(compositedBackground);
  return (Math.max(foregroundLum, backgroundLum) + 0.05) /
    (Math.min(foregroundLum, backgroundLum) + 0.05);
}

function resolveValue(tokens, name) {
  return tokens.get(`--color-${name}`) ?? null;
}

function checkContrast(tokens) {
  const failures = [];
  for (const [themeName, pairs] of Object.entries(CONTRAST_PAIRS)) {
    const scopedTokens = tokens[themeName];
    const base = resolveValue(scopedTokens, 'bg') ?? (themeName === 'dark' ? '#141517' : '#f6f6f2');
    for (const [foregroundName, backgroundName] of pairs) {
      const foreground = resolveValue(scopedTokens, foregroundName);
      const background = resolveValue(scopedTokens, backgroundName);
      const ratio = foreground && background
        ? contrastRatio(foreground, background, base)
        : null;
      if (ratio === null || ratio < MIN_CONTRAST) {
        failures.push({
          theme: themeName,
          foreground: foregroundName,
          background: backgroundName,
          ratio,
        });
      }
    }
  }
  return failures;
}

export function parseScopedTokens(css) {
  const rootMatch = css.match(/:root\s*,\s*:root\[data-theme=['"]light['"]\]\s*\{([\s\S]*?)\}/);
  const darkMatch = css.match(/:root\[data-theme=['"]dark['"]\]\s*\{([\s\S]*?)\}/);
  const light = parseTokens(rootMatch?.[1] ?? css);
  const dark = parseTokens(darkMatch?.[1] ?? '');
  return { light, dark };
}

export function findShadowViolations(css, file = 'tokens.css') {
  const tokens = parseTokens(css);
  const violations = [];
  for (const [name, value] of tokens) {
    if (name.startsWith('--shadow-') && !ALLOWED_SHADOWS.has(value)) {
      violations.push({ file, property: name, value });
    }
  }
  for (const match of stripComments(css).matchAll(/box-shadow\s*:\s*([^;}]+?)\s*(?:;|(?=}))/g)) {
    const value = match[1].trim();
    if (value.startsWith('var(')) {
      const tokenName = value.match(/^var\((--[\w-]+)\)$/)?.[1];
      const resolved = tokenName ? tokens.get(tokenName) : undefined;
      if (!resolved || !ALLOWED_SHADOWS.has(resolved)) {
        violations.push({ file, property: 'box-shadow', value });
      }
    } else if (!ALLOWED_SHADOWS.has(value)) {
      violations.push({ file, property: 'box-shadow', value });
    }
  }
  return violations;
}

export function checkUiTokens({ tokensCss, styleFiles = [] } = {}) {
  const css = tokensCss ?? readFileSync(TOKENS_FILE, 'utf8');
  const tokens = parseScopedTokens(css);
  const failures = checkContrast(tokens);
  const shadows = [
    ...findShadowViolations(css, relative(ROOT, TOKENS_FILE)),
    ...styleFiles.flatMap(({ file, content }) => findShadowViolations(content, file)),
  ];
  return { contrastFailures: failures, shadowViolations: shadows };
}

const defaultFileSystem = { existsSync, readdirSync, statSync, readFileSync };

export function readStyleFiles(
  directory = join(ROOT, 'src'),
  relativeDirectory = 'src',
  fileSystem = defaultFileSystem,
) {
  if (!fileSystem.existsSync(directory)) {
    return [];
  }
  const files = [];
  for (const entry of fileSystem.readdirSync(directory)) {
    const fullPath = join(directory, entry);
    const relativePath = `${relativeDirectory}/${entry}`;
    if (fileSystem.statSync(fullPath).isDirectory()) {
      files.push(...readStyleFiles(fullPath, relativePath, fileSystem));
    } else if ((entry.endsWith('.css') || entry.endsWith('.svelte')) && relativePath !== 'src/styles/tokens.css') {
      files.push({ file: relativePath, content: fileSystem.readFileSync(fullPath, 'utf8') });
    }
  }
  return files;
}

export const collectStyleFiles = readStyleFiles;

export function runUiChecks() {
  const result = checkUiTokens({ styleFiles: readStyleFiles() });
  if (result.contrastFailures.length || result.shadowViolations.length) {
    console.error('check:ui FAIL');
    for (const failure of result.contrastFailures) {
      console.error(
        `  contrast ${failure.theme}: ${failure.foreground} on ${failure.background} = ${failure.ratio ?? 'unknown'} (minimum ${MIN_CONTRAST})`,
      );
    }
    for (const violation of result.shadowViolations) {
      console.error(`  shadow ${violation.file}: ${violation.property} = ${violation.value}`);
    }
    return 1;
  }
  console.log('check:ui ok — contrast AA and elevation allow-list pass.');
  return 0;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(runUiChecks());
}
