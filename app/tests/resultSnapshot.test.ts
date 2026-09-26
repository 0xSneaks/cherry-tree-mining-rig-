/**
 * Signed blink snapshot integrity (Mainnet blocker #3, regression of
 * `1e25925`).
 *
 * Inside a Cherry inline/preview render the host's snapshot is the only
 * source for the result card; the URL query — attacker-controlled in a shared
 * link — must never fill, override, or select a field. The query is read only
 * on a page positively classified as standalone.
 */
import { afterEach, describe, expect, it } from 'vitest';

import type { ResultParams } from '../src/components/ResultCard';
import {
  RESULT_FIELDS,
  classifyResultEnvironment,
  readResultEnvironment,
  resolveSignedSnapshot,
  resolveSignedTokenParams,
  resolveStandaloneQuery,
  type ResultField,
  type TrustedCherryContext,
} from '../src/lib/resultSnapshot';

/**
 * One valid value per field. Typed as a total record so adding a field to
 * `ResultParams` without a fixture here fails to compile; the runtime check
 * below fails if the two lists ever disagree.
 */
const VALID: Record<ResultField, string> = {
  kind: 'progress',
  rigs: '77',
  compounded: '19',
  claimed: '250000000',
  rooted: 'yes',
  milestone: 'First Bloom',
  ref: 'abc123',
};
const FORGED: Record<ResultField, string> = {
  kind: 'claim',
  rigs: '999999',
  compounded: '888888',
  claimed: '777777',
  rooted: 'no',
  milestone: 'Forged',
  ref: 'attacker',
};

function trusted(mode: 'inline' | 'preview'): TrustedCherryContext {
  const c = classifyResultEnvironment({
    search: `?mode=${mode}&token=t`,
    hash: '',
    cherryEmbedded: true,
  });
  if (c.context === null) throw new Error('expected a trusted context');
  return c.context;
}

/** What `/result` renders inside Cherry, given a host snapshot and a URL. */
function renderTrusted(mode: 'inline' | 'preview', snapshot: unknown, url: string): ResultParams {
  const c = classifyResultEnvironment({ search: url, hash: '', cherryEmbedded: true });
  expect(c.env).toBe(mode);
  if (c.context === null) throw new Error('expected a trusted context');
  return resolveSignedSnapshot(c.context, snapshot);
}

const forgedQuery = (base: string) =>
  `${base}&${RESULT_FIELDS.map((k) => `${k}=${encodeURIComponent(FORGED[k])}`).join('&')}`;

describe('field inventory', () => {
  it('covers every ResultParams field exactly once', () => {
    expect([...RESULT_FIELDS].sort()).toStrictEqual(Object.keys(VALID).sort());
    expect(new Set(RESULT_FIELDS).size).toBe(RESULT_FIELDS.length);
    expect(RESULT_FIELDS).toStrictEqual([
      'kind',
      'rigs',
      'compounded',
      'claimed',
      'rooted',
      'milestone',
      'ref',
    ]);
  });
});

