/**
 * The durable adapters behind the storage boundary, exercised against a
 * mocked Upstash REST endpoint — no live Redis, no network, fully
 * deterministic. What is pinned: the exact commands sent (atomic Lua, string
 * arguments, namespaced keys), the TTL arithmetic, first-write-wins
 * attribution, and that every Redis failure mode surfaces as
 * StoreUnavailableError — the same condition routes fail closed on — with
 * the token never appearing in any message.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  MemoryRateLimitStore,
  RedisAttributionStore,
  RedisRateLimitStore,
  resolveStores,
  StoreUnavailableError,
} from '../src/lib/server/stores';
import { UpstashRedis, UpstashRedisError } from '../src/lib/server/upstashRedis';

const URL_ = 'https://example-rules-2890.upstash.io';
const TOKEN = 'AX9zEXAMPLEnotreal0123456789abcdef';

type Call = { url: string; init: RequestInit };

let calls: Call[];
let respond: () => Response | Promise<Response>;

beforeEach(() => {
  calls = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string, init: RequestInit) => {
      calls.push({ url, init });
      return await respond();
    }),
  );
});
afterEach(() => {
  vi.unstubAllGlobals();
});

function result(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

/** The JSON command array the adapter sent, decoded. */
function sentCommand(): string[] {
  return JSON.parse(String(calls.at(-1)!.init.body)) as string[];
}

describe('upstash rest client', () => {
  it('posts the command as a JSON array with the bearer token', async () => {
    respond = () => result({ result: 'PONG' });
    const reply = await new UpstashRedis(URL_, TOKEN).command<string>(['PING']);
    expect(reply).toBe('PONG');
    expect(calls).toHaveLength(1);
    expect(calls[0]!.url).toBe(URL_);
    expect(new Headers(calls[0]!.init.headers).get('authorization')).toBe(
      `Bearer ${TOKEN}`,
    );
    expect(JSON.parse(String(calls[0]!.init.body))).toEqual(['PING']);
    expect(calls[0]!.init.cache).toBe('no-store');
  });

  it.each([
    ['network failure', () => Promise.reject(new TypeError('fetch failed'))],
    ['HTTP 500', () => result({ error: 'boom' }, 500)],
    ['command rejected', () => result({ error: 'ERR wrong number of arguments' })],
    ['unparseable body', () => new Response('<html>', { status: 200 })],
  ])('fails closed on %s, and never says the token', async (_name, responder) => {
    respond = () => responder() as Response | Promise<Response>;
    const client = new UpstashRedis(URL_, TOKEN);
    await expect(client.command(['PING'])).rejects.toBeInstanceOf(UpstashRedisError);
    await expect(client.command(['PING'])).rejects.toThrow(/upstash redis unavailable/);
    try {
      await client.command(['PING']);
    } catch (e) {
      expect((e as Error).message).not.toContain(TOKEN);
    }
  });
});

describe('redis rate-limit store', () => {
  const store = new RedisRateLimitStore(new UpstashRedis(URL_, TOKEN));

  it('admits while the atomic counter is within the limit', async () => {
    respond = () => result({ result: [3, 45_000] });
    expect(await store.take('session:W1', 3, 60_000)).toEqual({ ok: true, retryAfterSeconds: 0 });
  });

  it('denies past the limit and reports the remaining window, rounded up', async () => {
    respond = () => result({ result: [4, 45_123] });
    expect(await store.take('session:W1', 3, 60_000)).toEqual({ ok: false, retryAfterSeconds: 46 });
  });

  it('floors the retry at one second even for a degenerate TTL answer', async () => {
    respond = () => result({ result: [4, 0] });
    expect((await store.take('session:W1', 3, 60_000)).retryAfterSeconds).toBe(1);
    respond = () => result({ result: [4, -1] });
    expect((await store.take('session:W1', 3, 60_000)).retryAfterSeconds).toBe(1);
  });

  it('runs as one atomic Lua script over a namespaced key, with string args', async () => {
    respond = () => result({ result: [1, 60_000] });
    await store.take('embed-nonce:203.0.113.9', 30, 60_000);
    const cmd = sentCommand();
    expect(cmd[0]).toBe('EVAL');
    expect(cmd[1]).toContain("redis.call('INCR', KEYS[1])");
    // PEXPIRE is armed inside the same script — INCR can never outlive it.
    expect(cmd[1]).toContain("PEXPIRE', KEYS[1], ARGV[1]");
    expect(cmd[2]).toBe('1'); // numkeys — a string, as REST requires
    expect(cmd[3]).toBe('cherry-tree:rate:embed-nonce:203.0.113.9');
    expect(cmd[4]).toBe('60000'); // the window, as a string
  });

  it('keeps a hostile key as one inert argument — no command injection', async () => {
    respond = () => result({ result: [1, 60_000] });
    await store.take('x"; FLUSHALL; "', 30, 60_000);
    const cmd = sentCommand();
    expect(cmd[3]).toBe('cherry-tree:rate:x"; FLUSHALL; "');
    expect(cmd).toHaveLength(5);
  });

  it('surfaces Redis failure as StoreUnavailableError — the fail-closed condition', async () => {
    respond = () => result({ error: 'READONLY' }, 500);
    await expect(store.take('k', 1, 1)).rejects.toBeInstanceOf(StoreUnavailableError);
    // And the mapping keeps the founder-remediation message.
    await expect(store.take('k', 1, 1)).rejects.toThrow(/did not answer/);
  });
});

