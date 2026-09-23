/**
 * The blink render target: a pure function of the snapshot params.
 *
 * Shared blinks are read-only snapshots — no callback buttons, no bot behind
 * them. Nothing here reads the chain, and nothing here is interactive.
 */
import { PRODUCT_NAME } from '../lib/brand';
import { formatCount, formatSol } from '../lib/format';

export type ResultParams = {
  kind?: string;
  rigs?: string;
  compounded?: string;
  claimed?: string;
};

function safeBigInt(v: string | undefined): bigint | null {
  if (!v || !/^\d{1,30}$/.test(v)) return null;
  try {
    return BigInt(v);
  } catch {
    return null;
  }
}

export function ResultCard({ params }: { params: ResultParams }) {
  const rigs = safeBigInt(params.rigs);
  const compounded = safeBigInt(params.compounded);
  const claimed = safeBigInt(params.claimed);

  const headline =
    compounded !== null
      ? `+${formatCount(compounded)} RIGS`
      : claimed !== null
        ? `${formatSol(claimed, 4)} SOL CLAIMED`
        : PRODUCT_NAME;

  return (
    <main
      className="shell"
      style={{ paddingTop: 18, paddingBottom: 18, maxWidth: 480 }}
    >
      <div className="panel">
        <p className="section-title" style={{ marginBottom: 6 }}>
          {PRODUCT_NAME} · MINE REPORT
        </p>
        <p
          style={{
            margin: '2px 0 10px',
            fontSize: 28,
            color: 'var(--blossom)',
            letterSpacing: '-0.01em',
          }}
        >
          {headline}
        </p>
        <div className="readout">
          <span className="readout-label">Hashpower</span>
          <span className="readout-value">
            {rigs !== null ? formatCount(rigs) : '—'}
            <span className="unit">RIGS</span>
          </span>
        </div>
        <p className="note" style={{ marginTop: 10 }}>
          Mining simulation with SOL-denominated rewards paid from a funded
          reward vault. Read-only snapshot.
        </p>
      </div>
    </main>
  );
}