describe('classifyResultEnvironment', () => {
  const cases: Array<[string, { search: string; hash?: string; cherryEmbedded: boolean }, string]> = [
    ['trusted inline', { search: '?mode=inline&token=t', cherryEmbedded: true }, 'inline'],
    ['trusted preview', { search: '?mode=preview&token=t', cherryEmbedded: true }, 'preview'],
    ['trusted inline, mode in hash', { search: '', hash: '#mode=inline&token=t', cherryEmbedded: true }, 'inline'],
    ['trusted preview, mode in hash', { search: '', hash: '#mode=preview', cherryEmbedded: true }, 'preview'],
    ['standalone, no params', { search: '', cherryEmbedded: false }, 'standalone'],
    ['standalone with result fields', { search: '?kind=compound&rigs=54', cherryEmbedded: false }, 'standalone'],
    // Query-only or user-controlled signals resembling Cherry.
    ['query-only mode=inline outside Cherry', { search: '?mode=inline&rigs=1', cherryEmbedded: false }, 'unknown'],
    ['query-only mode=preview outside Cherry', { search: '?mode=preview', cherryEmbedded: false }, 'unknown'],
    ['hash-only mode=inline outside Cherry', { search: '', hash: '#mode=inline', cherryEmbedded: false }, 'unknown'],
    ['bare token outside Cherry', { search: '?token=t&rigs=1', cherryEmbedded: false }, 'unknown'],
    ['token in hash outside Cherry', { search: '', hash: '#token=t', cherryEmbedded: false }, 'unknown'],
    ['cherry_embed marker, strict detection off', { search: '?cherry_embed=0', cherryEmbedded: false }, 'unknown'],
    // Missing environment signal inside Cherry.
    ['Cherry detected, no mode', { search: '', cherryEmbedded: true }, 'unknown'],
    ['Cherry detected, token, no mode', { search: '?token=t', cherryEmbedded: true }, 'unknown'],
    // Malformed signals.
    ['empty mode', { search: '?mode=', cherryEmbedded: true }, 'unknown'],
    ['uppercase mode', { search: '?mode=INLINE', cherryEmbedded: true }, 'unknown'],
    ['padded mode', { search: '?mode=%20inline', cherryEmbedded: true }, 'unknown'],
    ['unrecognized mode', { search: '?mode=embedded', cherryEmbedded: true }, 'unknown'],
    ['standalone-looking mode', { search: '?mode=standalone', cherryEmbedded: false }, 'unknown'],
    // Conflicting signals.
    ['duplicate identical mode', { search: '?mode=inline&mode=inline', cherryEmbedded: true }, 'unknown'],
    ['conflicting modes in search', { search: '?mode=preview&mode=inline', cherryEmbedded: true }, 'unknown'],
    ['unknown mode first, inline second', { search: '?mode=x&mode=inline', cherryEmbedded: true }, 'unknown'],
    ['search vs hash conflict', { search: '?mode=inline', hash: '#mode=preview', cherryEmbedded: true }, 'unknown'],
    ['search and hash agree but repeat', { search: '?mode=inline', hash: '#mode=inline', cherryEmbedded: true }, 'unknown'],
  ];

  for (const [name, input, expected] of cases) {
    it(`${name} → ${expected}`, () => {
      const c = classifyResultEnvironment({ hash: '', ...input });
      expect(c.env).toBe(expected);
      if (expected === 'inline' || expected === 'preview') {
        expect(c.context).not.toBeNull();
        expect(c.context?.mode).toBe(expected);
      } else {
        expect(c.context).toBeNull();
      }
    });
  }

  it('an unparseable hash is unknown', () => {
    const c = classifyResultEnvironment({
      search: '',
      hash: { replace: () => { throw new Error('bad'); } } as unknown as string,
      cherryEmbedded: false,
    });
    expect(c.env).toBe('unknown');
  });

  it('a non-boolean embed signal is never trusted and never standalone', () => {
    for (const cherryEmbedded of [undefined, null, 'true', 1] as unknown as boolean[]) {
      expect(classifyResultEnvironment({ search: '?mode=inline', hash: '', cherryEmbedded }).env).toBe('unknown');
      expect(classifyResultEnvironment({ search: '', hash: '', cherryEmbedded }).env).toBe('unknown');
    }
  });
});

describe('readResultEnvironment (browser adapter)', () => {
  type FakeWindow = {
    location: { search: string; hash: string };
    parent?: unknown;
    __cherry?: boolean;
    ReactNativeWebView?: unknown;
  };
  const g = globalThis as unknown as { window?: FakeWindow };
  afterEach(() => delete g.window);
  const set = (w: FakeWindow) => {
    w.parent ??= w;
    g.window = w;
  };

  it('is unknown with no window (server render)', () => {
    expect(readResultEnvironment().env).toBe('unknown');
  });

  it('trusts the Cherry WebView with an inline mode', () => {
    set({ location: { search: '?mode=inline&token=t', hash: '' }, __cherry: true });
    expect(readResultEnvironment().env).toBe('inline');
  });

  it('trusts the Cherry iframe marker with a preview mode', () => {
    set({ location: { search: '?mode=preview&cherry_embed=1', hash: '' } });
    expect(readResultEnvironment().env).toBe('preview');
  });

  it('does not trust a non-Cherry React Native WebView claiming inline', () => {
    set({ location: { search: '?mode=inline', hash: '' }, ReactNativeWebView: { postMessage() {} } });
    expect(readResultEnvironment().env).toBe('unknown');
  });

  it('is standalone in a plain browser with result fields', () => {
    set({ location: { search: '?kind=compound&rigs=54', hash: '' } });
    expect(readResultEnvironment().env).toBe('standalone');
  });

  it('is unknown when reading the location throws', () => {
    g.window = {
      get location(): never {
        throw new Error('denied');
      },
    } as unknown as FakeWindow;
    expect(readResultEnvironment().env).toBe('unknown');
  });
});

