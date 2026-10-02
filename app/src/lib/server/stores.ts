import 'server-only';

import { UpstashRedis, UpstashRedisError } from './upstashRedis';

/**
 * Server-side storage boundary: rate limiting and room attribution.
 *
 * Both began as process-local Maps, which is correct exactly where one
 * process serves everything — dev, tests, a single self-hosted node — and
 * wrong anywhere else: a serverless or multi-instance deployment gives every
 * instance its own map, so each cold start hands out a fresh rate-limit
 * budget and attribution forgets itself on redeploy. In production the
 * stores therefore fail closed rather than silently degrade to per-process
 * memory: the endpoints that need them answer 503 until the deployment
 * either configures the durable store or explicitly acknowledges running as
 * a single process.
 *
 * The durable backend is Upstash Redis (the founder decision), driven over
 * its REST protocol — see `upstashRedis.ts` — and configured by the official
 * variable pair `UPSTASH_REDIS_REST_URL` / `UPSTASH_REDIS_REST_TOKEN`. Both
 * are required: half a configuration is treated as none, so a typo fails
 * closed instead of half-working. Setting them selects the durable adapters
 * in every environment, so staging can be exercised against a real resource
 * before production depends on it.
 */

/** Fixed-window counter. The Promise leaves room for a networked adapter. */
export interface RateLimitStore {
  /**
   * Consume one unit from `key`'s window. Must be atomic per key: the answer
   * has to be the same under concurrency as it would be serialized.
   */
  take(
    key: string,
    limit: number,
    windowMs: number,
  ): Promise<{ ok: boolean; retryAfterSeconds: number }>;
}

export interface AttributionStore {
  /** First write wins — attribution is set once, on first interaction. */
  attribute(wallet: string, roomId: string): Promise<void>;
  /** Wallets attributed to a room. */
  membersOf(roomId: string): Promise<string[]>;
  roomOf(wallet: string): Promise<string | null>;
}

/**
 * Thrown by the fail-closed adapters. Routes map it to a 503 — the endpoint
 * is refusing to operate without durable state, which is the configured
 * behavior, not a crash.
 */
export class StoreUnavailableError extends Error {
  constructor(feature: string) {
    super(
      `${feature} has no durable store: set UPSTASH_REDIS_REST_URL and ` +
        'UPSTASH_REDIS_REST_TOKEN (Upstash Redis), or set ' +
        'CHERRY_ALLOW_INPROCESS_STORE=1 to acknowledge a single-process deployment',
    );
    this.name = 'StoreUnavailableError';
  }
}

export class MemoryRateLimitStore implements RateLimitStore {
  private buckets = new Map<string, { count: number; resetAt: number }>();

  async take(
    key: string,
    limit: number,
    windowMs: number,
  ): Promise<{ ok: boolean; retryAfterSeconds: number }> {
    const now = Date.now();
    const bucket = this.buckets.get(key);

    if (!bucket || now >= bucket.resetAt) {
      this.buckets.set(key, { count: 1, resetAt: now + windowMs });
      return { ok: true, retryAfterSeconds: 0 };
    }
    if (bucket.count >= limit) {
      return {
        ok: false,
        retryAfterSeconds: Math.max(1, Math.ceil((bucket.resetAt - now) / 1000)),
      };
    }
    bucket.count += 1;
    return { ok: true, retryAfterSeconds: 0 };
  }
}

export class MemoryAttributionStore implements AttributionStore {
  private walletToRoom = new Map<string, string>();
  private roomToWallets = new Map<string, Set<string>>();

  async attribute(wallet: string, roomId: string): Promise<void> {
    if (this.walletToRoom.has(wallet)) return;
    this.walletToRoom.set(wallet, roomId);
    const set = this.roomToWallets.get(roomId) ?? new Set<string>();
    set.add(wallet);
    this.roomToWallets.set(roomId, set);
  }

  async membersOf(roomId: string): Promise<string[]> {
    return [...(this.roomToWallets.get(roomId) ?? [])];
  }

  async roomOf(wallet: string): Promise<string | null> {
    return this.walletToRoom.get(wallet) ?? null;
  }
}

/** Production stand-in that refuses every operation. */
class ClosedRateLimitStore implements RateLimitStore {
  async take(): Promise<never> {
    throw new StoreUnavailableError('rate limiting');
  }
}

class ClosedAttributionStore implements AttributionStore {
  async attribute(): Promise<never> {
    throw new StoreUnavailableError('room attribution');
  }
  async membersOf(): Promise<never> {
    throw new StoreUnavailableError('room attribution');
  }
  async roomOf(): Promise<never> {
    throw new StoreUnavailableError('room attribution');
  }
}

// ---- durable adapters (Upstash Redis) -------------------------------------
//
// Keys are namespaced `cherry-tree:` so this app can share a Redis with
// others without colliding, and so rate-limit and attribution data cannot
// collide with each other. Every multi-step operation runs as one Lua
// script: Redis executes scripts atomically, which is what makes the
// fixed-window counter race-free (INCR and its PEXPIRE cannot land apart)
// and keeps attribution's first-write-wins true across both of its keys.

