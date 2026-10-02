/**
 * The ore-bin readout.
 *
 * Spec §10 asks for a claimable figure that ticks visibly. At a realistic
 * emission rate a single miner earns tens of lamports per second, so six
 * decimal places would sit still for half a minute at a time and the mine
 * would look stopped.
 *
 * The fix is precision, not theatre: the figure is shown to the full lamport,
 * with the last three digits de-emphasised. The headline reads as
 * `0.018291`, the tail moves every tick, and nothing is invented to make it
 * move — those digits are real lamports.
 */
import { formatSol } from '../lib/format';

export function OreBin({ lamports }: { lamports: bigint }) {
  const full = formatSol(lamports, 9);
  const head = full.slice(0, full.length - 3);
  const tail = full.slice(-3);

  return (
    <div className="readout">
      <span className="readout-label">Ready to harvest · unclaimed SOL</span>
      <span
        className="readout-value readout-value--hero"
        aria-label={`${full} SOL unclaimed`}
      >
        {head}
        <span className="lamport-tail">{tail}</span>
      </span>
    </div>
  );
}
