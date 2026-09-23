import 'server-only';

/**
 * Cherry Chat embed tokens — the `app-trusted+wallet` half that Cherry trusts.
 *
 * Two things are proven to Cherry on every mount: that *this backend* vouches
 * for the session (this file, an HS256 JWT signed with the app secret), and
 * that the user *holds the wallet* (`walletProof.ts`, an Ed25519 signature
 * over a server-issued nonce). Cherry checks the second itself through
 * `signChallengeHandler`; we check it too, because the token is what carries
 * `sub` and a token minted for a wallet the caller does not hold would let
 * them inherit that wallet's room access.
 *
 * The app secret is an HMAC key and never leaves the server. It is read from
 * `CHERRY_APP_SECRET`, is never prefixed `NEXT_PUBLIC_`, and is never
 * returned, logged, or embedded in a response.
 *
 * HS256 is HMAC-SHA256 over `base64url(header).base64url(payload)`, so this is
 * `node:crypto` and no dependency. `tests/embedToken.test.ts` checks it
 * against the RFC 7515 A.1 vector.
 */
import { createHmac, randomUUID, timingSafeEqual } from 'node:crypto';

/** Seconds a minted token stays valid. Cherry's contract says ~5 minutes. */
export const EMBED_TOKEN_TTL_SECONDS = 300;

export class EmbedTokenError extends Error {}

function base64url(input: string | Uint8Array): string {
  return Buffer.from(input).toString('base64url');
}

/** The Cherry app secret, or a refusal. Never returned to a caller. */
export function appSecret(): string {
  const secret = process.env.CHERRY_APP_SECRET;
  if (!secret) {
    throw new EmbedTokenError(
      'CHERRY_APP_SECRET is not set; refusing to mint an embed token',
    );
  }
  return secret;
}

/** The embed's app id. Public — it is also in the client bundle. */
export function embedAppId(): string {
  const id = process.env.NEXT_PUBLIC_CHERRY_CHAT_APP_ID;
  if (!id) {
    throw new EmbedTokenError(
      'NEXT_PUBLIC_CHERRY_CHAT_APP_ID is not set; refusing to mint an embed token',
    );
  }
  return id;
}

/** Sign `header.payload` with HMAC-SHA256. Exported for the RFC vector test. */
export function hs256(signingInput: string, secret: string | Uint8Array): string {
  return createHmac('sha256', secret).update(signingInput).digest('base64url');
}

/**
 * Mint a Cherry embed token.
 *
 * Claims are exactly what Cherry's contract specifies — `sub` and `app_id`,
 * not `walletAddress`/`appId` — plus the standard `exp`/`iat`/`jti`. `jti` is
 * what makes a captured token single-use on Cherry's side.
 *
 * `walletAddress` must already be proven; this function does not check it,
 * and its only caller does.
 */
export function mintEmbedToken(walletAddress: string): {
  token: string;
  expiresInSeconds: number;
} {
  const now = Math.floor(Date.now() / 1000);
  const header = base64url(JSON.stringify({ alg: 'HS256', typ: 'JWT' }));
  const payload = base64url(
    JSON.stringify({
      sub: walletAddress,
      app_id: embedAppId(),
      iat: now,
      exp: now + EMBED_TOKEN_TTL_SECONDS,
      jti: randomUUID(),
    }),
  );
  const signingInput = `${header}.${payload}`;
  return {
    token: `${signingInput}.${hs256(signingInput, appSecret())}`,
    expiresInSeconds: EMBED_TOKEN_TTL_SECONDS,
  };
}

/** Constant-time compare of two base64url strings of equal expected length. */
export function safeEqual(a: string, b: string): boolean {
  const left = Buffer.from(a);
  const right = Buffer.from(b);
  if (left.length !== right.length) return false;
  return timingSafeEqual(left, right);
}