const REDIS_NAMESPACE = 'cherry-tree:';

/**
 * Atomic fixed window: INCR, arm the TTL exactly once (on the first hit),
 * and report both the count and the remaining window in one round trip.
 */
const RATE_LIMIT_LUA = [
  "local current = redis.call('INCR', KEYS[1])",
  "if current == 1 then redis.call('PEXPIRE', KEYS[1], ARGV[1]) end",
  "return {current, redis.call('PTTL', KEYS[1])}",
].join('\n');

/**
 * Atomic first-write-wins attribution: claim the wallet→room key with SET NX
 * and, only if the claim succeeded, add the wallet to the room's member set.
 * A wallet that re-launches from another room changes nothing.
 */
const ATTRIBUTE_LUA = [
  "if redis.call('SET', KEYS[1], ARGV[1], 'NX') then",
  "  redis.call('SADD', KEYS[2], ARGV[2])",
  '  return 1',
  'end',
  'return 0',
].join('\n');

export class RedisRateLimitStore implements RateLimitStore {
  constructor(private redis: UpstashRedis) {}

  async take(
    key: string,
    limit: number,
    windowMs: number,
  ): Promise<{ ok: boolean; retryAfterSeconds: number }> {
    try {
      // All REST arguments are strings; the count comes back as numbers.
      const [current, ttl] = await this.redis.command<[number, number]>([
        'EVAL',
        RATE_LIMIT_LUA,
        '1',
        `${REDIS_NAMESPACE}rate:${key}`,
        String(windowMs),
      ]);
      if (current <= limit) return { ok: true, retryAfterSeconds: 0 };
      // PTTL of an armed key is always positive; anything else still denies
      // with a short, deterministic retry rather than trusting a raw -1/-2.
      return {
        ok: false,
        retryAfterSeconds: ttl > 0 ? Math.max(1, Math.ceil(ttl / 1000)) : 1,
      };
    } catch (e) {
      throw mapRedisError(e);
    }
  }
}

export class RedisAttributionStore implements AttributionStore {
  constructor(private redis: UpstashRedis) {}

  async attribute(wallet: string, roomId: string): Promise<void> {
    try {
      await this.redis.command<number>([
        'EVAL',
        ATTRIBUTE_LUA,
        '2',
        `${REDIS_NAMESPACE}attr:wallet:${wallet}`,
        `${REDIS_NAMESPACE}attr:room:${roomId}`,
        roomId,
        wallet,
      ]);
    } catch (e) {
      throw mapRedisError(e);
    }
  }

  async membersOf(roomId: string): Promise<string[]> {
    try {
      return await this.redis.command<string[]>([
        'SMEMBERS',
        `${REDIS_NAMESPACE}attr:room:${roomId}`,
      ]);
    } catch (e) {
      throw mapRedisError(e);
    }
  }

  async roomOf(wallet: string): Promise<string | null> {
    try {
      // REST GET answers {"result": null} for a missing key.
      return await this.redis.command<string | null>([
        'GET',
        `${REDIS_NAMESPACE}attr:wallet:${wallet}`,
      ]);
    } catch (e) {
      throw mapRedisError(e);
    }
  }
}

/** Redis being unreachable is the same condition as no store at all. */
function mapRedisError(e: unknown): unknown {
  return e instanceof UpstashRedisError
    ? new StoreUnavailableError('the Upstash Redis store did not answer')
    : e;
}

type Env = Record<string, string | undefined>;

/**
 * Pure resolution, so tests can drive the whole matrix without rewiring the
 * process. The durable Upstash adapters are selected by their variable pair
 * in every environment; in-process memory is the default everywhere else
 * except production, where it must be acknowledged explicitly.
 */
export function resolveStores(env: Env): {
  rateLimit: RateLimitStore;
  attribution: AttributionStore;
} {
  const redisUrl = env.UPSTASH_REDIS_REST_URL?.trim();
  const redisToken = env.UPSTASH_REDIS_REST_TOKEN?.trim();
  // Both halves or neither: a half configuration is a typo, and treating it
  // as configured would fail every request; treating it as absent fails
  // closed, which is the direction this module always errs in.
  if (redisUrl && redisToken) {
    const redis = new UpstashRedis(redisUrl, redisToken);
    return {
      rateLimit: new RedisRateLimitStore(redis),
      attribution: new RedisAttributionStore(redis),
    };
  }
  const inProcess =
    env.NODE_ENV !== 'production' || env.CHERRY_ALLOW_INPROCESS_STORE === '1';
  return inProcess
    ? { rateLimit: new MemoryRateLimitStore(), attribution: new MemoryAttributionStore() }
    : { rateLimit: new ClosedRateLimitStore(), attribution: new ClosedAttributionStore() };
}

// Survives Next.js dev hot reloads, which otherwise reset module state.
const globalRef = globalThis as unknown as {
  __rigsStores?: { rateLimit: RateLimitStore; attribution: AttributionStore };
};
export const stores = (globalRef.__rigsStores ??= resolveStores(process.env as Env));

export const rateLimitStore: RateLimitStore = stores.rateLimit;
export const attributionStore: AttributionStore = stores.attribution;
