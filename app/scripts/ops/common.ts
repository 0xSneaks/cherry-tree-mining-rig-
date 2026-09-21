/**
 * Shared plumbing for THE CHERRY TREE devnet operations scripts.
 *
 * These run on an operator's machine, never in the browser and never on the
 * server that serves the mini-app. They are the only code in the repository
 * that loads a keypair from disk, and they never print, log or transmit
 * secret material — only public keys.
 */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import {
  appendTransactionMessageInstruction,
  assertIsTransactionWithBlockhashLifetime,
  createKeyPairSignerFromBytes,
  createSolanaRpc,
  createSolanaRpcSubscriptions,
  getBase64Encoder,
  getSignatureFromTransaction,
  sendAndConfirmTransactionFactory,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  signTransactionMessageWithSigners,
  createTransactionMessage,
  type Address,
  type Instruction,
  type KeyPairSigner,
} from '@solana/kit';

import { decodeConfig, decodeMiner, type Config, type Miner } from '../../src/lib/program/accounts';
import { programAddresses, minerPda } from '../../src/lib/program/pdas';
import { LAMPORTS_PER_SOL } from '../../src/lib/program/constants';

export const REPO_ROOT = resolve(import.meta.dirname, '..', '..', '..');

// ---------------------------------------------------------------- env