describe.each(['inline', 'preview'] as const)('trusted %s: the signed snapshot is authoritative', (mode) => {
  const base = `?mode=${mode}&token=t`;

  for (const field of RESULT_FIELDS) {
    describe(field, () => {
      it('uses a present signed value', () => {
        const out = renderTrusted(mode, { [field]: VALID[field] }, base);
        expect(out).toStrictEqual({ [field]: VALID[field] });
      });

      it('a query value cannot override it', () => {
        const out = renderTrusted(mode, { [field]: VALID[field] }, forgedQuery(base));
        expect(out[field]).toBe(VALID[field]);
      });

      it('a query value cannot fill it when the snapshot omits it', () => {
        const others = Object.fromEntries(RESULT_FIELDS.filter((k) => k !== field).map((k) => [k, VALID[k]]));
        const out = renderTrusted(mode, others, forgedQuery(base));
        expect(Object.hasOwn(out, field)).toBe(false);
        expect(out[field]).toBeUndefined();
        expect(Object.keys(out).sort()).toStrictEqual(Object.keys(others).sort());
      });

      it('stays absent when omitted and the snapshot is empty', () => {
        const out = renderTrusted(mode, {}, forgedQuery(base));
        expect(out).toStrictEqual({});
        expect(Object.hasOwn(out, field)).toBe(false);
      });

      it('keeps a signed empty string, and the query cannot replace it', () => {
        const out = renderTrusted(mode, { [field]: '' }, forgedQuery(base));
        expect(Object.hasOwn(out, field)).toBe(true);
        expect(out[field]).toBe('');
      });

      it('stringifies a signed number, as before', () => {
        const out = renderTrusted(mode, { [field]: 42 }, base);
        expect(out).toStrictEqual({ [field]: '42' });
      });

      it('omits malformed signed values without falling back to the query', () => {
        for (const bad of [null, undefined, true, false, {}, [], ['77'], { v: '77' }, () => '77', 1n, Symbol('x')]) {
          const out = renderTrusted(mode, { [field]: bad }, forgedQuery(base));
          expect(Object.hasOwn(out, field)).toBe(false);
        }
      });

      it('ignores an inherited value', () => {
        const snapshot = Object.create({ [field]: VALID[field] }) as object;
        const out = renderTrusted(mode, snapshot, forgedQuery(base));
        expect(Object.hasOwn(out, field)).toBe(false);
      });

      it('repeated or conflicting query values cannot alter it', () => {
        const url = `${base}&${field}=${encodeURIComponent(FORGED[field])}&${field}=${encodeURIComponent(FORGED[field])}x&${field}=`;
        expect(renderTrusted(mode, { [field]: VALID[field] }, url)).toStrictEqual({ [field]: VALID[field] });
        expect(renderTrusted(mode, {}, url)).toStrictEqual({});
      });
    });
  }

  it('drops unknown signed and query fields', () => {
    const out = renderTrusted(mode, { ...VALID, admin: 'yes', __proto__: { rigs: '1' } }, `${base}&admin=yes&extra=1`);
    expect(out).toStrictEqual(VALID);
  });

  it('a non-object snapshot yields nothing, not the query', () => {
    for (const snapshot of [null, undefined, 'rigs=77', 77, [VALID], true]) {
      expect(renderTrusted(mode, snapshot, forgedQuery(base))).toStrictEqual({});
    }
  });

  it('a full snapshot survives a fully forged query', () => {
    expect(renderTrusted(mode, VALID, forgedQuery(base))).toStrictEqual(VALID);
  });
});

