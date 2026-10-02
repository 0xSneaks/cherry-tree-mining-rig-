import 'server-only';

/**
 * Proof that the caller actually holds the wallet they are asking a token for.
 *
 * Cherry's contract says the token route must authenticate the caller and
 * "only issue a token for the logged-in user's wallet" — a `sub` taken
 * straight from the request body lets any client mint a token for any wallet
 * and inherit its room access, bans and rate limits.
 *
 * THE CHERRY TREE has no accounts and no login: identity here *is* the wallet.
 * So the login step is an ownership proof — the server issues a nonce, the
 * wallet signs it, and the server verifies the Ed25519 signature before
 * minting. That is the standalone equivalent of a session, and it is why the
 * route's body is `{ walletAddress, nonce, signature }` rather than the
 * `{ walletAddress }` in Cherry's example: the extra fields are what make the
 * first field trustworthy.
 *
 * Stateless by construction. The nonce carries its own HMAC and timestamp, so
 * nothing has to be remembered between the two requests and the route works
 * unchanged across serverless instances. The HMAC key is derived from the app
 * secret with a domain separator, so there is no second secret to configure
 * and the app secret itself is never used directly here.
 */
import { createHmac, createPublicKey, randomBytes, verify } from 'node:crypto';
import { address, getAddressEncoder } from '@solana/kit';

import { appSecret, safeEqual } from './embedToken';

/** How long a nonce stays usable. Long enough to approve a wallet prompt. */
export const NONCE_TTL_MS = 120_000;

export class WalletProofError extends Error {}

/** A separate key for nonces, so the Cherry app secret is never the MAC key. */
function nonceKey(): Buffer {
  return createHmac('sha256', appSecret())
    .update('cherry-tree/embed-nonce/v1')
    .digest();
}

/**
 * What the user actually signs.
 *
 * Human-readable on purpose: a wallet shows this text, and someone approving
 * it should be able to tell what they are agreeing to. It names the app and
 * says plainly that it authorises nothing on chain.
 */
export function challengeMessage(nonce: string): string {
  return [
    'THE CHERRY TREE — community chat sign-in',
    '',
    'Signing proves you hold this wallet so the chat can show you as its owner.',
    'This signature does NOT approve a transaction and moves no SOL.',
    '',
    `Nonce: ${nonce}`,
  ].join('\n');
}

/** Issue a nonce: `<issuedAtMs>.<random>.<hmac>`. */
export function issueNonce(): { nonce: string; message: string; expiresInMs: number } {
  const body = `${Date.now()}.${randomBytes(18).toString('base64url')}`;
  const mac = createHmac('sha256', nonceKey()).update(body).digest('base64url');
  const nonce = `${body}.${mac}`;
  return { nonce, message: challengeMessage(nonce), expiresInMs: NONCE_TTL_MS };
}

/** Check a nonce is ours and still fresh. Throws otherwise. */
export function assertNonceValid(nonce: string, now = Date.now()): void {
  const parts = nonce.split('.');
  if (parts.length !== 3) throw new WalletProofError('malformed nonce');
  const [issuedAt, random, mac] = parts as [string, string, string];

  const expected = createHmac('sha256', nonceKey())
    .update(`${issuedAt}.${random}`)
    .digest('base64url');
  if (!safeEqual(mac, expected)) throw new WalletProofError('nonce is not ours');

  const at = Number(issuedAt);
  if (!Number.isFinite(at)) throw new WalletProofError('malformed nonce timestamp');
  // A clock that runs backwards should fail closed, not mint forever.
  if (at > now + 5_000) throw new WalletProofError('nonce is from the future');
  if (now - at > NONCE_TTL_MS) throw new WalletProofError('nonce has expired');
}

/** SPKI prefix for an Ed25519 public key, so a raw 32-byte key can be imported. */
const ED25519_SPKI_PREFIX = Buffer.from('302a300506032b6570032100', 'hex');

/** Verify `signature` is `walletAddress`'s Ed25519 signature over `message`. */
export function verifySignature(
  walletAddress: string,
  message: Uint8Array,
  signature: Uint8Array,
): boolean {
  // Ed25519 signatures are always 64 bytes; anything else is not one.
  if (signature.length !== 64) return false;

  let raw: Uint8Array;
  try {
    raw = new Uint8Array(getAddressEncoder().encode(address(walletAddress)));
  } catch {
    return false;
  }
  if (raw.length !== 32) return false;

  try {
    const key = createPublicKey({
      key: Buffer.concat([ED25519_SPKI_PREFIX, Buffer.from(raw)]),
      format: 'der',
      type: 'spki',
    });
    return verify(null, Buffer.from(message), key, Buffer.from(signature));
  } catch {
    // An address that is not a valid curve point cannot have signed anything.
    return false;
  }
}

/**
 * The whole check, in the order that fails cheapest first.
 *
 * Returns the proven wallet address. Anything short of a valid, fresh nonce
 * and a matching signature throws.
 */
export function verifyWalletProof(input: {
  walletAddress: unknown;
  nonce: unknown;
  signature: unknown;
}): string {
  const { walletAddress, nonce, signature } = input;

  if (typeof walletAddress !== 'string' || walletAddress.length === 0) {
    throw new WalletProofError('walletAddress is required');
  }
  if (typeof nonce !== 'string' || nonce.length === 0) {
    throw new WalletProofError('nonce is required');
  }
  if (typeof signature !== 'string' || signature.length === 0) {
    throw new WalletProofError('signature is required');
  }

  assertNonceValid(nonce);

  let bytes: Buffer;
  try {
    bytes = Buffer.from(signature, 'base64');
  } catch {
    throw new WalletProofError('signature is not base64');
  }

  const message = new TextEncoder().encode(challengeMessage(nonce));
  if (!verifySignature(walletAddress, message, bytes)) {
    throw new WalletProofError('signature does not match the wallet');
  }

  return walletAddress;
}
