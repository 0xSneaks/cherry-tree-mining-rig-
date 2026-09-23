/**
 * Stubs for the two things a browser test cannot reach: the Solana RPC and
 * the Cherry host.
 *
 * The account data served here is the byte-for-byte Borsh output of the Rust
 * program (`cargo test -p sol-rigs --test discriminators`), so the app does
 * real decoding of real layouts — only the transport is faked.
 */
import type { Page, Route } from '@playwright/test';

export const WALLET = '7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU';

/** Rust fixture: paused=true, so it is rewritten per test as needed. */
const CONFIG_HEX =
  '9b0caae01efacc820101010101010101010101010101010101010101010101010101010101010101020202020202020202020202020202020202020202020202020202020202020203030303030303030303030303030303030303030303030303030303030303030180969800000000002c01c937000000000000b288840100000000803bb16a0000000000a5186a00000000d20a3f4eeee073c3f60fe98e010000009210000000000000c007857303000000c08c6c050000000040ed9604000000000000943577000000000087930300000040420f0000000000db03000000000000fafbfc0100fd';

const MINER_HEX =
  'df710f367b7a8c6409090909090909090909090909090909090909090909090909090909090909092a00000000000000ea7e26e5384da55a050000000000000038191701000000001e000000000000000c0000000000000040f76a1200000000000e270700000000e0dc4e0300000000800e80690000000040f0646a00000000fe';

function hexToBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

function toBase64(bytes: Uint8Array): string {
  return Buffer.from(bytes).toString('base64');
}

function setU8(bytes: Uint8Array, offset: number, value: number): Uint8Array {
  const copy = bytes.slice();
  copy[offset] = value;
  return copy;
}

function setI64(bytes: Uint8Array, offset: number, value: bigint): Uint8Array {
  const copy = bytes.slice();
  new DataView(copy.buffer, copy.byteOffset).setBigInt64(offset, value, true);
  return copy;
}

function setU64(bytes: Uint8Array, offset: number, value: bigint): Uint8Array {
  const copy = bytes.slice();
  new DataView(copy.buffer, copy.byteOffset).setBigUint64(offset, value, true);
  return copy;
}

function setU128(bytes: Uint8Array, offset: number, value: bigint): Uint8Array {
  const copy = bytes.slice();
  const view = new DataView(copy.buffer, copy.byteOffset);
  view.setBigUint64(offset, value & 0xffff_ffff_ffff_ffffn, true);
  view.setBigUint64(offset + 8, value >> 64n, true);
  return copy;
}

// Byte offsets inside the Config account, discriminator included.
const OFF_PAUSED = 8 + 32 * 3;
const OFF_PERIOD_FINISH = OFF_PAUSED + 1 + 8 + 2 + 8 + 8;
const OFF_LAST_UPDATE = OFF_PERIOD_FINISH + 8;
const OFF_RPR_STORED = OFF_LAST_UPDATE + 8;
const OFF_RATE = OFF_PAUSED + 1 + 8 + 2 + 8;

/** ≈2.200 SOL/day across the whole mine, as on the spec's example screen. */
const REWARD_RATE = 25_462n;

// Byte offsets inside the Miner account.
const OFF_MINER_RPR_PAID = 8 + 32 + 8;
const OFF_MINER_PENDING = OFF_MINER_RPR_PAID + 16;

/**
 * The Rust fixtures deliberately carry extreme values to stress the decoder.
 * For rendering, the accumulator is rewritten to a realistic pair: at 42 rigs
 * the watermark gap below settles to exactly 0.018291 SOL claimable, which is
 * the figure the spec's example screen shows.
 */
const RPR_STORED = 1_000_000_000_000_000_000n;
const RPR_PAID = 564_500_000_000_000_000n;

export type StubOptions = {
  paused?: boolean;
  /** Chain time the stub reports; the schedule is written around it. */
  now?: bigint;
  /** Seconds of funded window remaining from `now`. */
  runwaySeconds?: bigint;
  /** Omit the miner account, as for a wallet that has never bought a rig. */
  withMiner?: boolean;
};