describe('resolveSignedSnapshot guard', () => {
  it('refuses to run without a classifier-minted context', () => {
    for (const ctx of [null, undefined, {}, { mode: 'standalone' }, { mode: 'INLINE' }]) {
      expect(() => resolveSignedSnapshot(ctx as unknown as TrustedCherryContext, VALID)).toThrow();
    }
  });

  it('takes no query input', () => {
    expect(resolveSignedSnapshot.length).toBe(2);
    expect(resolveSignedSnapshot(trusted('inline'), VALID)).toStrictEqual(VALID);
  });
});

describe('resolveSignedTokenParams (server /inline)', () => {
  it('maps every field from verified token params', () => {
    expect(resolveSignedTokenParams(VALID)).toStrictEqual(VALID);
  });

  it('leaves omitted fields absent and keeps signed empty values', () => {
    expect(resolveSignedTokenParams({ rigs: '', kind: 'claim' })).toStrictEqual({ rigs: '', kind: 'claim' });
  });

  it('omits malformed values and non-object params', () => {
    expect(resolveSignedTokenParams({ rigs: { n: 1 }, claimed: null, ref: ['x'] })).toStrictEqual({});
    expect(resolveSignedTokenParams(null)).toStrictEqual({});
    expect(resolveSignedTokenParams(undefined)).toStrictEqual({});
  });
});

describe('standalone: the query path', () => {
  const standalone = (search: string) => {
    const c = classifyResultEnvironment({ search, hash: '', cherryEmbedded: false });
    expect(c.env).toBe('standalone');
    return resolveStandaloneQuery('standalone', new URLSearchParams(search));
  };

  for (const field of RESULT_FIELDS) {
    it(`${field}: a single query value is used`, () => {
      expect(standalone(`?${field}=${encodeURIComponent(VALID[field])}`)).toStrictEqual({ [field]: VALID[field] });
    });

    it(`${field}: a repeated query value is ambiguous and omitted`, () => {
      const out = standalone(`?${field}=${encodeURIComponent(VALID[field])}&${field}=other`);
      expect(Object.hasOwn(out, field)).toBe(false);
    });
  }

  it('passes values through for ResultCard to validate, and ignores unknown keys', () => {
    expect(standalone('?rigs=<script>&compounded=NaN&admin=1')).toStrictEqual({
      rigs: '<script>',
      compounded: 'NaN',
    });
  });

  it('keeps the existing standalone card URL working', () => {
    expect(standalone('?kind=compound&rigs=54&compounded=12')).toStrictEqual({
      kind: 'compound',
      rigs: '54',
      compounded: '12',
    });
  });

  it('cannot be entered from a non-standalone classification', () => {
    for (const env of ['inline', 'preview', 'unknown', undefined]) {
      expect(() => resolveStandaloneQuery(env as 'standalone', new URLSearchParams('?rigs=1'))).toThrow();
    }
    // @ts-expect-error — only the literal 'standalone' type-checks.
    expect(() => resolveStandaloneQuery('inline', new URLSearchParams())).toThrow();
  });

  it('standalone cannot be selected by spoofing a Cherry signal', () => {
    for (const search of ['?mode=inline', '?token=t', '?cherry_embed=1', '?mode=standalone']) {
      expect(classifyResultEnvironment({ search, hash: '', cherryEmbedded: false }).env).toBe('unknown');
    }
  });
});

describe('regression: the pre-fix `?? sp.get` merge', () => {
  // The expression `1e25925` removed and the hardening base had reintroduced.
  const legacyMerge = (params: Record<string, unknown> | undefined, sp: URLSearchParams) => {
    const str = (v: unknown) => (typeof v === 'string' ? v : typeof v === 'number' ? String(v) : undefined);
    return Object.fromEntries(RESULT_FIELDS.map((k) => [k, str(params?.[k]) ?? sp.get(k) ?? undefined]));
  };

  it('the old merge filled omitted fields from the query; the fix does not', () => {
    const snapshot = { kind: 'compound', rigs: '77' };
    const url = forgedQuery('?mode=inline&token=t');

    const before = legacyMerge(snapshot, new URLSearchParams(url));
    expect(before.compounded).toBe(FORGED.compounded);
    expect(before.claimed).toBe(FORGED.claimed);

    expect(renderTrusted('inline', snapshot, url)).toStrictEqual(snapshot);
  });
});
