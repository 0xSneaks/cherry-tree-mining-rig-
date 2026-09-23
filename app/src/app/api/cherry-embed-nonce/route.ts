/**
 * `POST /api/cherry-embed-nonce` — step one of the chat sign-in.
 *
 * Hands out a short-lived, HMAC-stamped nonce for the wallet to sign. Nothing
 * is stored: the nonce carries its own timestamp and MAC, so the token route
 * can validate it without shared state.
 *
 * Deliberately reveals nothing. It takes no input, so there is no wallet
 * address to enumerate and no oracle to probe.
 */
import { json } from '../../../lib/server/launchToken';
import { rateLimit } from '../../../lib/server/rateLimit';
import { issueNonce } from '../../../lib/server/walletProof';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(req: Request): Promise<Response> {
  // Keyed by source address: minting nonces is cheap but not free, and a
  // caller has no identity yet to key on.
  const who = req.headers.get('x-forwarded-for')?.split(',')[0]?.trim() ?? 'anon';
  const limited = rateLimit(`embed-nonce:${who}`, 30, 60_000);
  if (!limited.ok) {
    return json({ error: 'rate limited' }, 429, {
      'retry-after': String(limited.retryAfterSeconds),
    });
  }

  try {
    const { nonce, message, expiresInMs } = issueNonce();
    return json({ nonce, message, expiresInMs });
  } catch {
    // The only failure here is a missing app secret, which is a deployment
    // fact, not something a caller should learn the detail of.
    return json({ error: 'chat sign-in is not configured' }, 503);
  }
}
