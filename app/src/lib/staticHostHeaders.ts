/**
 * Hash-based CSP for the static export.
 *
 * A static host has no server, so nothing can mint a per-request nonce —
 * but the export is fully built when `scripts/emit-static-headers.ts` runs,
 * and at that point every inline script Next.js baked into the HTML is a
 * fixed byte string. Hashing those strings and shipping the digests in the
 * per-path `_headers` rules admits exactly the scripts the build produced
 * and nothing else, which is the same guarantee a nonce gives a Node
 * deployment, minus the per-request part.
 *
 * The flight-data hash differs per page and per build, so the digests are
 * computed from the exported files themselves at emit time — never cached
 * from an earlier build.
 *
 * Every HTML file the scanner finds gets its own rule, and the `/*` fallback
 * deliberately carries no script allowance: a page the scanner somehow
 * missed must fail closed (its scripts blocked, loudly) rather than fall
 * back to a blanket allowance.
 */
import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { join, relative } from 'node:path';

import { securityHeaders, type ScriptSrcSources } from './csp';

/** An inline `<script>…</script>` body — `src`-carrying scripts are not inline. */
const INLINE_SCRIPT = /<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/g;

/**
 * `'sha256-…'` sources for every inline script in an HTML document. Next.js
 * escapes `</script>` inside script content as `<\/script>`, so the
 * non-greedy match to the first real closing tag is exact.
 */
export function inlineScriptHashSources(html: string): ScriptSrcSources {
  const sources = new Set<string>();
  for (const match of html.matchAll(INLINE_SCRIPT)) {
    sources.add(`'sha256-${createHash('sha256').update(match[1] ?? '', 'utf8').digest('base64')}'`);
  }
  return [...sources];
}

export type StaticHeaderRule = {
  /** URL path the rule matches, already prefixed with the base path. */
  path: string;
  headers: Array<{ key: string; value: string }>;
};

/**
 * One `_headers` rule per exported HTML document. Nested `index.html` maps
 * to both the trailing-slash and bare forms because the export builds with
 * `trailingSlash: true` while a host or a link may request either.
 */
export function staticHeaderRules(outDir: string, basePath: string): StaticHeaderRule[] {
  const htmlFiles = collectHtmlFiles(outDir);
  if (htmlFiles.length === 0) {
    throw new Error(
      `no HTML documents found under ${outDir} — refusing to emit a fallback-only ` +
        '`_headers` file that would block every page\'s scripts. Run the static export first.',
    );
  }

  const rules: StaticHeaderRule[] = [];
  const emitted = new Set<string>();
  for (const file of htmlFiles) {
    const scriptSrc = inlineScriptHashSources(readFileSync(file, 'utf8'));
    if (scriptSrc.length === 0) {
      // Next.js always emits at least its bootstrap script, so zero matches
      // means the extraction broke — not a document with nothing to admit.
      throw new Error(`no inline scripts found in ${file} — check the extraction regex`);
    }
    for (const path of urlPathsFor(relative(outDir, file).split(/[\\/]/))) {
      // The export carries the same document in several guises (404.html,
      // 404/index.html, _not-found/index.html); one rule per URL path.
      if (emitted.has(path)) continue;
      emitted.add(path);
      rules.push({
        path: `${basePath}${path}`,
        headers: securityHeaders({ dev: false, scriptSrc: ["'self'", ...scriptSrc] }),
      });
    }
  }

  // Fail-closed fallback for anything the scanner did not enumerate.
  rules.push({
    path: `${basePath}/*`,
    headers: securityHeaders({ dev: false, scriptSrc: ["'self'"] }),
  });
  return rules;
}

/** URL paths a single exported HTML file can be requested under. */
function urlPathsFor(segments: string[]): string[] {
  const last = segments[segments.length - 1] ?? '';
  if (last === 'index.html') {
    const dir = segments.slice(0, -1).join('/');
    return dir ? [`/${dir}`, `/${dir}/`] : ['/'];
  }
  const base = `/${segments.join('/').replace(/\.html$/, '')}`;
  return [base, `${base}.html`];
}

function collectHtmlFiles(dir: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      // `_next` is the asset tree, not documents.
      if (entry.name === '_next') continue;
      files.push(...collectHtmlFiles(full));
    } else if (entry.name.endsWith('.html')) {
      files.push(full);
    }
  }
  return files;
}

/**
 * Render the rules as a `_headers` body (Cloudflare Pages / Netlify). More
 * specific paths come first: both hosts let the most specific matching rule
 * win per header.
 */
export function formatHeadersFile(rules: StaticHeaderRule[]): string {
  return rules
    .flatMap((rule) => [rule.path, ...rule.headers.map((h) => `  ${h.key}: ${h.value}`), ''])
    .join('\n');
}
