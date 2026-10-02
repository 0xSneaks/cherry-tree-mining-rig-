'use client';

/**
 * PLANT SEEDS — SOL first.
 *
 * The planter says how much SOL to spend; the seed count is the largest whole
 * number the program's own price admits within it (`rigsAffordable`, the same
 * function the program mirrors). The instruction still carries a seed count —
 * nothing about the transaction changes, only which number the person types.
 *
 * The quote shown here is produced by the same pricing function the program
 * runs (`quotePurchase`, a port cross-checked against the Rust engine in
 * `app/tests/pricing.test.ts`). Spec §12.29: the total on screen is exactly
 * what gets debited. Nothing about the fee is hidden or rounded for display.
 */
import { useMemo, useState } from 'react';

import { formatCount, formatSol, parseSolInput } from '../lib/format';
import { buyResetDisclosure, showsTopUpWarning } from '../lib/minerCadence';
import { EARLY_CLAIM_DISCLOSURE } from '../lib/program/penalty';
import type { Config } from '../lib/program/accounts';
import { MAX_RIGS_PER_PURCHASE } from '../lib/program/constants';
import { quotePurchase, rigsAffordable } from '../lib/program/pricing';
import { aprBps, formatAprBps, yieldInputs } from '../lib/program/yield';
import { projectionsAllowed } from '../lib/growth/runway';
import { Readout } from './Readout';

/** Quick amounts, in SOL. The seed count follows from the program's price. */
const QUICK_SOL = ['0.1', '0.25', '0.5', '1'];
const DEFAULT_SOL = '0.1';

/** `0.103000000` → `0.103`: a lamport amount as a person would type it. */
function solInputText(lamports: bigint): string {
  return formatSol(lamports, 9).replace(/\.?0+$/, '');
}

