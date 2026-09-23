/**
 * The devnet rehearsal recognises refusals by Anchor error code. Those codes
 * are positional (6000 + index in `error.rs`), so a reordered enum would make
 * the rehearsal accept the wrong refusal. Pin them to the source.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = join(import.meta.dirname, '..', '..');

describe('devnet rehearsal error codes', () => {
  it('match the program error enum', () => {
    const rs = readFileSync(join(root, 'programs', 'sol-rigs', 'src', 'error.rs'), 'utf8');
    const body = rs.slice(rs.indexOf('#[error_code]'));
    const enumBody = body.slice(body.indexOf('{') + 1, body.indexOf('\n}'));
    const variants = enumBody
      .split('\n')
      .map((l) => l.trim())
      .filter((l) => /^[A-Z][A-Za-z]+,?$/.test(l))
      .map((l) => l.replace(',', ''));
    const code = (name: string) => 6000 + variants.indexOf(name);

    const ts = readFileSync(join(root, 'app', 'scripts', 'ops', 'devnet-rehearsal.ts'), 'utf8');
    for (const name of ['ReferrerAlreadyBound', 'AccessDenied']) {
      expect(variants).toContain(name);
      const m = ts.match(new RegExp(`${name}: (\\d+)`));
      expect(Number(m?.[1]), name).toBe(code(name));
    }
  });

  it('is devnet-guarded and never loads the mainnet founder', () => {
    const ts = readFileSync(join(root, 'app', 'scripts', 'ops', 'devnet-rehearsal.ts'), 'utf8');
    expect(ts).toContain('requireDevnet()');
    expect(ts).toContain('assertConnectedToDevnet(chain)');
    expect(ts).not.toMatch(/EXPECTED_FOUNDER|FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp/);
  });
});
