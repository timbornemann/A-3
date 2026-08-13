import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const tokensCss = readFileSync(resolve(process.cwd(), 'src/design-tokens.css'), 'utf8');
const componentCss = readFileSync(resolve(process.cwd(), 'src/styles.css'), 'utf8');
const scopedComponentCss = readScopedComponentCss(resolve(process.cwd(), 'src'));

const REQUIRED_FOUNDATION_TOKENS = [
  '--font-sans',
  '--font-display',
  '--font-size-body',
  '--line-height-body',
  '--space-1',
  '--space-7',
  '--radius-control',
  '--radius-card',
  '--control-min-size',
  '--focus-width',
  '--focus-offset',
] as const;

const REQUIRED_THEME_TOKENS = [
  '--color-canvas',
  '--color-text',
  '--color-muted',
  '--color-accent-text',
  '--color-on-accent',
  '--color-surface-raised',
  '--color-positive',
  '--color-info',
  '--color-neutral',
  '--color-warning',
  '--color-danger',
  '--color-hypothesis',
  '--color-focus',
  '--color-code',
  '--color-on-code',
] as const;

const CONTRAST_PAIRS = [
  ['--color-text', '--color-canvas'],
  ['--color-muted', '--color-canvas'],
  ['--color-subtle', '--color-canvas'],
  ['--color-accent-text', '--color-surface-raised'],
  ['--color-on-accent', '--color-accent-strong'],
  ['--color-on-accent', '--color-info'],
  ['--color-on-accent', '--color-warning'],
  ['--color-positive', '--color-positive-surface'],
  ['--color-info', '--color-info-surface'],
  ['--color-neutral', '--color-neutral-surface'],
  ['--color-warning', '--color-warning-surface'],
  ['--color-danger', '--color-danger-surface'],
  ['--color-hypothesis', '--color-hypothesis-surface'],
] as const;

describe('U9 design tokens', () => {
  it('defines the required color, typography, spacing, control, and focus contracts', () => {
    const light = selectorTokens(":root,\n:root[data-theme='light']");
    const dark = selectorTokens(":root[data-theme='dark']");

    for (const token of REQUIRED_FOUNDATION_TOKENS) {
      expect(light.has(token), `light is missing ${token}`).toBe(true);
    }
    for (const token of REQUIRED_THEME_TOKENS) {
      expect(light.has(token), `light is missing ${token}`).toBe(true);
      expect(dark.has(token), `dark is missing ${token}`).toBe(true);
    }
    expect(light.get('--radius-control')).toBe('0.25rem');
    expect(light.get('--radius-panel')).toBe('0.35rem');
    expect(light.get('--radius-card')).toBe('0.5rem');
    expect(componentCss).toMatch(/^@import '\.\/design-tokens\.css';/u);
  });

  it.each([
    ['light', ":root,\n:root[data-theme='light']"],
    ['dark', ":root[data-theme='dark']"],
  ])('%s semantic text pairs meet WCAG AA normal-text contrast', (_theme, selector) => {
    const tokens = selectorTokens(selector);

    for (const [foregroundToken, backgroundToken] of CONTRAST_PAIRS) {
      const foreground = requiredHex(tokens, foregroundToken);
      const background = requiredHex(tokens, backgroundToken);
      expect(
        contrastRatio(foreground, background),
        `${foregroundToken} on ${backgroundToken}`,
      ).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('keeps raw component colors out of the component stylesheet', () => {
    for (const [source, css] of [['styles.css', componentCss], ...scopedComponentCss] as const) {
      expect(css, `${source} contains a raw hex color`).not.toMatch(/#[0-9a-f]{3,8}\b/iu);
      expect(css, `${source} contains a raw rgb color`).not.toMatch(/\brgba?\(/iu);
      expect(css, `${source} contains a raw named color`).not.toMatch(
        /(?:^|[;{])\s*(?:color|background(?:-color)?):\s*(?:black|white)\b/imu,
      );
    }
  });

  it('keeps text scalable and suppresses non-essential motion through the user preference', () => {
    const allComponentCss = [componentCss, ...scopedComponentCss.map(([, css]) => css)].join('\n');
    expect(allComponentCss).not.toMatch(/font-size:\s*[^;{}]*px\b/iu);
    expect(componentCss).toContain('@media (prefers-reduced-motion: reduce)');
    expect(componentCss).toContain('scroll-behavior: auto !important');
    expect(componentCss).toContain('animation-duration: 0.01ms !important');
    expect(componentCss).toContain('animation-iteration-count: 1 !important');
    expect(componentCss).toContain('transition-duration: 0.01ms !important');
  });

  it('keeps dense desktop status and settings layouts bounded inside the workspace pane', () => {
    expect(componentCss).toMatch(
      /\.global-status dl > div\s*\{[^}]*grid-template-columns:\s*auto auto minmax\(0, 1fr\)/u,
    );
    expect(componentCss).toMatch(
      /\.workspace-content\s*\{[^}]*overflow-x:\s*hidden[^}]*overflow-y:\s*auto/u,
    );
    expect(componentCss).toMatch(
      /@media \(width <= 1100px\)\s*\{[\s\S]*?\.model-profile-grid,[\s\S]*?\.project-policy-grid,[\s\S]*?\.privacy-settings\s*\{[^}]*grid-template-columns:\s*minmax\(0, 1fr\)/u,
    );
  });
});

function readScopedComponentCss(directory: string): Array<readonly [string, string]> {
  const sources: Array<readonly [string, string]> = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) {
      sources.push(...readScopedComponentCss(path));
      continue;
    }
    if (!entry.isFile() || !entry.name.endsWith('.svelte')) continue;
    const source = readFileSync(path, 'utf8');
    for (const match of source.matchAll(/<style(?:\s[^>]*)?>(?<css>[\s\S]*?)<\/style>/gu)) {
      sources.push([path, match.groups?.css ?? '']);
    }
  }
  return sources;
}

function selectorTokens(selector: string): Map<string, string> {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/gu, '\\$&');
  const block = new RegExp(`${escaped}\\s*\\{(?<body>[^}]*)\\}`, 'u').exec(tokensCss)?.groups?.body;
  if (block === undefined) throw new Error(`missing token selector ${selector}`);
  return new Map(
    [...block.matchAll(/(?<name>--[a-z0-9-]+):\s*(?<value>[^;]+);/gu)].map((match) => [
      match.groups?.name ?? '',
      match.groups?.value.trim() ?? '',
    ]),
  );
}

function requiredHex(tokens: Map<string, string>, name: string): string {
  const value = tokens.get(name);
  if (value === undefined || !/^#[0-9a-f]{6}$/iu.test(value)) {
    throw new Error(`${name} must be a six-digit hex color, received ${String(value)}`);
  }
  return value;
}

function contrastRatio(foreground: string, background: string): number {
  const foregroundLuminance = relativeLuminance(foreground);
  const backgroundLuminance = relativeLuminance(background);
  const lighter = Math.max(foregroundLuminance, backgroundLuminance);
  const darker = Math.min(foregroundLuminance, backgroundLuminance);
  return (lighter + 0.05) / (darker + 0.05);
}

function relativeLuminance(color: string): number {
  const channels = [color.slice(1, 3), color.slice(3, 5), color.slice(5, 7)].map(
    (channel) => Number.parseInt(channel, 16) / 255,
  );
  const [red = 0, green = 0, blue = 0] = channels.map((channel) =>
    channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4,
  );
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}