describe('redis attribution store', () => {
  const store = new RedisAttributionStore(new UpstashRedis(URL_, TOKEN));

  it('claims atomically: SET NX on the wallet key, then the room set', async () => {
    respond = () => result({ result: 1 });
    await store.attribute('W1', 'room-a');
    const cmd = sentCommand();
    expect(cmd[0]).toBe('EVAL');
    expect(cmd[1]).toContain("SET', KEYS[1], ARGV[1], 'NX'");
    expect(cmd[1]).toContain("SADD', KEYS[2], ARGV[2]");
    expect(cmd[2]).toBe('2');
    expect(cmd[3]).toBe('cherry-tree:attr:wallet:W1');
    expect(cmd[4]).toBe('cherry-tree:attr:room:room-a');
    expect(cmd.slice(5)).toEqual(['room-a', 'W1']);
  });

  it('resolves void on both claim outcomes — first write wins either way', async () => {
    respond = () => result({ result: 1 });
    await expect(store.attribute('W1', 'room-a')).resolves.toBeUndefined();
    respond = () => result({ result: 0 }); // already attributed elsewhere
    await expect(store.attribute('W1', 'room-b')).resolves.toBeUndefined();
  });

  it('reads members with SMEMBERS on the room key', async () => {
    respond = () => result({ result: ['W1', 'W2'] });
    expect(await store.membersOf('room-a')).toEqual(['W1', 'W2']);
    expect(sentCommand()).toEqual(['SMEMBERS', 'cherry-tree:attr:room:room-a']);
  });

  it('reads attribution with GET; a missing key is null', async () => {
    respond = () => result({ result: 'room-a' });
    expect(await store.roomOf('W1')).toBe('room-a');
    expect(sentCommand()).toEqual(['GET', 'cherry-tree:attr:wallet:W1']);

    respond = () => result({ result: null });
    expect(await store.roomOf('nobody')).toBeNull();
  });

  it('namespaces rate and attribution keys apart — no cross-feature collisions', async () => {
    respond = () => result({ result: [1, 1000] });
    await new RedisRateLimitStore(new UpstashRedis(URL_, TOKEN)).take('room-a', 1, 1000);
    expect(sentCommand()[3]).toBe('cherry-tree:rate:room-a');
    respond = () => result({ result: null });
    await store.roomOf('room-a');
    expect(sentCommand()[1]).toBe('cherry-tree:attr:wallet:room-a');
  });

  it('fails closed on Redis error for every operation', async () => {
    respond = () => Promise.reject(new TypeError('fetch failed'));
    await expect(store.attribute('W1', 'r')).rejects.toBeInstanceOf(StoreUnavailableError);
    await expect(store.membersOf('r')).rejects.toBeInstanceOf(StoreUnavailableError);
    await expect(store.roomOf('W1')).rejects.toBeInstanceOf(StoreUnavailableError);
  });
});

describe('resolveStores — the Upstash pair selects the durable adapters', () => {
  const PAIR = { UPSTASH_REDIS_REST_URL: URL_, UPSTASH_REDIS_REST_TOKEN: TOKEN };

  it('selects them in production, overriding any in-process acknowledgment', () => {
    const { rateLimit, attribution } = resolveStores({
      ...PAIR,
      NODE_ENV: 'production',
      CHERRY_ALLOW_INPROCESS_STORE: '1',
    });
    expect(rateLimit).toBeInstanceOf(RedisRateLimitStore);
    expect(attribution).toBeInstanceOf(RedisAttributionStore);
  });

  it('selects them outside production too, so staging can exercise them', () => {
    const { rateLimit } = resolveStores({ ...PAIR, NODE_ENV: 'development' });
    expect(rateLimit).toBeInstanceOf(RedisRateLimitStore);
  });

  it('treats half a pair as unconfigured — production then fails closed', async () => {
    const { rateLimit } = resolveStores({
      UPSTASH_REDIS_REST_URL: URL_,
      NODE_ENV: 'production',
    });
    await expect(rateLimit.take('k', 1, 1)).rejects.toBeInstanceOf(StoreUnavailableError);
  });

  it('still honors in-process memory for dev and tests with no Redis configured', () => {
    const { rateLimit } = resolveStores({ NODE_ENV: 'test' });
    expect(rateLimit).toBeInstanceOf(MemoryRateLimitStore);
  });
});
