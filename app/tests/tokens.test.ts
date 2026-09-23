/**
 * Visual-system drift guards.
 *
 * THE CHERRY TREE has one palette, defined once in `globals.css` `:root`.
 * These fail when a colour is restated outside it — a stray hex in a rule, an
 * rgba copy of the brand pink, a hardcoded colour in a component — or when the
 * surfaces that cannot read CSS variables (the chat theme seeds, the browser
 * theme-color) stop matching the tokens they mirror.
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import { CHERRY_CHAT_THEME } from '../src/lib/cherryChat';

const css = readFileSync(new URL('../src/app/globals.css', import.meta.url), 'utf8');
const stripComments = (t: string) => t.replace(/\/\*[\s\S]*?\*\//g, ' ');

const rootBlock = stripComments(css).match(/:root\s*\{([\s\S]*?)\n\}/)?.[1] ?? '';
const outsideRoot = stripComments(css).replace(/:root\s*\{[\s\S]*?\n\}/, '');

function token(name: string): string {
  const m = rootBlock.match(new RegExp(`--${name}:\\s*(#[0-9a-fA-F]{3,8});`));
  if (!m?.[1]) throw new Error(`token --${name} not found in :root`);
  return m[1].toLowerCase();
}

function hue(hex: string): { h: number; s: number; l: number } {
  const n = hex.replace('#', '');
  const [r, g, b] = [0, 2, 4].map((i) => Number.parseInt(n.slice(i, i + 2), 16) / 255) as [
    number,
    number,
    number,
  ];
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  const d = max - min;
  let h = 0;
  if (d) {
    if (max === r) h = ((g - b) / d) % 6;
    else if (max === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
    h = (h * 60 + 360) % 360;
  }
  const s = d === 0 ? 0 : d / (1 - Math.abs(2 * l - 1));
  return { h, s, l };
}

function walk(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    return statSync(p).isDirectory() ? walk(p) : [p];
  });
}

describe('one palette, defined once', () => {
  it('has no hex or rgb/hsl literal outside :root', () => {
    expect(outsideRoot).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
    expect(outsideRoot).not.toMatch(/\b(rgba?|hsla?)\(/);
  });

  it('derives every blossom state from --blossom rather than restating it', () => {
    for (const derived of ['blossom-wash', 'blossom-wash-strong', 'blossom-glow']) {
      expect(rootBlock).toMatch(
        new RegExp(`--${derived}:\\s*color-mix\\(in srgb, var\\(--blossom\\) \\d+%, transparent\\);`),
      );
    }
  });

  it('keeps the canonical blossom where the product shipped it', () => {
    expect(token('blossom')).toBe('#ff5fa2');
  });

  it('keeps components free of colour literals — they use tokens', () => {
    const src = fileURLToPath(new URL('../src', import.meta.url));
    const offenders = walk(src)
      .filter((f) => /\.tsx$/.test(f))
      .filter((f) => /#[0-9a-fA-F]{6}\b|\brgba?\(/.test(stripComments(readFileSync(f, 'utf8'))))
      .map((f) => f.slice(src.length + 1))
      // Next's viewport themeColor cannot read a CSS variable; guarded below.
      .filter((f) => f !== join('app', 'layout.tsx'));
    expect(offenders).toEqual([]);
  });
});

describe('surfaces that cannot read CSS variables mirror the tokens', () => {
  it('browser theme-color is --bg', () => {
    const layout = readFileSync(new URL('../src/app/layout.tsx', import.meta.url), 'utf8');
    expect(layout.match(/themeColor:\s*'(#[0-9a-fA-F]{6})'/)?.[1]?.toLowerCase()).toBe(token('bg'));
  });

  it('chat seeds are exactly --blossom, --bg and --blossom-soft', () => {
    expect(CHERRY_CHAT_THEME.primaryColor.toLowerCase()).toBe(token('blossom'));
    expect(CHERRY_CHAT_THEME.backgroundColor.toLowerCase()).toBe(token('bg'));
    expect(CHERRY_CHAT_THEME.accentColor.toLowerCase()).toBe(token('blossom-soft'));
  });
});

describe('the brand reads as navy + blossom, not purple or grey', () => {
  it('blossom tokens are pink (hue 320–345), not magenta-purple', () => {
    for (const name of ['blossom', 'blossom-soft', 'blossom-dim']) {
      const { h } = hue(token(name));
      expect(h, name).toBeGreaterThanOrEqual(320);
      expect(h, name).toBeLessThanOrEqual(345);
    }
  });

  it('every surface is dark navy (hue 215–235), never grey or purple', () => {
    for (const name of ['bg', 'bg-panel', 'bg-inset', 'bg-raised']) {
      const { h, s, l } = hue(token(name));
      expect(h, name).toBeGreaterThanOrEqual(215);
      expect(h, name).toBeLessThanOrEqual(235);
      expect(s, `${name} saturation`).toBeGreaterThan(0.2);
      expect(l, `${name} lightness`).toBeLessThan(0.16);
    }
  });
});
