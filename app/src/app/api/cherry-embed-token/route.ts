/**
 * `POST /api/cherry-embed-token` — step two of the chat sign-in.
 *
 * Takes `{ walletAddress, nonce, signature }` and returns `{ token }`: an
 * HS256 embed token signed with the Cherry app secret, per Cherry's contract.
 *
 * Cherry's example shows a body of `{ walletAddress }` alongside the rule that
 * the route must authenticate the caller first. This app has no accounts —
 * identity *is* the wallet — so the authentication step is an ownership proof
 * over a nonce this server issued. The extra two fields are what make the
 * first one trustworthy; without them the route would mint a token for any
 * wallet a caller cared to name, and Cherry enforces room access, bans and
 * rate limits against exactly that `sub`.
 *
 * The app secret is read here and never leaves: it is not in the response, not
 * in an error, and not in any log line.
 */
import {
  EmbedTokenError,
  mintEmbedToken,
} from '../../../lib/server/embedToken';
import { json } from '../../../lib/server/launchToken';
import { rateLimit } from '../../../lib/server/rateLimit';
import { StoreUnavailableError } from '../../../lib/server/stores';
import { verifyWalletProof, WalletProofError } from '../../../lib/server/walletProof';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(req: Request): Promise<Response> {
  let body: unknown;
  try {
    body = await req.json();
  } catch {
    return json({ error: 'expected a JSON body' }, 400);
  }
  if (typeof body !== 'object' || body === null) {
    return json({ error: 'expected a JSON object' }, 400);
  }

  const { walletAddress, nonce, signature } = body as Record<string, unknown>;

  // Rate-limited before the signature check, so verification is never the
  // work an unauthenticated caller can force.
  const who = req.headers.get('x-forwarded-for')?.split(',')[0]?.trim() ?? 'anon';
  let limited;
  try {
    limited = await rateLimit(`embed-token:${who}`, 20, 60_000);
  } catch (e) {
    if (e instanceof StoreUnavailableError) {
      return json({ error: 'server storage is not configured' }, 503);
    }
    throw e;
  }
  if (!limited.ok) {
    return json({ error: 'rate limited' }, 429, {
      'retry-after': String(limited.retryAfterSeconds),
    });
  }

  let proven: string;
  try {
    proven = verifyWalletProof({ walletAddress, nonce, signature });
  } catch (e) {
    // The reason is safe to return: it tells an honest client what to retry,
    // and tells an attacker only that they failed.
    return json(
      { error: e instanceof WalletProofError ? e.message : 'invalid wallet proof' },
      401,
    );
  }

  try {
    const { token, expiresInSeconds } = mintEmbedToken(proven);
    return json({ token, expiresInSeconds });
  } catch (e) {
    if (e instanceof EmbedTokenError) {
      // Missing configuration. Never echo the message — it names the secret.
      return json({ error: 'chat sign-in is not configured' }, 503);
    }
    return json({ error: 'could not mint an embed token' }, 500);
  }
}
