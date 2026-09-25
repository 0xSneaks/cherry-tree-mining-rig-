/**
 * The storage boundary for rate limiting and room attribution. What is pinned
 * here: the in-process adapters keep the semantics the routes always had, the
 * resolver never lets production silently run on process-local memory, and
 * the fail-closed stores refuse every operation so routes answer 503 instead
 * of degrading to per-instance state.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  MemoryAttributionStore,
  MemoryRateLimitStore,
  resolveStores,
  StoreUnavailableError,
} from '../src/lib/server/stores';

describe('memory rate-limit store', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('admits up to the limit inside a window, then reports the wait', async () => {
    const store = new MemoryRateLimitStore();
    for (let i = 0; i < 3; i++) {
      expect(await store.take('k', 3, 60_000)).toEqual({ ok: true, retryAfterSeconds: 0 });
    }
    const denied = await store.take('k', 3, 60_000);
    expect(denied.ok).toBe(false);
    expect(denied.retryAfterSeconds).toBe(60);
  });

  it('resets when the window has passed', async () => {
    const store = new MemoryRateLimitStore();
    await store.take('k', 1, 60_000);
    expect((await store.take('k', 1, 60_000)).ok).toBe(false);
    vi.advanceTimersByTime(60_001);
    expect((await store.take('k', 1, 60_000)).ok).toBe(true);
  });

  it('keeps keys independent — one wallet\u2019s budget is not another\u2019s', async () => {
    const store = new MemoryRateLimitStore();
    await store.take('a', 1, 60_000);
    expect((await store.take('a', 1, 60_000)).ok).toBe(false);
    expect((await store.take('b', 1, 60_000)).ok).toBe(true);
  });
});

describe('memory attribution store', () => {
  it('first write wins — re-launching from another room moves nothing', async () => {
    const store = new MemoryAttributionStore();
    await store.attribute('W1', 'room-a');
    await store.attribute('W1', 'room-b');
    expect(await store.roomOf('W1')).toBe('room-a');
    expect(await store.membersOf('room-a')).toEqual(['W1']);
    expect(await store.membersOf('room-b')).toEqual([]);
  });

  it('groups members per room', async () => {
    const store = new MemoryAttributionStore();
    await store.attribute('W1', 'room-a');
    await store.attribute('W2', 'room-a');
    expect(await store.membersOf('room-a')).toEqual(['W1', 'W2']);
  });

  it('answers null for an unattributed wallet', async () => {
    expect(await new MemoryAttributionStore().roomOf('nobody')).toBeNull();
  });
});

describe('resolveStores — production never silently runs on process memory', () => {
  it('uses the in-process adapters outside production', () => {
    const { rateLimit, attribution } = resolveStores({ NODE_ENV: 'development' });
    expect(rateLimit).toBeInstanceOf(MemoryRateLimitStore);
    expect(attribution).toBeInstanceOf(MemoryAttributionStore);
  });

  it('fails closed in production by default', async () => {
    const { rateLimit, attribution } = resolveStores({ NODE_ENV: 'production' });
    await expect(rateLimit.take('k', 1, 1)).rejects.toBeInstanceOf(StoreUnavailableError);
    await expect(attribution.attribute('W', 'r')).rejects.toBeInstanceOf(StoreUnavailableError);
    await expect(attribution.membersOf('r')).rejects.toBeInstanceOf(StoreUnavailableError);
    await expect(attribution.roomOf('W')).rejects.toBeInstanceOf(StoreUnavailableError);
  });

  it('honors an explicit single-process acknowledgment in production', () => {
    const { rateLimit, attribution } = resolveStores({
      NODE_ENV: 'production',
      CHERRY_ALLOW_INPROCESS_STORE: '1',
    });
    expect(rateLimit).toBeInstanceOf(MemoryRateLimitStore);
    expect(attribution).toBeInstanceOf(MemoryAttributionStore);
  });

  it('names the founder decision in the failure, not a vendor', async () => {
    const { rateLimit } = resolveStores({ NODE_ENV: 'production' });
    const failure = rateLimit.take('k', 1, 1);
    await expect(failure).rejects.toThrow(/founder decision/i);
    await expect(failure).rejects.toThrow(/CHERRY_ALLOW_INPROCESS_STORE/);
  });
});
