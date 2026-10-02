/**
 * Production custody audit — a pure evaluation of the program's authority
 * layout against the written custody policy (docs/backlog/CUSTODY.md).
 *
 * Read-only. It never proposes, signs or sends an authority change. It can
 * only see keys, not people: two "different" signer keys held by one person
 * pass every check here and are still operationally single-sig. The report
 * says so explicitly.
 */
import { isAddress, isOffCurveAddress, type Address } from '@solana/kit';

export type CustodyInput = {
  /** BPF upgrade authority from ProgramData; null = immutable (revoked). */
  upgradeAuthority: Address | null;
  configAuthority: Address;
  pendingAuthority: Address;
  rewardFunder: Address;
  feeRecipient: Address;
  founder: Address;
};

export type CustodyPolicy = {
  /** Expected multisig vault (PDA) for the upgrade authority, or 'revoked'. */
  expectedUpgradeAuthority: Address | 'revoked' | null;
  expectedConfigAuthority: Address | null;
  expectedRewardFunder: Address | null;
  expectedFeeRecipient: Address | null;
};

export type CustodyCheck = { level: 'PASS' | 'FAIL' | 'WARN'; name: string; detail: string };

const DEFAULT_PUBKEY = '11111111111111111111111111111111';

function kind(a: Address): 'pda' | 'key' {
  return isOffCurveAddress(a) ? 'pda' : 'key';
}

export function parsePolicyAddress(v: string | undefined): Address | null {
  const t = (v ?? '').trim();
  return t !== '' && isAddress(t) ? (t as Address) : null;
}

export function evaluateCustody(i: CustodyInput, p: CustodyPolicy): CustodyCheck[] {
  const out: CustodyCheck[] = [];
  const add = (level: CustodyCheck['level'], name: string, detail: string) => out.push({ level, name, detail });

  // Upgrade authority.
  if (i.upgradeAuthority === null) {
    add(p.expectedUpgradeAuthority === 'revoked' ? 'PASS' : 'WARN', 'upgrade authority', 'revoked — program is immutable (no forward fixes possible)');
  } else {
    if (p.expectedUpgradeAuthority === 'revoked') add('FAIL', 'upgrade authority', 'policy expects revoked, but an authority exists');
    else if (p.expectedUpgradeAuthority && i.upgradeAuthority !== p.expectedUpgradeAuthority) {
      add('FAIL', 'upgrade authority', `${i.upgradeAuthority} ≠ expected ${p.expectedUpgradeAuthority}`);
    } else if (!p.expectedUpgradeAuthority) add('WARN', 'upgrade authority', 'no expected value supplied');
    else add('PASS', 'upgrade authority', 'matches policy');
    if (kind(i.upgradeAuthority) === 'key') {
      add('FAIL', 'upgrade authority is a single key', 'on-curve address: one private key can upgrade the program. Production requires a multisig vault (PDA).');
    }
  }

  // Config authority.
  if (p.expectedConfigAuthority && i.configAuthority !== p.expectedConfigAuthority) {
    add('FAIL', 'config authority', `${i.configAuthority} ≠ expected ${p.expectedConfigAuthority}`);
  } else if (p.expectedConfigAuthority) add('PASS', 'config authority', 'matches policy');
  else add('WARN', 'config authority', 'no expected value supplied');
  if (kind(i.configAuthority) === 'key') {
    add('FAIL', 'config authority is a single key', 'can pause claims, change pricing, rotate funder/fee recipient alone');
  }
  if (i.pendingAuthority !== DEFAULT_PUBKEY) {
    add('WARN', 'pending authority', `a handover to ${i.pendingAuthority} is open — accept or cancel it deliberately`);
  } else add('PASS', 'pending authority', 'none');

  // Reward funder.
  if (p.expectedRewardFunder && i.rewardFunder !== p.expectedRewardFunder) {
    add('FAIL', 'reward funder', `${i.rewardFunder} ≠ expected ${p.expectedRewardFunder}`);
  } else if (p.expectedRewardFunder) add('PASS', 'reward funder', 'matches policy');
  if (i.rewardFunder === i.configAuthority) add('FAIL', 'funder separation', 'reward funder equals config authority');
  if (i.upgradeAuthority && i.rewardFunder === i.upgradeAuthority) add('FAIL', 'funder separation', 'reward funder equals upgrade authority');

  // Fee recipient.
  if (p.expectedFeeRecipient && i.feeRecipient !== p.expectedFeeRecipient) {
    add('FAIL', 'fee recipient', `${i.feeRecipient} ≠ expected ${p.expectedFeeRecipient}`);
  } else if (p.expectedFeeRecipient) add('PASS', 'fee recipient', 'matches policy');
  if (i.feeRecipient === i.configAuthority) add('WARN', 'fee separation', 'fee recipient equals config authority (authority would both trigger and receive)');
  if (i.feeRecipient === i.rewardFunder) add('FAIL', 'fee separation', 'fee recipient equals reward funder — fees must never become reward funding');

  if (i.feeRecipient === i.founder) {
    add('WARN', 'fee recipient is the founder activation wallet', 'acceptable only as a disclosed interim; production target is a cold destination or multisig vault');
  }

  // Founder activation wallet must not hold operational authority.
  for (const [name, a] of [
    ['upgrade authority', i.upgradeAuthority],
    ['config authority', i.configAuthority],
    ['reward funder', i.rewardFunder],
  ] as const) {
    if (a && a === i.founder) add('FAIL', 'founder separation', `founder activation wallet is also the ${name}`);
  }

  add(
    'WARN',
    'human separation (not checkable)',
    'Keys cannot show who holds them. A 2-of-3 where one person controls all three keys is single-sig. Confirm signer custody in the signed custody register.',
  );
  return out;
}

// ---------------------------------------------------------------- loader parsing
// BPF upgradeable loader state (bincode): u32 tag, then
//   2 = Program     { programdata_address: Pubkey }
//   3 = ProgramData { slot: u64, upgrade_authority: Option<Pubkey> }

export const BPF_UPGRADEABLE_LOADER = 'BPFLoaderUpgradeab1e11111111111111111111111';

function u32le(b: Uint8Array, o: number): number {
  return (b[o]! | (b[o + 1]! << 8) | (b[o + 2]! << 16) | (b[o + 3]! << 24)) >>> 0;
}

export function programDataAddressFrom(programAccountData: Uint8Array, encode: (b: Uint8Array) => Address): Address {
  if (programAccountData.length < 36 || u32le(programAccountData, 0) !== 2) {
    throw new Error('not an upgradeable Program account');
  }
  return encode(programAccountData.subarray(4, 36));
}

export function upgradeAuthorityFrom(
  programDataHeader: Uint8Array,
  encode: (b: Uint8Array) => Address,
): Address | null {
  if (programDataHeader.length < 13 || u32le(programDataHeader, 0) !== 3) {
    throw new Error('not a ProgramData account');
  }
  const flag = programDataHeader[12];
  if (flag === 0) return null;
  if (flag !== 1 || programDataHeader.length < 45) throw new Error('malformed upgrade authority');
  return encode(programDataHeader.subarray(13, 45));
}
