/**
 * Instruction builders.
 *
 * Account order and writability mirror the `#[derive(Accounts)]` structs in
 * `programs/sol-rigs/src/instructions/`. Getting either wrong is a runtime
 * failure with an opaque message, so the ordering is annotated against the
 * Rust struct it came from.
 */
import {
  AccountRole,
  getAddressEncoder,
  getU64Encoder,
  type Address,
  type Instruction,
} from '@solana/kit';

import { IX_DISCRIMINATOR, PROGRAM_ID, SYSTEM_PROGRAM_ID } from './constants';
import type { ProgramAddresses } from './pdas';

function ix(
  discriminator: Uint8Array,
  accounts: Instruction['accounts'],
  args?: Uint8Array,
): Instruction {
  const data = new Uint8Array(discriminator.length + (args?.length ?? 0));
  data.set(discriminator, 0);
  if (args) data.set(args, discriminator.length);
  return { programAddress: PROGRAM_ID, accounts, data };
}

/**
 * `buy_rigs(count, referrer)` — see `instructions/buy_rigs.rs`.
 *
 * Args: `u64 count` then Borsh `Option<Pubkey>` (`0` | `1 ‖ 32 bytes`).
 * Remaining accounts: `[identity…, skr_binding?, referrer_miner?,
 * referral_ledger?]` — the buyer's `.skr` / `.sns` proof (`[name]` direct,
 * `[name, nft_record, token_account]` wrapped or tokenized, or nothing),
 * then — for a `.skr` proof — the writable Day-1 binding PDA
 * `["skr_bind", name]` (required during Day 1, ignored otherwise), then the
 * referrer's miner PDA iff a referrer is named, then the writable referral
 * ledger `["referral", referrer]` iff the position is referred (bound
 * earlier, or bound by this purchase) — the program reserves the referrer's
 * commission there (architecture §7a). The program verifies all of it; the
 * client choosing them grants nothing.
 */
export function buyRigsInstruction(
  owner: Address,
  miner: Address,
  pdas: ProgramAddresses,
  count: bigint,
  opts: {
    identity?: readonly Address[];
    skrBinding?: Address | null;
    referrer?: { wallet: Address; miner: Address } | null;
    /** `["referral", referrer]` of the position's (new or bound) referrer. */
    referralLedger?: Address | null;
  } = {},
): Instruction {
  const countBytes = getU64Encoder().encode(count);
  const referrer = opts.referrer ?? null;
  const args = new Uint8Array(8 + 1 + (referrer ? 32 : 0));
  args.set(countBytes, 0);
  args[8] = referrer ? 1 : 0;
  if (referrer) args.set(getAddressEncoder().encode(referrer.wallet), 9);
  return ix(
    IX_DISCRIMINATOR.buyRigs,
    [
      { address: owner, role: AccountRole.WRITABLE_SIGNER },
      { address: pdas.config, role: AccountRole.WRITABLE },
      { address: miner, role: AccountRole.WRITABLE },
      { address: pdas.capitalVault, role: AccountRole.WRITABLE },
      { address: pdas.feeVault, role: AccountRole.WRITABLE },
      // Read-only on purpose: spec §3. A purchase cannot reach reward backing.
      { address: pdas.rewardVault, role: AccountRole.READONLY },
      { address: SYSTEM_PROGRAM_ID, role: AccountRole.READONLY },
      // remaining accounts
      ...(opts.identity ?? []).map((address) => ({ address, role: AccountRole.READONLY })),
      // Writable: the first Day-1 use of a domain creates its binding.
      ...(opts.skrBinding ? [{ address: opts.skrBinding, role: AccountRole.WRITABLE }] : []),
      ...(referrer ? [{ address: referrer.miner, role: AccountRole.READONLY }] : []),
      ...(opts.referralLedger
        ? [{ address: opts.referralLedger, role: AccountRole.WRITABLE }]
        : []),
    ],
    args,
  );
}

/**
 * `claim()` — see `instructions/claim.rs`. The fee vault receives the
 * early-claim fee's 60% treasury share (architecture §7a item 5).
 */
export function claimInstruction(
  owner: Address,
  miner: Address,
  pdas: ProgramAddresses,
): Instruction {
  return ix(IX_DISCRIMINATOR.claim, [
    { address: owner, role: AccountRole.WRITABLE_SIGNER },
    { address: pdas.config, role: AccountRole.WRITABLE },
    { address: miner, role: AccountRole.WRITABLE },
    { address: pdas.rewardVault, role: AccountRole.WRITABLE },
    { address: pdas.feeVault, role: AccountRole.WRITABLE },
  ]);
}

/**
 * `compound()` — see `instructions/compound.rs`. Remaining accounts:
 * `[referral_ledger]` iff the position has a referrer.
 */
export function compoundInstruction(
  owner: Address,
  miner: Address,
  pdas: ProgramAddresses,
  opts: { referralLedger?: Address | null } = {},
): Instruction {
  return ix(IX_DISCRIMINATOR.compound, [
    { address: owner, role: AccountRole.WRITABLE_SIGNER },
    { address: pdas.config, role: AccountRole.WRITABLE },
    { address: miner, role: AccountRole.WRITABLE },
    { address: pdas.rewardVault, role: AccountRole.WRITABLE },
    { address: pdas.capitalVault, role: AccountRole.WRITABLE },
    { address: pdas.feeVault, role: AccountRole.WRITABLE },
    { address: SYSTEM_PROGRAM_ID, role: AccountRole.READONLY },
    ...(opts.referralLedger
      ? [{ address: opts.referralLedger, role: AccountRole.WRITABLE }]
      : []),
  ]);
}

/**
 * `claim_commission()` — see `instructions/admin.rs::ClaimCommission`. Pays
 * the referrer's whole unpaid commission from the fee vault.
 */
export function claimCommissionInstruction(
  referrer: Address,
  referralLedger: Address,
  pdas: ProgramAddresses,
): Instruction {
  return ix(IX_DISCRIMINATOR.claimCommission, [
    { address: referrer, role: AccountRole.WRITABLE_SIGNER },
    { address: pdas.config, role: AccountRole.WRITABLE },
    { address: referralLedger, role: AccountRole.WRITABLE },
    { address: pdas.feeVault, role: AccountRole.WRITABLE },
  ]);
}

/**
 * `apply_fee_change()` — permissionless once the 72-hour timelock has
 * elapsed. See `instructions/admin.rs::ApplyFeeChange`.
 */
export function applyFeeChangeInstruction(caller: Address, pdas: ProgramAddresses): Instruction {
  return ix(IX_DISCRIMINATOR.applyFeeChange, [
    { address: caller, role: AccountRole.READONLY_SIGNER },
    { address: pdas.config, role: AccountRole.WRITABLE },
  ]);
}
