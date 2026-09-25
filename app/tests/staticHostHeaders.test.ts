/**
 * The static export's `_headers` writer. No server exists there, so the
 * policy admits Next's inline scripts by sha-256 digest computed from the
 * exported HTML — these pin that mapping: digests that actually match the
 * document, one rule per URL path, and a fallback that admits nothing.
 */
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';

import { afterEach, describe, expect, it } from 'vitest';

import {
  formatHeadersFile,
  inlineScriptHashSources,
  staticHeaderRules,
} from '../src/lib/staticHostHeaders';

/** The two inline scripts Next.js bakes into every document. */
const PAGE = `<!doctype html><html><head>
<script>(self.__next_f=self.__next_f||[]).push([0])</script>
<script>self.__next_f.push([1,"1:\\"$Sreact.fragment\\"\\n2:I[39756]"])</script>
</head><body><script src="/_next/static/chunks/x.js"></script></body></html>`;

const sha256 = (text: string) =>
  `'sha256-${createHash('sha256').update(text, 'utf8').digest('base64')}'`;

describe('inline script hashing', () => {
  it('hashes every inline script and skips src scripts', () => {
    const bootstrap = '(self.__next_f=self.__next_f||[]).push([0])';
    const flight = 'self.__next_f.push([1,"1:\\"$Sreact.fragment\\"\\n2:I[39756]"])';
    expect(inlineScriptHashSources(PAGE)).toEqual([sha256(bootstrap), sha256(flight)]);
  });

  it('is exact about bytes — a one-character change is a different digest', () => {
    expect(inlineScriptHashSources(PAGE.replace('push', 'pushh'))).not.toEqual(
      inlineScriptHashSources(PAGE),
    );
  });

  it('does not stop at an escaped closing tag inside the flight data', () => {
    const html = '<script>self.__next_f.push([1,"a<\\/script>b"])</script><p>after</p>';
    const body = 'self.__next_f.push([1,"a<\\/script>b"])';
    expect(inlineScriptHashSources(html)).toEqual([sha256(body)]);
  });
});

describe('static header rules', () => {
  const dirs: string[] = [];
  afterEach(() => {
    for (const dir of dirs.splice(0)) {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  function exportTree(files: Record<string, string>): string {
    const out = mkdtempSync(join(tmpdir(), 'cherry-headers-'));
    dirs.push(out);
    for (const [name, body] of Object.entries(files)) {
      mkdirSync(join(out, dirname(name)), { recursive: true });
      writeFileSync(join(out, name), body);
    }
    return out;
  }

  it('maps each document to its URL paths, with and without the slash', () => {
    const out = exportTree({
      'index.html': PAGE,
      'result/index.html': PAGE,
    });
    const paths = staticHeaderRules(out, '').map((r) => r.path);
    expect(paths).toEqual(['/', '/result', '/result/', '/*']);
  });

  it('maps a bare html file to both its clean and .html paths', () => {
    const out = exportTree({ '404.html': PAGE });
    expect(staticHeaderRules(out, '').map((r) => r.path)).toEqual([
      '/404',
      '/404.html',
      '/*',
    ]);
  });

  it('dedupes the export\u2019s repeated copies of the same document', () => {
    const out = exportTree({
      '404.html': PAGE,
      '404/index.html': PAGE,
      '_not-found/index.html': PAGE,
    });
    const paths = staticHeaderRules(out, '').map((r) => r.path);
    expect(new Set(paths).size).toBe(paths.length);
  });

  it('prefixes every path with the Pages base path', () => {
    const out = exportTree({ 'index.html': PAGE });
    expect(staticHeaderRules(out, '/cherry-tree-mining-rig-').map((r) => r.path)).toEqual([
      '/cherry-tree-mining-rig-/',
      '/cherry-tree-mining-rig-/*',
    ]);
  });

  it('admits exactly the document\u2019s scripts, and nothing for unknown paths', () => {
    const out = exportTree({ 'index.html': PAGE });
    const [root, fallback] = staticHeaderRules(out, '');
    const rootCsp = root!.headers.find((h) => h.key === 'Content-Security-Policy')!.value;
    expect(rootCsp).toContain(
      `script-src 'self' ${inlineScriptHashSources(PAGE).join(' ')}`,
    );

    const fallbackCsp = fallback!.headers.find((h) => h.key === 'Content-Security-Policy')!.value;
    const fallbackScriptSrc = fallbackCsp.match(/script-src ([^;]*)/)?.[1] ?? '';
    expect(fallbackScriptSrc).toBe("'self'");
  });

  it('fails closed when there is nothing to hash — an empty export is a broken build', () => {
    const out = exportTree({ 'not-html.txt': 'x' });
    expect(() => staticHeaderRules(out, '')).toThrow(/no HTML documents/i);
  });

  it('fails closed on a document whose scripts the scanner cannot see', () => {
    const out = exportTree({ 'index.html': '<!doctype html><p>no scripts</p>' });
    expect(() => staticHeaderRules(out, '')).toThrow(/no inline scripts/i);
  });
});

describe('_headers formatting', () => {
  it('renders path rules with two-space-indented header lines', () => {
    const body = formatHeadersFile([
      {
        path: '/',
        headers: [
          { key: 'A', value: '1' },
          { key: 'B', value: '2' },
        ],
      },
      { path: '/*', headers: [{ key: 'A', value: '3' }] },
    ]);
    expect(body).toBe('/\n  A: 1\n  B: 2\n\n/*\n  A: 3\n');
  });
});