export function BuyPanel({
  config,
  chainTime,
  walletLamports,
  disabled,
  genesisRemaining = null,
  closedReason = null,
  referralNote = null,
  officialFallback = false,
  hasPosition,
  accruedRewardLamports,
  busy,
  onBuy,
  error = null,
}: {
  config: Config;
  chainTime: bigint;
  /** Spendable wallet balance, used only for the MAX button. */
  walletLamports: bigint | null;
  disabled: boolean;
  /** Remaining cumulative fresh buys during Genesis; null once Public. */
  genesisRemaining?: bigint | null;
  /** Why planting is closed, when the launch gate is what closes it. */
  closedReason?: string | null;
  /** Pre-signing disclosure of what referral (if any) this planting binds. */
  referralNote?: string | null;
  /** True when the referral shown is the official Cherry Tree fallback. */
  officialFallback?: boolean;
  hasPosition: boolean;
  accruedRewardLamports: bigint;
  busy: boolean;
  onBuy: (count: bigint) => void;
  /** The last failed planting, in product language. */
  error?: string | null;
}) {
  const [solText, setSolText] = useState<string>(DEFAULT_SOL);
  const [pendingCount, setPendingCount] = useState<bigint | null>(null);
  const emissionLive = projectionsAllowed(config, chainTime);
  const resetDisclosure = buyResetDisclosure(hasPosition, accruedRewardLamports);
  const topUpWarning = showsTopUpWarning(hasPosition, accruedRewardLamports);

  const maxAllowed =
    genesisRemaining === null
      ? MAX_RIGS_PER_PURCHASE
      : genesisRemaining < MAX_RIGS_PER_PURCHASE
        ? genesisRemaining
        : MAX_RIGS_PER_PURCHASE;

  const budget = parseSolInput(solText);
  let affordable = 0n;
  try {
    if (budget !== null) {
      affordable = rigsAffordable(budget, config.rigPriceLamports, BigInt(config.purchaseFeeBps));
    }
  } catch {
    affordable = 0n;
  }
  const count = affordable > maxAllowed ? maxAllowed : affordable;
  const capped = affordable > maxAllowed && maxAllowed > 0n;

  const quote = useMemo(() => {
    if (count <= 0n) return null;
    try {
      return quotePurchase(count, config.rigPriceLamports, BigInt(config.purchaseFeeBps));
    } catch {
      return null;
    }
  }, [count, config.rigPriceLamports, config.purchaseFeeBps]);

  // The smallest planting there is: one seed, fee included.
  let oneSeed: bigint | null = null;
  try {
    oneSeed = quotePurchase(1n, config.rigPriceLamports, BigInt(config.purchaseFeeBps)).totalLamports;
  } catch {
    oneSeed = null;
  }

  const setAmount = (text: string) => {
    setSolText(text);
    setPendingCount(null);
  };

  const max = () => {
    if (walletLamports === null) return;
    // Leave a little behind for the transaction fee and the miner PDA rent.
    const reserve = 5_000_000n;
    const available = walletLamports > reserve ? walletLamports - reserve : 0n;
    const n = rigsAffordable(available, config.rigPriceLamports, BigInt(config.purchaseFeeBps));
    const allowed = n > maxAllowed ? maxAllowed : n;
    const spend =
      allowed > 0n
        ? quotePurchase(allowed, config.rigPriceLamports, BigInt(config.purchaseFeeBps)).totalLamports
        : 0n;
    setAmount(solInputText(spend));
  };

  const overBalance = quote !== null && walletLamports !== null && quote.totalLamports > walletLamports;
  const unspent = budget !== null && quote ? budget - quote.totalLamports : 0n;

  return (
    <section className="plant" data-testid="plant-panel">
      <h2 className="section-title">Plant seeds</h2>

      {/* What stops or changes this planting comes before the controls. */}
      {closedReason ? <p className="banner banner--alert">{closedReason}</p> : null}
      {!emissionLive ? (
        <p className="banner banner--alert" data-testid="buy-dormant-note">
          No funded emission right now. Seeds planted now earn nothing until a new
          schedule is separately funded — planting does not restart emission, and
          no refill is promised.
        </p>
      ) : null}

      <label className="sol-field">
        <span className="sol-field-label">Amount to plant</span>
        <span className="sol-field-input">
          <input
            inputMode="decimal"
            autoComplete="off"
            value={solText}
            aria-label="Amount in SOL"
            aria-invalid={budget === null}
            onChange={(e) => setAmount(e.target.value.replace(',', '.').replace(/[^\d.]/g, ''))}
          />
          <span className="sol-field-unit">SOL</span>
        </span>
      </label>

      <div className="quick">
        {QUICK_SOL.map((sol) => (
          <button
            key={sol}
            type="button"
            aria-pressed={solText === sol}
            aria-label={`${sol} SOL`}
            onClick={() => setAmount(sol)}
          >
            {sol}
          </button>
        ))}
        <button type="button" onClick={max} disabled={walletLamports === null}>
          Max
        </button>
      </div>

      <p className="sol-hint" data-testid="plant-seeds-preview">
        {quote ? (
          <>
            You plant <strong>{formatCount(quote.count)} seed{quote.count === 1n ? '' : 's'}</strong>
            {' '}at {formatSol(config.rigPriceLamports, 4)} SOL each
            {unspent > 0n ? ` · ${formatSol(unspent, 4)} SOL stays in your wallet` : ''}.
          </>
        ) : budget === null ? (
          'Enter an amount in SOL.'
        ) : oneSeed !== null ? (
          `One seed needs at least ${formatSol(oneSeed, 4)} SOL, fee included.`
        ) : (
          '—'
        )}
      </p>
      {capped ? (
        <p className="note">
          Capped at {formatCount(maxAllowed)} seeds per planting
          {genesisRemaining !== null ? ' (Genesis allowance)' : ''}.
        </p>
      ) : null}
      {overBalance ? (
        <p className="note" data-testid="over-balance">
          That is more than this wallet holds ({formatSol(walletLamports ?? 0n, 4)} SOL).
        </p>
      ) : null}

      <div className="quote">
        <Readout label="Cost" value={quote ? formatSol(quote.capitalLamports, 4) : '—'} unit="SOL" />
        <Readout
          label={`Fee · ${(config.purchaseFeeBps / 100).toFixed(2)}%`}
          value={quote ? formatSol(quote.feeLamports, 4) : '—'}
          unit="SOL"
        />
        <div className="readout readout--total">
          <span className="readout-label">Total</span>
          <span className="readout-value">
            {quote ? formatSol(quote.totalLamports, 4) : '—'}
            <span className="unit">SOL</span>
          </span>
        </div>
        {/* The rate these rigs would earn, after the dilution they cause. */}
        <Readout
          label="APR after planting"
          value={quote ? formatAprBps(aprBps(yieldInputs(config, chainTime), quote.count)) : '—'}
        />
      </div>

      {/* The essential rules, read once, directly above the button. */}
      <div className="buy-reset-warning" data-testid="buy-reset-warning">
        <strong>Before you plant</strong>
        {topUpWarning ? (
          <span data-testid="top-up-warning">
            <strong>{resetDisclosure}</strong>
          </span>
        ) : null}
        <span data-testid="early-claim-disclosure">{EARLY_CLAIM_DISCLOSURE}</span>
        <span>
          Seeds are game production capacity, not a deposit. They are
          non-redeemable — you are planting capacity, not staking principal you
          can withdraw later.
        </span>
      </div>

      {referralNote ? (
        <p
          className={officialFallback ? 'banner' : 'note'}
          data-testid="referral-note"
          data-official-fallback={officialFallback ? 'true' : 'false'}
        >
          {referralNote}
        </p>
      ) : null}

      <button
        type="button"
        className="primary cta"
        style={{ width: '100%' }}
        disabled={disabled || busy || !quote || count > maxAllowed}
        onClick={() => quote && setPendingCount(quote.count)}
      >
        {busy
          ? 'Planting…'
          : quote
            ? `Review · ${formatSol(quote.totalLamports, 4)} SOL for ${formatCount(quote.count)} seed${quote.count === 1n ? '' : 's'}`
            : 'Plant seeds'}
      </button>

      {pendingCount !== null && quote ? (
        <div className="confirm-sheet" data-testid="buy-confirm" aria-label="Confirm fresh planting">
          <h3 className="section-title">Confirm fresh planting</h3>
          <p className="note">{resetDisclosure}</p>
          <div className="actions">
            <button type="button" onClick={() => setPendingCount(null)} disabled={busy}>
              Not now
            </button>
            <button
              type="button"
              className="primary"
              disabled={disabled || busy || pendingCount > maxAllowed}
              onClick={() => {
                const confirmedCount = pendingCount;
                setPendingCount(null);
                onBuy(confirmedCount);
              }}
            >
              {busy
                ? 'Planting…'
                : `Confirm plant · ${formatSol(quote.totalLamports, 4)} SOL · ${formatCount(pendingCount)} seed${pendingCount === 1n ? '' : 's'}`}
            </button>
          </div>
        </div>
      ) : null}

      {error ? <div className="banner banner--alert">{error}</div> : null}
    </section>
  );
}
