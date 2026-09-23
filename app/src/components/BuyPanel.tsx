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
import type { Config } from '../lib/program/accounts';
import { MAX_RIGS_PER_PURCHASE } from '../lib/program/constants';
import { quotePurchase, rigsAffordable } from '../lib/program/pricing';
import { aprBps, formatAprBps, yieldInputs } from '../lib/program/yield';
import { Readout } from './Readout';

const QUICK = [1n, 5n, 10n, 25n];

export function BuyPanel({
  config,
  chainTime,
  walletLamports,
  disabled,
  busy,
  onBuy,
}: {
  config: Config;
  chainTime: bigint;
  /** Spendable wallet balance, used only for the MAX button. */
  walletLamports: bigint | null;
  disabled: boolean;
  busy: boolean;
  onBuy: (count: bigint) => void;
}) {
  const [count, setCount] = useState<bigint>(10n);

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

  const clamp = (n: bigint) =>
    n < 1n ? 1n : n > MAX_RIGS_PER_PURCHASE ? MAX_RIGS_PER_PURCHASE : n;

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
    setCount(n > 0n ? n : 1n);
  };

  return (
    <section>
      <h2 className="section-title">Buy more hashpower</h2>

      <div className="stepper">
        <button
          type="button"
          onClick={() => setCount((c) => clamp(c - 1n))}
          aria-label="One fewer rig"
        >
          –
        </button>
        <input
          inputMode="numeric"
          pattern="[0-9]*"
          value={count.toString()}
          aria-label="Number of rigs"
          onChange={(e) => {
            const digits = e.target.value.replace(/\D/g, '');
            setCount(digits ? clamp(BigInt(digits)) : 1n);
          }}
        />
        <button
          type="button"
          onClick={() => setCount((c) => clamp(c + 1n))}
          aria-label="One more rig"
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
            onClick={() => setCount(n)}
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

      <button
        type="button"
        className="primary"
        style={{ width: '100%' }}
        disabled={disabled || busy || !quote}
        onClick={() => quote && onBuy(quote.count)}
      >
        {busy ? 'Hiring…' : `Hire ${count.toString()} rig${count === 1n ? '' : 's'}`}
      </button>

      <p className="note">
        Rigs are game production capacity, not a deposit. They are
        non-redeemable — you are buying hashpower, not staking principal you can
        withdraw later.
      </p>
    </section>
  );
}
