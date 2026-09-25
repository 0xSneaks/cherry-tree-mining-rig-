import 'server-only';

import { rateLimitStore } from './stores';

/**
 * Rate limiting keyed by the *verified* `sub` or source address, never by
 * anything a caller can choose (spec §9). Inside a WebView behind carrier
 * NAT, IP buckets punish unrelated users — and where the caller has no
 * identity yet, the route keys on what it has.
 *
 * The store behind this comes from `resolveStores`: in-process memory
 * outside production, a durable adapter once one is chosen, and a
 * fail-closed store in production until then — `rateLimit` then throws
 * `StoreUnavailableError`, which routes map to a 503.
 */
export function rateLimit(
  key: string,
  limit: number,
  windowMs: number,
): Promise<{ ok: boolean; retryAfterSeconds: number }> {
  return rateLimitStore.take(key, limit, windowMs);
}
