import 'server-only';

/**
 * Room attribution.
 *
 * A miner is attributed to the room they first interacted from, taken from the
 * *verified* `room_id` claim — never from anything the client sends. Kept
 * off-chain deliberately: V1 puts no room attribution on chain, and the
 * leaderboard has zero effect on reward math (spec §8).
 *
 * The in-memory map is the development store. A deployment swaps
 * `AttributionStore` for a real one; the interface is the whole contract.
 */
export interface AttributionStore {
  /** First write wins — attribution is set once, on first interaction. */
  attribute(wallet: string, roomId: string): Promise<void>;
  /** Wallets attributed to a room. */
  membersOf(roomId: string): Promise<string[]>;
  roomOf(wallet: string): Promise<string | null>;
}

class MemoryAttributionStore implements AttributionStore {
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

// Survives Next.js dev hot reloads, which otherwise reset module state.
const globalRef = globalThis as unknown as { __rigsAttribution?: AttributionStore };
export const attributionStore: AttributionStore =
  globalRef.__rigsAttribution ?? (globalRef.__rigsAttribution = new MemoryAttributionStore());
