import 'server-only';

/**
 * Minimal Upstash Redis REST client — the only Upstash-specific code in the
 * repo, and deliberately dependency-free: the REST protocol is one POST of a
 * JSON command array, so there is no SDK to vendor-lock and no client
 * bundle to leak into.
 *
 * Fail-closed is the contract: every failure mode — network error, abort on
 * timeout, non-2xx status, or a per-command `{"error": …}` body — throws
 * `UpstashRedisError`. Callers (the stores in `stores.ts`) map that to
 * `StoreUnavailableError`, and routes answer 503. Nothing retries and
 * nothing degrades to memory.
 *
 * Commands travel as a JSON array, never a URL path: arguments are data by
 * construction, so no key or argument can smuggle a second command.
 * The token appears only in the Authorization header and never in any
 * thrown or logged message.
 */

/** Every element must be a string — the REST protocol rejects JSON numbers. */
export type RedisCommand = string[];

export class UpstashRedisError extends Error {
  constructor(detail: string) {
    super(`upstash redis unavailable: ${detail}`);
    this.name = 'UpstashRedisError';
  }
}

/** A request budget so an unresponsive Redis fails closed, not by hanging. */
const TIMEOUT_MS = 3_000;

export class UpstashRedis {
  constructor(
    private readonly url: string,
    private readonly token: string,
  ) {}

  /** Send one command; resolves its `result`, or throws UpstashRedisError. */
  async command<T>(cmd: RedisCommand): Promise<T> {
    let response: Response;
    try {
      response = await fetch(this.url, {
        method: 'POST',
        headers: {
          authorization: `Bearer ${this.token}`,
          'content-type': 'application/json',
        },
        body: JSON.stringify(cmd),
        cache: 'no-store',
        signal: AbortSignal.timeout(TIMEOUT_MS),
      });
    } catch (e) {
      // AbortError, DNS, TLS — all "the store is not answering".
      throw new UpstashRedisError(e instanceof Error ? e.name : 'request failed');
    }

    if (!response.ok) {
      // Status only: the body is Upstash's text and adds nothing the route
      // could act on, and echoing responses into errors is how credentials
      // eventually reach logs.
      throw new UpstashRedisError(`HTTP ${response.status}`);
    }

    let body: { result?: unknown; error?: unknown };
    try {
      body = (await response.json()) as { result?: unknown; error?: unknown };
    } catch {
      throw new UpstashRedisError('unparseable response');
    }
    if (body.error !== undefined) {
      throw new UpstashRedisError('command rejected');
    }
    return body.result as T;
  }
}
