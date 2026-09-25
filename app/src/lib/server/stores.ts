import 'server-only';

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
 * either configures a durable store or explicitly acknowledges running as a
 * single process.
 *
 * No durable backend is implemented yet. That is deliberate: which one to
 * buy is a founder decision, not an engineering default, and this file is
 * the entire seam — when a vendor is chosen, its adapter implements these
 * interfaces, `resolveStores` wires it from configuration, and nothing
 * outside this module changes. `checkProductionEnv` surfaces the open
 * decision to the readiness tooling so a launch cannot miss it.
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
      `${feature} has no durable store: choose one (founder decision) or set ` +
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

type Env = Record<string, string | undefined>;

/**
 * Pure resolution, so tests can drive the whole matrix without rewiring the
 * process. In-process memory is the default everywhere except production,
 * where it must be acknowledged explicitly.
 */
export function resolveStores(env: Env): {
  rateLimit: RateLimitStore;
  attribution: AttributionStore;
} {
  // No durable adapter exists yet; when one lands, its configuration branch
  // goes first here and the in-process fallbacks stay for dev and tests.
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
