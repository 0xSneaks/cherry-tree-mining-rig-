/**
 * The `app-trusted+wallet` server half.
 *
 * Two independent things are proven to Cherry: that this backend vouches for
 * the session (an HS256 token signed with the app secret) and that the caller
 * holds the wallet (an Ed25519 signature over a server-issued nonce). These
 * exercise both against real crypto — no mocks — because the failure mode of
 * getting either wrong is a client minting tokens for wallets it does not own.
 */
import { createHmac, generateKeyPairSync, sign as edSign } from 'node:crypto';
import { beforeEach, afterEach, describe, expect, it } from 'vitest';
import { getAddressDecoder } from '@solana/kit';

import {
  EMBED_TOKEN_TTL_SECONDS,
  EmbedTokenError,
  hs256,
  mintEmbedToken,
  safeEqual,
} from '../src/lib/server/embedToken';
import {
  NONCE_TTL_MS,
  WalletProofError,
  assertNonceValid,
  challengeMessage,
  issueNonce,
  verifySignature,
  verifyWalletProof,
} from '../src/lib/server/walletProof';

const APP_ID = 'ee009264-9c63-47cc-b346-46a44b25ff29';
const SECRET = 'test-app-secret-not-a-real-one-0123456789';

const ORIGINAL = { ...process.env };
beforeEach(() => {
  process.env.CHERRY_APP_SECRET = SECRET;
  process.env.NEXT_PUBLIC_CHERRY_CHAT_APP_ID = APP_ID;
});
afterEach(() => {
  process.env = { ...ORIGINAL };
});

/** A throwaway Solana identity: raw 32-byte key, base58 address, real signing. */
function wallet() {
  const { publicKey, privateKey } = generateKeyPairSync('ed25519');
  const raw = new Uint8Array(
    publicKey.export({ type: 'spki', format: 'der' }).subarray(-32),
  );
  return {
    address: getAddressDecoder().decode(raw) as string,
    sign: (message: Uint8Array) =>
      new Uint8Array(edSign(null, Buffer.from(message), privateKey)),
  };
}

function decode(segment: string): Record<string, unknown> {
  return JSON.parse(Buffer.from(segment, 'base64url').toString('utf8'));
}

describe('HS256', () => {
  it('matches the RFC 7515 A.1 test vector', () => {
    // The exact signing input and key from the RFC, so this checks the
    // implementation rather than checking it against itself.
    const signingInput =
      'eyJ0eXAiOiJKV1QiLA0KICJhbGciOiJIUzI1NiJ9.' +
      'eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFt' +
      'cGxlLmNvbS9pc19yb290Ijp0cnVlfQ';
    const key = Buffer.from(
      'AyM1SysPpbyDfgZld3umj1qzKObwVMkoqQ-EstJQLr_T-1qS0gZH75aKtMN3Yj0i' +
        'PS4hcgUuTwjAzZr1Z9CAow',
      'base64url',
    );
    expect(hs256(signingInput, key)).toBe(
      'dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk',
    );
  });
});

describe('embed token', () => {
  it('carries exactly the claims Cherry specifies', () => {
    const { token } = mintEmbedToken('7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU');
    const [header, payload] = token.split('.') as [string, string, string];

    expect(decode(header)).toEqual({ alg: 'HS256', typ: 'JWT' });

    const claims = decode(payload);
    // `sub` and `app_id` — NOT `walletAddress` / `appId`.
    expect(claims.sub).toBe('7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU');
    expect(claims.app_id).toBe(APP_ID);
    expect(claims).not.toHaveProperty('walletAddress');
    expect(claims).not.toHaveProperty('appId');
    expect(typeof claims.jti).toBe('string');
  });

  it('expires in five minutes', () => {
    const { token, expiresInSeconds } = mintEmbedToken('anyone');
    const claims = decode(token.split('.')[1]!);
    expect(expiresInSeconds).toBe(EMBED_TOKEN_TTL_SECONDS);
    expect(EMBED_TOKEN_TTL_SECONDS).toBe(300);
    expect((claims.exp as number) - (claims.iat as number)).toBe(300);
  });

  it('is signed with the app secret, and a wrong secret does not verify', () => {
    const { token } = mintEmbedToken('anyone');
    const [h, p, s] = token.split('.') as [string, string, string];
    expect(createHmac('sha256', SECRET).update(`${h}.${p}`).digest('base64url')).toBe(s);
    expect(createHmac('sha256', 'wrong').update(`${h}.${p}`).digest('base64url')).not.toBe(s);
  });

  it('gives every token a distinct jti, so one cannot be replayed as another', () => {
    const ids = new Set(
      Array.from({ length: 25 }, () => decode(mintEmbedToken('anyone').token.split('.')[1]!).jti),
    );
    expect(ids.size).toBe(25);
  });

  it('refuses to mint without the app secret rather than signing with nothing', () => {
    delete process.env.CHERRY_APP_SECRET;
    expect(() => mintEmbedToken('anyone')).toThrow(EmbedTokenError);
  });

  it('refuses to mint without the app id', () => {
    delete process.env.NEXT_PUBLIC_CHERRY_CHAT_APP_ID;
    expect(() => mintEmbedToken('anyone')).toThrow(EmbedTokenError);
  });
});

