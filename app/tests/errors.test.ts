/**
 * Error translation for the miner screen: expected protocol failures become
 * product language, the raw error survives for the developer console, and
 * nothing else is touched by guesswork.
 */
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import {
  ANCHOR_ERROR_NAMES,
  detectAnchorError,
  explainActionError,
} from '../src/lib/errors';

const root = join(import.meta.dirname, '..', '..');

describe('anchor error names', () => {
  it('match the program error enum, in order', () => {
    const rs = readFileSync(join(root, 'programs', 'sol-rigs', 'src', 'error.rs'), 'utf8');
    const body = rs.slice(rs.indexOf('#[error_code]'));
    const enumBody = body.slice(body.indexOf('{') + 1, body.indexOf('\n}'));
    const variants = enumBody
      .split('\n')
      .map((l) => l.trim())
      .filter((l) => /^[A-Z][A-Za-z]+,?$/.test(l))
      .map((l) => l.replace(',', ''));
    expect(variants).toEqual([...ANCHOR_ERROR_NAMES]);
  });
});

describe('detectAnchorError', () => {
  it('reads the Anchor log line a wallet actually surfaces', () => {
    expect(
      detectAnchorError(
        'AnchorError thrown in programs/sol-rigs/src/error.rs:97. Error Code: AccessDenied. ' +
          'Error Number: 6023. Error Message: Buying is not open to this wallet in the current launch phase.',
      ),
    ).toBe('AccessDenied');
  });

  it('reads the JSON-RPC InstructionError shape', () => {
    expect(detectAnchorError('sendTransaction: err: {"InstructionError":[0,{"Custom":6007}]}')).toBe(
      'NothingToClaim',
    );
  });

  it('resolves the error number when the name is absent', () => {
    expect(detectAnchorError('Error Number: 6011 while claiming')).toBe('ProgramPaused');
    expect(detectAnchorError('Error Number: 6008')).toBe('NothingToCompound');
  });

  it('never translates a number that is not an explicit error number', () => {
    // 6021 could be ReferrerAlreadyBound — or a lamport figure.
    expect(detectAnchorError('costs 6021 lamports, blockhash expired')).toBeNull();
  });

  it('ignores unknown names and out-of-range numbers', () => {
    expect(detectAnchorError('Error Code: SomethingElse')).toBeNull();
    expect(detectAnchorError('Error Number: 6999')).toBeNull();
  });
});

describe('explainActionError', () => {
  it('translates the expected protocol errors into product language', () => {
    const cases: Array<[string, RegExp]> = [
      ['Error Code: AccessDenied. Error Number: 6023.', /not eligible during the current access phase/i],
      ['Error Code: NothingToClaim. Error Number: 6007.', /No harvest is currently available/i],
      ['Error Code: NothingToCompound. Error Number: 6008.', /Not enough accrued rewards to plant another rig yet/i],
      ['Error Code: ProgramPaused. Error Number: 6011.', /temporarily paused/i],
      ['Error Code: GenesisCapExceeded. Error Number: 6025.', /Genesis fresh-buy allowance/i],
    ];
    for (const [text, copy] of cases) {
      const explained = explainActionError(new Error(text));
      expect(explained.message, text).toMatch(copy);
      expect(explained.raw, text).toContain(text);
    }
  });

  it('passes unknown errors through untouched rather than inventing copy', () => {
    const explained = explainActionError(new Error('blockhash not found'));
    expect(explained.message).toBe('Error: blockhash not found');
  });

  it('stringifies non-Error throwables', () => {
    expect(explainActionError({ weird: true }).raw).toBe('[object Object]');
  });
});

describe('client safety', () => {
  it('defaults to devnet, never mainnet', async () => {
    delete process.env.NEXT_PUBLIC_SOLANA_RPC_URL;
    delete process.env.NEXT_PUBLIC_SOLANA_CLUSTER;
    const env = await import('../src/lib/env');
    expect(env.RPC_URL).toBe('https://api.devnet.solana.com');
    expect(env.CLUSTER).toBe('devnet');
  });

  it('ships no keypair or secret references in client-reachable code', () => {
    const clientDirs = ['src/components', 'src/hooks', 'src/lib/program', 'src/lib/chain.ts'];
    const files: string[] = [];
    const walk = (p: string) => {
      if (statSync(p).isDirectory()) for (const e of readdirSync(p)) walk(join(p, e));
      else if (p.endsWith('.ts') || p.endsWith('.tsx')) files.push(p);
    };
    for (const d of clientDirs) walk(join(root, 'app', d));
    expect(files.length).toBeGreaterThan(10);
    for (const f of files) {
      expect(f, readFileSync(f, 'utf8')).not.toMatch(
        /KEYPAIR|ANCHOR_WALLET|CHERRY_APP_SECRET|seed phrase|private key/i,
      );
    }
  });
});
