'use client';

/**
 * HARVEST CONFIRMATION — the one action that moves SOL out, made explicit.
 *
 * Claims are penalised by planting age, so the exact gross → penalty → net
 * split is shown before anything is signed. Confirming is a second, deliberate
 * tap; cancelling leaves everything untouched. All figures come from the same
 * `quoteClaim` port the note under the ore bin uses — nothing is re-derived
 * here.
 */
import { formatSol } from '../lib/format';
import type { ClaimQuote } from '../lib/program/penalty';
import { Readout } from './Readout';

export function ClaimConfirm({
  quote,
  busy,
  onConfirm,
  onCancel,
}: {
  quote: ClaimQuote;
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
          label={`Early-claim penalty · ${quote.penaltyBps / 100}%`}
          value={`− ${formatSol(quote.penalty, 6)}`}
          unit="SOL"
        />
        <div className="readout readout--total">
          <span className="readout-label">You receive</span>
          <span className="readout-value">
            {formatSol(quote.net, 6)}
            <span className="unit">SOL</span>
          </span>
        </div>
      </div>
      <p className="note">
        The penalty stays in the reward pool for other planters. Replanting has
        no penalty and grows your grove instead.
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
