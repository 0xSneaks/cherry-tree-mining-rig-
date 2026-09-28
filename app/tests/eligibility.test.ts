/**
 * Test 28 (TypeScript half): the product's borrowed-funds disclosure is the
 * engine's `NON_REGISTRY_DEBT_DISCLOSURE`, verbatim, and says honestly what
 * is not detected.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { NON_REGISTRY_DEBT_DISCLOSURE } from '../src/lib/eligibility';

describe('non-registry debt disclosure', () => {
  it('mirrors the Rust engine constant exactly', () => {
    const rs = readFileSync(
      join(import.meta.dirname, '..', '..', 'crates', 'eligibility-engine', 'src', 'lib.rs'),
      'utf8',
    );
    const m = rs.match(/NON_REGISTRY_DEBT_DISCLOSURE: &str = "([^"]+)";/);
    expect(m?.[1]).toBe(NON_REGISTRY_DEBT_DISCLOSURE);
  });

  it('discloses the limitation and never claims universal detection', () => {
    expect(NON_REGISTRY_DEBT_DISCLOSURE).toContain('cannot detect borrowing in other programs');
    expect(NON_REGISTRY_DEBT_DISCLOSURE).toContain('loans from other wallets');
    expect(NON_REGISTRY_DEBT_DISCLOSURE).toContain('off-chain loans');
    expect(NON_REGISTRY_DEBT_DISCLOSURE).toContain('does not prove');
    expect(NON_REGISTRY_DEBT_DISCLOSURE).not.toMatch(/all borrow|any borrow|every loan|guarantee/i);
  });

  it('is rendered in the product', () => {
    const tsx = readFileSync(
      join(import.meta.dirname, '..', 'src', 'components', 'TransparencyPanel.tsx'),
      'utf8',
    );
    expect(tsx).toContain('{NON_REGISTRY_DEBT_DISCLOSURE}');
    expect(tsx).toContain('data-testid="debt-disclosure"');
  });
});
