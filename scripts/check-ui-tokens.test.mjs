import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import {
  checkUiTokens,
  collectStyleFiles,
  contrastRatio,
  findShadowViolations,
} from './check-ui-tokens.mjs';

const root = process.cwd();
const tokensCss = readFileSync(join(root, 'src/styles/tokens.css'), 'utf8');

describe('UI token guard', () => {
  it('passes the light/dark contrast matrix and shadow allow-list', () => {
    const result = checkUiTokens({ tokensCss });
    expect(result.contrastFailures).toEqual([]);
    expect(result.shadowViolations).toEqual([]);
  });

  it('measures WCAG ratios with alpha backgrounds', () => {
    expect(contrastRatio('#171a1f', '#ffffff')).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio('#7a6019', '#eceeef', '#141517')).toBeGreaterThanOrEqual(4.5);
  });

  it('rejects an elevation outside the flat UI allow-list', () => {
    const violations = findShadowViolations(
      ':root { --shadow-card: 0 2px 8px rgb(0 0 0 / 20%); } .card { box-shadow: var(--shadow-card); }',
      'fixture.css',
    );
    expect(violations).toHaveLength(2);

    const semicolonless = findShadowViolations(
      '.card { box-shadow: 0 2px 8px rgb(0 0 0 / 20%) }',
      'semicolonless.css',
    );
    expect(semicolonless).toHaveLength(1);
  });

  it('rejects a contrast regression instead of weakening the threshold', () => {
    const result = checkUiTokens({
      tokensCss: tokensCss.replace('--color-text-muted: #5b6470;', '--color-text-muted: #a0a6b0;'),
    });
    expect(result.contrastFailures.some((failure) => failure.foreground === 'text-muted')).toBe(true);
  });

  it('disables continuous motion for reduced-motion users', () => {
    const globalCss = readFileSync(join(root, 'src/styles/global.css'), 'utf8');
    expect(globalCss).toContain('@media (prefers-reduced-motion: reduce)');
    expect(globalCss).toContain('.status-dot');
    expect(globalCss).toContain('.streaming-caret');
    expect(globalCss).toContain('.panel-motion');
    expect(globalCss).toContain('animation: none !important');
  });

  it('locks the approved geometry and light/dark design colors', () => {
    for (const [token, value] of [
      ['--sidebar-width', '260px'],
      ['--header-height', '64px'],
      ['--window-min-width', '1024px'],
      ['--window-min-height', '680px'],
    ]) {
      expect(tokensCss).toContain(`${token}: ${value};`);
    }

    for (const declaration of [
      '--color-bg: #f6f6f2;',
      '--color-bg-sidebar: #f0f0eb;',
      '--color-surface: #ffffff;',
      '--color-surface-sunken: #ecede8;',
      '--color-border: #e1e4e8;',
      '--color-border-strong: #cfd4da;',
      '--color-text: #171a1f;',
      '--color-text-secondary: #3c4551;',
      '--color-text-muted: #5b6470;',
      '--color-accent: #0f766e;',
      '--color-accent-hover: #0b5d57;',
      '--color-accent-soft: #e6f3f1;',
      '--color-accent-border: #b7ddd8;',
      '--color-danger: #b42318;',
      '--color-warning: #8a4b0a;',
      '--color-info: #1e4e9b;',
      '--color-recover: #5b2fa3;',
      '--color-mark: #fde68a;',
      '--color-bg: #141517;',
      '--color-bg-sidebar: #1a1b1e;',
      '--color-surface: #1f2024;',
      '--color-surface-sunken: #2a2b30;',
      '--color-border: #2e3036;',
      '--color-border-strong: #3c3f47;',
      '--color-text: #ecedef;',
      '--color-text-secondary: #c2c7cf;',
      '--color-text-muted: #a0a6b0;',
      '--color-accent: #2dd4bf;',
      '--color-accent-hover: #5eead4;',
      '--color-accent-soft: rgb(45 212 191 / 14%);',
      '--color-accent-border: rgb(45 212 191 / 38%);',
      '--color-danger: #f87171;',
      '--color-warning: #f5b75c;',
      '--color-info: #7aa7f0;',
      '--color-recover: #c4a6f5;',
      '--color-mark: #7a6019;',
    ]) {
      expect(tokensCss).toContain(declaration);
    }

    const shellCss = readFileSync(join(root, 'src/components/AppShell.svelte'), 'utf8');
    const globalCss = readFileSync(join(root, 'src/styles/global.css'), 'utf8');
    expect(shellCss).toContain('width: var(--sidebar-width);');
    expect(shellCss).toContain('height: var(--header-height);');
    expect(shellCss).toContain('min-width: var(--window-min-width);');
    expect(shellCss).toContain('min-height: var(--window-min-height);');
    expect(globalCss).toContain('outline: 2px solid var(--color-accent);');
    expect(globalCss).toContain('outline-offset: 2px;');
  });

  it('catches dark-only and active accent-soft contrast regressions', () => {
    const darkRegression = checkUiTokens({
      tokensCss: tokensCss.replace('--color-text-muted: #a0a6b0;', '--color-text-muted: #404040;'),
    });
    expect(darkRegression.contrastFailures.some(
      (failure) => failure.theme === 'dark' && failure.foreground === 'text-muted',
    )).toBe(true);

    const activeRegression = checkUiTokens({
      tokensCss: tokensCss.replace('--color-accent-soft: #e6f3f1;', '--color-accent-soft: #0f766e;'),
    });
    expect(activeRegression.contrastFailures.some(
      (failure) => failure.theme === 'light' &&
        failure.foreground === 'accent' && failure.background === 'accent-soft',
    )).toBe(true);
  });

  it('traverses nested component styles through the injectable filesystem seam', () => {
    const directories = new Map([
      ['/fixture/src', ['components', 'root.css']],
      ['/fixture/src/components', ['Button.svelte']],
    ]);
    const contents = new Map([
      ['/fixture/src/root.css', '.root {}'],
      ['/fixture/src/components/Button.svelte', '<style>.button { box-shadow: 0 2px 8px rgb(0 0 0 / 20%) }</style>'],
    ]);
    const fileSystem = {
      existsSync: (path) => directories.has(path) || contents.has(path),
      readdirSync: (path) => directories.get(path) ?? [],
      statSync: (path) => ({ isDirectory: () => directories.has(path) }),
      readFileSync: (path) => contents.get(path),
    };

    const files = collectStyleFiles('/fixture/src', 'src', fileSystem);
    expect(files.map(({ file }) => file)).toEqual([
      'src/components/Button.svelte',
      'src/root.css',
    ]);
    expect(checkUiTokens({ tokensCss, styleFiles: files }).shadowViolations).toEqual([
      {
        file: 'src/components/Button.svelte',
        property: 'box-shadow',
        value: '0 2px 8px rgb(0 0 0 / 20%)',
      },
    ]);
  });

  it('locks onboarding card width and flexible translated controls', () => {
    const onboarding = readFileSync(join(root, 'src/routes/Onboarding.svelte'), 'utf8');
    expect(onboarding).toContain('width: min(100%, 600px);');
    expect(onboarding).toContain('min-width: max-content;');
    expect(onboarding).toContain('min-width: 112px;');
  });
});
