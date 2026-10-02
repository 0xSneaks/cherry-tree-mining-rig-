'use client';

/**
 * HARVEST CONFIRMATION — the one action that moves SOL out, made explicit.
 *
 * Claims within 72h of the latest fresh purchase pay the early-claim fee
 * (architecture §7a item 5), so the exact gross → fee → net split, and where
 * the fee goes (60% Protocol Treasury / 40% other planters), is shown before
 * anything is signed. Confirming is a second, deliberate tap; cancelling
 * leaves everything untouched. All figures come from the same `quoteClaim`
 * port the note under the ore bin uses — nothing is re-derived here.
 */
import { formatSol } from '../lib/format';
import type { ClaimQuote } from '../lib/program/penalty';
import { Readout } from './Readout';

export function ClaimConfirm({
  quote,
  compoundFeeBps,
  busy,
  onConfirm,
  onCancel,
}: {
  quote: ClaimQuote;
  /** The current compound (replant) fee, shown so it is never mistaken for free. */
  compoundFeeBps: number;
  busy: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <section className="confirm-sheet" data-testid="claim-confirm" aria-label="Confirm harvest">
      <h2 className="section-title">Confirm harvest</h2>
      <div className="panel">
        <Readout label="Gross harvest" value={formatSol(quote.gross, 6)} unit="SOL" />
        <Readout
          label={`Early-claim fee · ${quote.penaltyBps / 100}%`}
          value={`− ${formatSol(quote.penalty, 6)}`}
          unit="SOL"
        />
        {quote.penalty > 0n ? (
          <>
            <Readout
              label="· to the Protocol Treasury (60%)"
              value={formatSol(quote.penaltyTreasury, 6)}
              unit="SOL"
            />
            <Readout
              label="· redistributed to other planters (40%)"
              value={formatSol(quote.penaltyGrove, 6)}
              unit="SOL"
            />
          </>
        ) : null}
        <div className="readout readout--total">
          <span className="readout-label">You receive</span>
          <span className="readout-value">
            {formatSol(quote.net, 6)}
            <span className="unit">SOL</span>
          </span>
        </div>
      </div>
      <p className="note">
        The early-claim fee applies within 72 hours of your most recent seed
        purchase: 30% under 24h, 20% to 48h, 10% to 72h, then 0%. 60% of it goes
        to the Protocol Treasury and 40% is redistributed to the other planters.
        Replanting does not pay the early-claim fee and does not restart the
        timer, but the {compoundFeeBps / 100}% replant (compound) fee applies to the seeds it
        plants.
      </p>
      <div className="actions">
        <button type="button" onClick={onCancel} disabled={busy}>
          Not now
        </button>
        <button type="button" className="primary" onClick={onConfirm} disabled={busy}>
          {busy ? 'Claiming…' : 'Confirm harvest'}
        </button>
      </div>
    </section>
  );
}
