/**
 * `GET /api/room/:id/leaderboard` — top rigs among miners who launched from
 * this room.
 *
 * Room-scoped, so the launch token is verified and the requested room must be
 * the one in the *verified* `room_id` claim. A client cannot read another
 * room's board by changing the path.
 *
 * Display only: this endpoint reads indexed chain data and has no effect
 * whatsoever on reward math.
 */
import { json, requireSession } from '../../../../../lib/server/launchToken';
import { allMiners } from '../../../../../lib/server/indexer';
import { rateLimit } from '../../../../../lib/server/rateLimit';
import { attributionStore } from '../../../../../lib/server/rooms';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const LIMIT = 20;

export async function GET(
  req: Request,
  { params }: { params: Promise<{ id: string }> },
): Promise<Response> {
  const auth = await requireSession(req);
  if ('response' in auth) return auth.response;
  const { session } = auth;

  const { id } = await params;
  if (!session.roomId || session.roomId !== id) {
    return json({ error: 'launch token does not grant access to this room' }, 403);
  }

  const limited = rateLimit(`leaderboard:${session.sub}`, 60, 60_000);
  if (!limited.ok) {
    return json({ error: 'rate limited' }, 429, {
      'retry-after': String(limited.retryAfterSeconds),
    });
  }

  const [members, miners] = await Promise.all([
    attributionStore.membersOf(id),
    allMiners(),
  ]);
  const inRoom = new Set(members);

  const entries = miners
    .filter((m) => inRoom.has(m.owner))
    .sort((a, b) => (a.rigs === b.rigs ? 0 : a.rigs > b.rigs ? -1 : 1))
    .slice(0, LIMIT)
    .map((m) => ({ address: m.owner, rigs: m.rigs.toString() }));

  return json({ roomId: id, entries });
}