describe('nonce', () => {
  it('round-trips its own nonce', () => {
    const { nonce, expiresInMs } = issueNonce();
    expect(expiresInMs).toBe(NONCE_TTL_MS);
    expect(() => assertNonceValid(nonce)).not.toThrow();
  });

  it('is unique per issue', () => {
    const seen = new Set(Array.from({ length: 50 }, () => issueNonce().nonce));
    expect(seen.size).toBe(50);
  });

  it('rejects a nonce this server did not issue', () => {
    expect(() => assertNonceValid(`${Date.now()}.forged.notavalidmac`)).toThrow(
      /not ours/,
    );
  });

  it('rejects a nonce whose timestamp was edited to extend its life', () => {
    const { nonce } = issueNonce();
    const [, random, mac] = nonce.split('.') as [string, string, string];
    // Same MAC, newer timestamp — the MAC covers the timestamp, so this fails.
    expect(() => assertNonceValid(`${Date.now() + 60_000}.${random}.${mac}`)).toThrow(
      /not ours/,
    );
  });

  it('rejects an expired nonce', () => {
    const { nonce } = issueNonce();
    expect(() => assertNonceValid(nonce, Date.now() + NONCE_TTL_MS + 1_000)).toThrow(
      /expired/,
    );
  });

  it('rejects a nonce minted in the future rather than trusting the clock', () => {
    const { nonce } = issueNonce();
    expect(() => assertNonceValid(nonce, Date.now() - 60_000)).toThrow(/future/);
  });

  it('is signed under a key derived from the app secret, not the secret itself', () => {
    const { nonce } = issueNonce();
    const [issuedAt, random, mac] = nonce.split('.') as [string, string, string];
    const direct = createHmac('sha256', SECRET)
      .update(`${issuedAt}.${random}`)
      .digest('base64url');
    expect(mac).not.toBe(direct);
  });
});

describe('wallet ownership proof', () => {
  it('accepts a genuine signature over the challenge', () => {
    const w = wallet();
    const { nonce, message } = issueNonce();
    const signature = w.sign(new TextEncoder().encode(message));

    expect(
      verifyWalletProof({
        walletAddress: w.address,
        nonce,
        signature: Buffer.from(signature).toString('base64'),
      }),
    ).toBe(w.address);
  });

  it('refuses a token for a wallet the caller does not hold', () => {
    // The whole point: Alice signs, Bob's address is claimed.
    const alice = wallet();
    const bob = wallet();
    const { nonce, message } = issueNonce();
    const signature = alice.sign(new TextEncoder().encode(message));

    expect(() =>
      verifyWalletProof({
        walletAddress: bob.address,
        nonce,
        signature: Buffer.from(signature).toString('base64'),
      }),
    ).toThrow(/does not match/);
  });

  it('refuses a signature over a different nonce', () => {
    const w = wallet();
    const first = issueNonce();
    const second = issueNonce();
    const signature = w.sign(new TextEncoder().encode(first.message));

    expect(() =>
      verifyWalletProof({
        walletAddress: w.address,
        nonce: second.nonce,
        signature: Buffer.from(signature).toString('base64'),
      }),
    ).toThrow(/does not match/);
  });

  it('refuses a bare wallet address with no proof at all', () => {
    const w = wallet();
    expect(() =>
      verifyWalletProof({ walletAddress: w.address, nonce: undefined, signature: undefined }),
    ).toThrow(WalletProofError);
  });

  it('refuses a signature of the wrong length', () => {
    const w = wallet();
    const { nonce } = issueNonce();
    expect(() =>
      verifyWalletProof({
        walletAddress: w.address,
        nonce,
        signature: Buffer.from(new Uint8Array(63)).toString('base64'),
      }),
    ).toThrow(/does not match/);
  });

  it('refuses an address that is not a valid public key', () => {
    const { nonce, message } = issueNonce();
    const w = wallet();
    const signature = w.sign(new TextEncoder().encode(message));
    expect(() =>
      verifyWalletProof({
        walletAddress: 'not-a-base58-address',
        nonce,
        signature: Buffer.from(signature).toString('base64'),
      }),
    ).toThrow(/does not match/);
  });

  it('rejects an off-curve address without throwing', () => {
    // The program ID is a hash, not a curve point — a useful real example.
    expect(
      verifySignature(
        'By7qjU4ZYBYQvdkh41UjnhxWmLyP63S3PbdZXbg1UiKH',
        new TextEncoder().encode('x'),
        new Uint8Array(64),
      ),
    ).toBe(false);
  });

  it('names the app and disclaims on-chain authority in what the user signs', () => {
    const text = challengeMessage('abc');
    expect(text).toContain('THE CHERRY TREE');
    expect(text).toMatch(/does NOT approve a transaction/i);
    expect(text).toContain('abc');
  });
});

describe('safeEqual', () => {
  it('compares equal strings and rejects unequal ones', () => {
    expect(safeEqual('abc', 'abc')).toBe(true);
    expect(safeEqual('abc', 'abd')).toBe(false);
    expect(safeEqual('abc', 'abcd')).toBe(false);
  });
});
