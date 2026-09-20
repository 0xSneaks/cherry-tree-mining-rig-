import 'server-only';

/**
 * Launch-token verification.
 *
 * Every endpoint that returns anything user-scoped or room-scoped verifies the
 * token server-side. A wallet address in a request body is never trusted
 * (spec §8, §9).
 *
 * The token authenticates a *session*, not a transaction. It must never gate a
 * money movement — the program's account validation does that, and this
 * backend cannot move SOL or hold a key that could.
 */
import { verifyLaunchToken } from '@cherrydotfun/miniapp-sdk';

export type VerifiedSession = {
  /** Verified wallet address. */
  sub: string;
  /** Room the mini-app was opened from, when launched from one. */
  roomId: string | null;
  /** Blink message id, when this is an inline launch. */
  messageId: string | null;
  /** Signed snapshot params for an inline launch. */
  params: Record<string, unknown> | null;
  route: string | null;
};

export class LaunchTokenError extends Error {}

function appId(): string {
  const id = process.env.CHERRY_APP_ID;
  if (!id) {
    throw new LaunchTokenError(
      'CHERRY_APP_ID is not set; refusing to verify launch tokens without an expected audience',
    );
  }
  return id;
}

export async function verifySession(token: string): Promise<VerifiedSession> {
  if (!token) throw new LaunchTokenError('missing launch token');

  const payload = await verifyLaunchToken(token, {
    expectedAppId: appId(),
    ...(process.env.CHERRY_EXPECTED_ORIGIN
      ? { expectedOrigin: process.env.CHERRY_EXPECTED_ORIGIN }
      : {}),
    ...(process.env.CHERRY_JWKS_URL ? { jwksUrl: process.env.CHERRY_JWKS_URL } : {}),
  });

  const sub = typeof payload.sub === 'string' ? payload.sub : null;
  if (!sub) throw new LaunchTokenError('launch token has no subject');

  const record = payload as unknown as Record<string, unknown>;
  return {
    sub,
    roomId: typeof record.room_id === 'string' ? record.room_id : null,
    messageId: typeof record.message_id === 'string' ? record.message_id : null,
    params:
      record.params && typeof record.params === 'object'
        ? (record.params as Record<string, unknown>)
        : null,
    route: typeof record.route === 'string' ? record.route : null,
  };
}

/** Pull the token from `Authorization: Bearer …` or a `token` query field. */
export function tokenFromRequest(req: Request): string | null {
  const auth = req.headers.get('authorization');
  if (auth?.toLowerCase().startsWith('bearer ')) return auth.slice(7).trim();
  const url = new URL(req.url);
  return url.searchParams.get('token');
}

/** Verify, or return the 401 the caller should send back. */
export async function requireSession(
  req: Request,
): Promise<{ session: VerifiedSession } | { response: Response }> {
  const token = tokenFromRequest(req);
  if (!token) {
    return { response: json({ error: 'missing launch token' }, 401) };
  }
  try {
    return { session: await verifySession(token) };
  } catch (e) {
    return {
      response: json(
        { error: e instanceof Error ? e.message : 'invalid launch token' },
        401,
      ),
    };
  }
}

export function json(body: unknown, status = 200, extraHeaders?: HeadersInit): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      'content-type': 'application/json; charset=utf-8',
      'cache-control': 'no-store',
      ...Object.fromEntries(new Headers(extraHeaders).entries()),
    },
  });
}
