/**
 * Terminology guard. THE CHERRY TREE plants SEEDS; the on-chain program,
 * its accounts and the TypeScript program types keep their `rig` names for
 * interface compatibility. User-facing copy must never say "rig", "rigs" or
 * "hashpower".
 *
 * What is scanned: JSX text, the user-facing props (label, unit, aria-label,
 * placeholder, title, alt, caption) and every string / template literal that
 * contains a space (i.e. reads as prose) in components and pages. Code
 * identifiers, object keys and CSS classes are not copy and are not scanned.
 *
 * To approve an exception, add the exact phrase to APPROVED with a reason.
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const BANNED = /\brigs?\b|hashpower/i;
const APPROVED: Record<string, string> = {};

const root = join(import.meta.dirname, '..', 'src');
function walk(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    return statSync(p).isDirectory() ? walk(p) : [p];
  });
}
const files = [
  ...walk(join(root, 'components')),
  ...walk(join(root, 'app')).filter((f) => f.endsWith('.tsx')),
  join(root, 'lib', 'brand.ts'),
];

function copyOf(source: string): string[] {
  const code = source.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/(^|[^:])\/\/.*$/gm, '$1');
  const out: string[] = [];
  // JSX text between tags.
  for (const m of code.matchAll(/>([^<>{}]*[A-Za-z][^<>{}]*)</g)) out.push(m[1]!.trim());
  // User-facing props.
  for (const m of code.matchAll(
    /\b(?:label|unit|aria-label|placeholder|title|alt|caption)\s*[=:]\s*(?:\{\s*)?(['"`])([^'"`]*)\1/g,
  )) {
    out.push(m[2]!);
  }
  // Prose-like literals: single-line, containing a space, not code, not a
  // CSS class list.
  for (const line of code.split('\n')) {
    for (const m of line.matchAll(/'([^'\n]*)'|"([^"\n]*)"|`([^`\n]*)`/g)) {
      const t = m[1] ?? m[2] ?? m[3] ?? '';
      if (!/\s/.test(t) || /[;{}()|]|--|^\s*[:,]/.test(t.replace(/\$\{[^}]*\}/g, ''))) continue;
      if (/className=/.test(line) && !/[A-Z]/.test(t)) continue;
      out.push(t);
    }
  }
  return out.filter((t) => t.length > 0);
}

describe('user-facing terminology', () => {
  it('scans the UI', () => {
    expect(files.length).toBeGreaterThan(10);
  });

  it('never says rig, rigs or hashpower', () => {
    const offenders: string[] = [];
    for (const f of files) {
      for (const text of copyOf(readFileSync(f, 'utf8'))) {
        if (BANNED.test(text) && !(text in APPROVED)) offenders.push(`${f.slice(root.length + 1)}: ${text}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it('keeps Claim explicit on the claim action', () => {
    const game = readFileSync(join(root, 'components', 'Game.tsx'), 'utf8');
    expect(game).toContain("'Claim SOL'");
    expect(game).toMatch(/Replant/);
  });
});

describe('founder privacy', () => {
  it('never names the founder in the UI — only the wallet is used, internally', () => {
    const offenders: string[] = [];
    for (const f of files) {
      if (/sneaks/i.test(readFileSync(f, 'utf8'))) offenders.push(f.slice(root.length + 1));
    }
    expect(offenders).toEqual([]);
  });
});
