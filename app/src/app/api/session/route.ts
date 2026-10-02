/**
 * `POST /api/session` — verify the launch token, record room attribution.
 *
 * The wallet address comes from the verified `sub` claim. A wallet address in
 * the request body is ignored entirely; there is nothing a client can send
 * that changes who this endpoint thinks they are.
 */
import { json, requireSession } from '../../../lib/server/launchToken';
import { rateLimit } from '../../../lib/server/rateLimit';
import { attributionStore, StoreUnavailableError } from '../../../lib/server/stores';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(req: Request): Promise<Response> {
  const auth = await requireSession(req);
  if ('response' in auth) return auth.response;
  const { session } = auth;

  let limited;
  try {
    limited = await rateLimit(`session:${session.sub}`, 30, 60_000);
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

  try {
    // First interaction wins; re-launching from another room does not move a
    // miner's attribution.
    if (session.roomId) {
      await attributionStore.attribute(session.sub, session.roomId);
    }

    return json({
      wallet: session.sub,
      roomId: session.roomId,
      attributedRoomId: await attributionStore.roomOf(session.sub),
    });
  } catch (e) {
    if (e instanceof StoreUnavailableError) {
      return json({ error: 'server storage is not configured' }, 503);
    }
    throw e;
  }
}
