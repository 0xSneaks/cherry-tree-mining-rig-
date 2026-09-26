'use client';

/**
 * BUY MORE HASHPOWER.
 *
 * The quote shown here is produced by the same pricing function the program
 * runs (`quotePurchase`, a port cross-checked against the Rust engine in
 * `app/tests/pricing.test.ts`). Spec §12.29: the total on screen is exactly
 * what gets debited. Nothing about the fee is hidden or rounded for display.
 */
import { useMemo, useState } from 'react';

import { formatSol } from '../lib/format';
import { buyResetDisclosure } from '../lib/minerCadence';
import type { Config } from '../lib/program/accounts';
import { MAX_RIGS_PER_PURCHASE } from '../lib/program/constants';
import { quotePurchase, rigsAffordable } from '../lib/program/pricing';
import { aprBps, formatAprBps, yieldInputs } from '../lib/program/yield';
import { projectionsAllowed } from '../lib/growth/runway';
import { Readout } from './Readout';

const QUICK = [1n, 5n, 10n, 25n];

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
  const [count, setCount] = useState<bigint>(10n);
  const [pendingCount, setPendingCount] = useState<bigint | null>(null);
  const emissionLive = projectionsAllowed(config, chainTime);
  const resetDisclosure = buyResetDisclosure(hasPosition, accruedRewardLamports);

  const quote = useMemo(() => {
    try {
      return quotePurchase(
        count,
        config.rigPriceLamports,
        BigInt(config.purchaseFeeBps),
      );
    } catch {
      return null;
    }
  }, [count, config.rigPriceLamports, config.purchaseFeeBps]);

  const maxAllowed =
    genesisRemaining === null
      ? MAX_RIGS_PER_PURCHASE
      : genesisRemaining < MAX_RIGS_PER_PURCHASE
        ? genesisRemaining
        : MAX_RIGS_PER_PURCHASE;
  const clamp = (n: bigint) =>
    maxAllowed === 0n ? 1n : n < 1n ? 1n : n > maxAllowed ? maxAllowed : n;

  const max = () => {
    if (walletLamports === null) return;
    // Leave a little behind for the transaction fee and the miner PDA rent.
    const reserve = 5_000_000n;
    const budget = walletLamports > reserve ? walletLamports - reserve : 0n;
    const n = rigsAffordable(
      budget,
      config.rigPriceLamports,
      BigInt(config.purchaseFeeBps),
    );
    const allowed = n > maxAllowed ? maxAllowed : n;
    setCount(allowed > 0n ? allowed : 1n);
    setPendingCount(null);
  };

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

      <div className="stepper">
        <button
          type="button"
          onClick={() => {
            setCount((c) => clamp(c - 1n));
            setPendingCount(null);
          }}
          aria-label="One fewer seed"
        >
          –
        </button>
        <input
          inputMode="numeric"
          pattern="[0-9]*"
          value={count.toString()}
          aria-label="Number of seeds"
          onChange={(e) => {
            const digits = e.target.value.replace(/\D/g, '');
            setCount(digits ? clamp(BigInt(digits)) : 1n);
            setPendingCount(null);
          }}
        />
        <button
          type="button"
          onClick={() => {
            setCount((c) => clamp(c + 1n));
            setPendingCount(null);
          }}
          aria-label="One more seed"
        >
          +
        </button>
      </div>

      <div className="quick">
        {QUICK.map((n) => (
          <button
            key={n.toString()}
            type="button"
            aria-pressed={count === n}
            onClick={() => {
              setCount(clamp(n));
              setPendingCount(null);
            }}
          >
            {n.toString()}
          </button>
        ))}
        <button type="button" onClick={max} disabled={walletLamports === null}>
          Max
        </button>
      </div>

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
        <span>
          {resetDisclosure}
          {hasPosition && accruedRewardLamports > 0n
            ? ` Accrued now: ${formatSol(accruedRewardLamports, 6)} SOL.`
            : null}
        </span>
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
        {busy ? 'Planting…' : `Plant ${count.toString()} seed${count === 1n ? '' : 's'} · review`}
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
              {busy ? 'Planting…' : `Confirm plant · ${formatSol(quote.totalLamports, 4)} SOL`}
            </button>
          </div>
        </div>
      ) : null}

      {error ? <div className="banner banner--alert">{error}</div> : null}
    </section>
  );
}