export function configBytes(opts: StubOptions = {}): Uint8Array {
  const now = opts.now ?? 1_780_000_000n;
  let bytes = hexToBytes(CONFIG_HEX);
  bytes = setU8(bytes, OFF_PAUSED, opts.paused ? 1 : 0);
  bytes = setI64(bytes, OFF_PERIOD_FINISH, now + (opts.runwaySeconds ?? 604_800n));
  // Anchored at `now` so the rendered claimable starts at exactly the
  // 0.018291 SOL the spec's example screen shows, then ticks from there.
  bytes = setI64(bytes, OFF_LAST_UPDATE, now);
  bytes = setU64(bytes, OFF_RATE, REWARD_RATE);
  bytes = setU128(bytes, OFF_RPR_STORED, RPR_STORED);
  return bytes;
}

export function minerBytes(): Uint8Array {
  let bytes = hexToBytes(MINER_HEX);
  bytes = setU128(bytes, OFF_MINER_RPR_PAID, RPR_PAID);
  bytes = setU64(bytes, OFF_MINER_PENDING, 0n);
  return bytes;
}

/** Intercept every Solana JSON-RPC call the app makes. */
export async function stubRpc(page: Page, opts: StubOptions = {}): Promise<void> {
  const now = opts.now ?? 1_780_000_000n;
  const withMiner = opts.withMiner ?? true;

  const account = (data: Uint8Array, lamports: string) => ({
    data: [toBase64(data), 'base64'],
    executable: false,
    lamports,
    owner: 'By7qjU4ZYBYQvdkh41UjnhxWmLyP63S3PbdZXbg1UiKH',
    rentEpoch: '0',
    space: String(data.length),
  });

  await page.route('**/api.devnet.solana.com/**', async (route: Route) => {
    const body = route.request().postDataJSON() as
      | { id: number; method: string; params?: unknown[] }
      | Array<{ id: number; method: string; params?: unknown[] }>;

    const answer = (req: { id: number; method: string }) => {
      const ctx = { context: { apiVersion: '2.3.0', slot: '300000000' } };
      switch (req.method) {
        case 'getMultipleAccounts':
          return {
            jsonrpc: '2.0',
            id: req.id,
            result: {
              ...ctx,
              value: [
                account(configBytes(opts), '14823000000'),
                account(new Uint8Array(10), '14824000000'),
                ...(withMiner ? [account(minerBytes(), '2400000')] : [null]),
              ],
            },
          };
        case 'getSlot':
          return { jsonrpc: '2.0', id: req.id, result: 300000000 };
        case 'getBlockTime':
          return { jsonrpc: '2.0', id: req.id, result: Number(now) };
        case 'getMinimumBalanceForRentExemption':
          return { jsonrpc: '2.0', id: req.id, result: 1002240 };
        case 'getBalance':
          return {
            jsonrpc: '2.0',
            id: req.id,
            result: { ...ctx, value: 4_000_000_000 },
          };
        default:
          return { jsonrpc: '2.0', id: req.id, result: null };
      }
    };

    const result = Array.isArray(body) ? body.map(answer) : answer(body);
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(result),
    });
  });
}

/**
 * Impersonate the Cherry host.
 *
 * The real handshake is a `cherry:init` message carrying a launch-token JWT
 * that the SDK decodes for the wallet, user and room. The stub mints the same
 * shape (unsigned — the client only decodes it; the *backend* is what
 * verifies signatures, and that path is covered in `tests/launchToken.test.ts`).
 */