/** Reads the repo `.env` without printing any of it. */
export function loadEnv(): void {
  let raw: string;
  try {
    raw = readFileSync(resolve(REPO_ROOT, '.env'), 'utf8');
  } catch {
    return;
  }
  for (const line of raw.split('\n')) {
    const t = line.trim();
    if (!t || t.startsWith('#')) continue;
    const eq = t.indexOf('=');
    if (eq < 1) continue;
    const key = t.slice(0, eq).trim();
    const value = t.slice(eq + 1).trim().replace(/^["']|["']$/g, '');
    if (process.env[key] === undefined) process.env[key] = value;
  }
}

/**
 * Aborts the current operation.
 *
 * Throws rather than calling `process.exit` so the guards are testable: each
 * script's top-level `catch` prints the message and sets the exit code.
 */
export class OpsError extends Error {}

export function fail(message: string): never {
  throw new OpsError(message);
}

/**
 * The single place that decides whether a script may talk to a cluster.
 * Mainnet is refused outright, and a devnet label pointed at a mainnet RPC is
 * refused too — that is the mistake actually worth catching.
 */
export function requireDevnet(): { cluster: string; rpcUrl: string } {
  const cluster = process.env.SOLANA_CLUSTER ?? '';
  const rpcUrl = process.env.SOLANA_RPC_URL ?? 'https://api.devnet.solana.com';

  if (!cluster) fail('SOLANA_CLUSTER is not set. Refusing to guess.');
  if (/^mainnet(-beta|_beta)?$/.test(cluster)) {
    fail('MAINNET IS REFUSED. These scripts are devnet-only.');
  }
  if (cluster !== 'devnet') fail(`SOLANA_CLUSTER='${cluster}' is not devnet. Refusing.`);
  if (/mainnet/i.test(rpcUrl)) fail(`SOLANA_RPC_URL looks like mainnet: ${rpcUrl}`);

  return { cluster, rpcUrl };
}

export function requireEnv(name: string, hint: string): string {
  const v = process.env[name];
  if (!v) fail(`${name} is not set. ${hint}`);
  return v;
}

// ---------------------------------------------------------------- wallets

/**
 * Load a signer from a Solana CLI keypair JSON.
 *
 * The secret bytes stay inside the signer. Nothing in this module returns or
 * logs them; callers only ever see `signer.address`.
 */
export async function loadSigner(path: string, role: string): Promise<KeyPairSigner> {
  let parsed: unknown;
  try {
    parsed = JSON.parse(readFileSync(path, 'utf8'));
  } catch (e) {
    fail(`could not read the ${role} keypair at ${path}: ${e instanceof Error ? e.message : e}`);
  }
  if (!Array.isArray(parsed) || (parsed.length !== 64 && parsed.length !== 32)) {
    fail(`${path} is not a Solana keypair JSON (expected a 64-byte array)`);
  }
  return createKeyPairSignerFromBytes(Uint8Array.from(parsed as number[]));
}

// ---------------------------------------------------------------- rpc

export type Chain = {
  rpc: ReturnType<typeof createSolanaRpc>;
  rpcUrl: string;
  sendAndConfirm: ReturnType<typeof sendAndConfirmTransactionFactory>;
};

export function connect(rpcUrl: string): Chain {
  const rpc = createSolanaRpc(rpcUrl);
  // Confirmation rides the websocket subscription, so nothing here polls a
  // signature status in a loop and calls it confirmed.
  const rpcSubscriptions = createSolanaRpcSubscriptions(rpcUrl.replace(/^http/, 'ws'));
  return {
    rpc,
    rpcUrl,
    sendAndConfirm: sendAndConfirmTransactionFactory({ rpc, rpcSubscriptions }),
  };
}

/** Build, sign with `payer`, send, and confirm. Returns the signature. */
export async function submit(
  chain: Chain,
  payer: KeyPairSigner,
  instruction: Instruction,
): Promise<string> {
  const { value: latestBlockhash } = await chain.rpc.getLatestBlockhash().send();
  // Applied step by step rather than through `pipe` so the blockhash-lifetime
  // marker survives inference — `sendAndConfirm` requires it specifically.
  const base = createTransactionMessage({ version: 0 });
  const withPayer = setTransactionMessageFeePayerSigner(payer, base);
  const withIx = appendTransactionMessageInstruction(instruction, withPayer);
  const message = setTransactionMessageLifetimeUsingBlockhash(latestBlockhash, withIx);
  const signed = await signTransactionMessageWithSigners(message);
  // Signing widens the lifetime type back to the blockhash-or-durable-nonce
  // union. These scripts never use a durable nonce, so this narrows it back
  // with kit's own runtime assertion rather than a cast.
  assertIsTransactionWithBlockhashLifetime(signed);
  await chain.sendAndConfirm(signed, { commitment: 'confirmed' });
  return getSignatureFromTransaction(signed);
}

// ---------------------------------------------------------------- reads

function b64(data: string): Uint8Array {
  return new Uint8Array(getBase64Encoder().encode(data));
}

export async function fetchConfig(chain: Chain): Promise<Config | null> {
  const { config } = await programAddresses();
  const acc = await chain.rpc.getAccountInfo(config, { encoding: 'base64' }).send();
  if (!acc.value) return null;
  return decodeConfig(b64(acc.value.data[0]));
}

export async function fetchMiner(chain: Chain, owner: Address): Promise<Miner | null> {
  const pda = await minerPda(owner);
  const acc = await chain.rpc.getAccountInfo(pda, { encoding: 'base64' }).send();
  if (!acc.value) return null;
  return decodeMiner(b64(acc.value.data[0]));
}

export async function lamportsOf(chain: Chain, address: Address): Promise<bigint> {
  const r = await chain.rpc.getBalance(address).send();
  return BigInt(r.value);
}

/**
 * Vault lamports minus the rent-exempt reserve — the figure the program's own
 * fully-funded assertion uses.
 */
export async function vaultDistributable(chain: Chain, address: Address): Promise<bigint> {
  const acc = await chain.rpc.getAccountInfo(address, { encoding: 'base64' }).send();
  if (!acc.value) return 0n;
  const rent = await chain.rpc
    .getMinimumBalanceForRentExemption(BigInt(acc.value.space ?? 0n))
    .send();
  const balance = BigInt(acc.value.lamports);
  const reserve = BigInt(rent);
  return balance > reserve ? balance - reserve : 0n;
}

export async function chainTime(chain: Chain): Promise<bigint> {
  const slot = await chain.rpc.getSlot({ commitment: 'confirmed' }).send();
  return BigInt(await chain.rpc.getBlockTime(slot).send());
}

// ---------------------------------------------------------------- display

/** Exact lamports rendered as SOL. Integer maths only — no float anywhere. */
export function sol(lamports: bigint, decimals = 9): string {
  const neg = lamports < 0n;
  const abs = neg ? -lamports : lamports;
  const whole = abs / LAMPORTS_PER_SOL;
  const frac = (abs % LAMPORTS_PER_SOL).toString().padStart(9, '0').slice(0, decimals);
  return `${neg ? '-' : ''}${whole}.${frac}`;
}

export function row(label: string, value: unknown): void {
  console.log(`  ${label.padEnd(30)} ${String(value)}`);
}

export function header(title: string): void {
  console.log(`\n${'='.repeat(62)}\n ${title}\n${'='.repeat(62)}`);
}

export function section(title: string): void {
  console.log(`\n--> ${title}`);
}

export async function sleep(ms: number): Promise<void> {
  await new Promise((r) => setTimeout(r, ms));
}