export async function stubCherryHost(
  page: Page,
  opts: { room?: { id: string; title: string; memberCount: number } } = {},
): Promise<void> {
  const room = opts.room ?? { id: 'room-42', title: 'Mine Shaft', memberCount: 412 };

  const b64url = (o: unknown) =>
    Buffer.from(JSON.stringify(o))
      .toString('base64')
      .replace(/\+/g, '-')
      .replace(/\//g, '_')
      .replace(/=+$/, '');

  const token = [
    b64url({ alg: 'RS256', typ: 'JWT' }),
    b64url({
      sub: WALLET,
      app_id: 'sol-rigs-test-app',
      room_id: room.id,
      room: { title: room.title, member_count: room.memberCount },
      user: { display_name: 'sneaks', avatar_url: '' },
      iat: Math.floor(Date.now() / 1000),
      exp: Math.floor(Date.now() / 1000) + 300,
    }),
    'stub-signature',
  ].join('.');

  await page.addInitScript(
    ({ token }) => {
      const w = window as unknown as {
        __cherry?: boolean;
        ReactNativeWebView?: { postMessage(data: string): void };
      };
      w.__cherry = true;

      const post = (msg: unknown) => window.postMessage(msg, '*');

      // In a WebView the SDK talks to the host through
      // `ReactNativeWebView.postMessage`; answer requests the way the native
      // host would, by echoing a `cherry:response` back into the page.
      w.ReactNativeWebView = {
        postMessage(raw: string) {
          let msg: { type?: string; id?: string; method?: string };
          try {
            msg = JSON.parse(raw);
          } catch {
            return;
          }
          if (msg.type !== 'cherry:request' || !msg.id) return;
          const result =
            msg.method === 'wallet.signAndSendTransaction'
              ? { signature: '1'.repeat(88) }
              : {};
          setTimeout(() => post({ type: 'cherry:response', id: msg.id, result }), 0);
        },
      };

      // The host pushes `cherry:init` unprompted once the page is up.
      const init = () => post({ type: 'cherry:init', version: 2, token });
      if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', () => setTimeout(init, 0));
      } else {
        setTimeout(init, 0);
      }
      // The SDK buffers an init that arrives before it starts listening, but
      // repeating it keeps the stub robust to load-order changes.
      setTimeout(init, 150);
      setTimeout(init, 600);

      // Lets a test push a lifecycle event the way the host would.
      (window as unknown as { __cherryEmit: (e: string) => void }).__cherryEmit = (
        event: string,
      ) => post({ type: 'cherry:event', event });
    },
    { token },
  );
}

/**
 * Impersonate the Cherry host for an *inline* launch — a blink card, or the
 * live preview Cherry shows in the share picker.
 *
 * Inline mode is a different bootstrap from the embedded one above: the SDK
 * reads `mode` and `token` from the launch URL and then asks the host for the
 * blink context over the bridge (`host.init`), rather than waiting for the
 * host to push `cherry:init`. The snapshot the card renders comes back on
 * that response, so this stub is what makes the blink path testable at all.
 */
export async function stubCherryInlineHost(
  page: Page,
  ctx: {
    params: Record<string, unknown>;
    messageId?: string;
    roomId?: string;
    route?: string;
  },
): Promise<void> {
  const response = {
    viewerWallet: WALLET,
    roomId: ctx.roomId ?? 'room-42',
    messageId: ctx.messageId ?? 'msg-1',
    route: ctx.route ?? '/result',
    params: ctx.params,
    height: 'medium',
  };

  await page.addInitScript((hostInit) => {
    const w = window as unknown as {
      __cherry?: boolean;
      ReactNativeWebView?: { postMessage(data: string): void };
    };
    w.__cherry = true;

    w.ReactNativeWebView = {
      postMessage(raw: string) {
        let msg: { type?: string; id?: string; method?: string };
        try {
          msg = JSON.parse(raw);
        } catch {
          return;
        }
        if (msg.type !== 'cherry:request' || !msg.id) return;
        const result = msg.method === 'host.init' ? hostInit : {};
        setTimeout(() => window.postMessage({ type: 'cherry:response', id: msg.id, result }, '*'), 0);
      },
    };
  }, response);
}
