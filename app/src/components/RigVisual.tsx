/**
 * The grove: one cherry tree that grows with the seeds this wallet holds.
 *
 * Cosmetic only — it never affects a number. The drawing stage comes from the
 * real seed count (`rigTier`, finer steps early so the first few plantings
 * visibly change the tree); the stage *name* and the next-stage progress come
 * from the growth layer (`groveTier`), the same names the share card and
 * milestones use, so the screen never calls one grove two different things.
 *
 * Petals fall and the canopy sways only while the funded schedule is paying;
 * both stop under prefers-reduced-motion.
 */
import { formatCount } from '../lib/format';
import { groveTier, nextGroveTier } from '../lib/growth/milestones';

export type RigTier = {
  key: 'empty' | 'hand' | 'small' | 'industrial' | 'multi' | 'facility';
  /** Blossom clusters in the main canopy. */
  clusters: number;
  /** Trunk height in viewBox units. */
  trunk: number;
  /** Canopy scale. */
  scale: number;
};

export function rigTier(rigs: bigint): RigTier {
  if (rigs >= 500n) return { key: 'facility', clusters: 12, trunk: 52, scale: 1.15 };
  if (rigs >= 100n) return { key: 'multi', clusters: 12, trunk: 54, scale: 1.25 };
  if (rigs >= 25n) return { key: 'industrial', clusters: 8, trunk: 50, scale: 1.1 };
  if (rigs >= 5n) return { key: 'small', clusters: 6, trunk: 44, scale: 0.95 };
  if (rigs >= 1n) return { key: 'hand', clusters: 0, trunk: 20, scale: 0.6 };
  return { key: 'empty', clusters: 0, trunk: 0, scale: 0 };
}

const WIDTH = 260;
const HEIGHT = 150;
const GROUND = 134;

/**
 * Canopy clusters around the top of the trunk, [dx, dy, r], ordered so the
 * first few already read as a rounded crown and later ones fill it out.
 */
const CANOPY: readonly [number, number, number][] = [
  [0, -16, 17],
  [-22, -4, 14],
  [22, -4, 14],
  [0, -40, 13],
  [-18, -30, 13],
  [18, -30, 13],
  [-38, 8, 11],
  [38, 8, 11],
  [-34, -18, 11],
  [34, -18, 11],
  [-10, 4, 10],
  [12, 6, 10],
];

/** Petal start points (relative to canopy centre) and animation delays. */
const PETALS: readonly [number, number, number][] = [
  [-30, 6, 0],
  [18, 10, 1.6],
  [-8, 12, 3.1],
  [34, 0, 4.4],
  [-42, -4, 5.7],
];

function Tree({ x, tier, sway }: { x: number; tier: RigTier; sway: boolean }) {
  const top = GROUND - tier.trunk;
  const s = tier.scale;

  if (tier.key === 'hand') {
    // A seedling: one stem, two leaves, no blossom yet.
    return (
      <g>
        <path d={`M${x} ${GROUND} Q${x - 2} ${top + 6} ${x} ${top}`} stroke="var(--foliage)" strokeWidth="2" fill="none" />
        <ellipse cx={x - 6} cy={top + 2} rx="6" ry="3" fill="var(--foliage)" transform={`rotate(-25 ${x - 6} ${top + 2})`} />
        <ellipse cx={x + 6} cy={top} rx="6" ry="3" fill="var(--foliage)" transform={`rotate(25 ${x + 6} ${top})`} />
      </g>
    );
  }

  const clusters = CANOPY.slice(0, tier.clusters);
  const cy = top - 6 * s;
  return (
    <g>
      {/* Trunk, tapering, with two main limbs into the crown. */}
      <path
        d={`M${x - 6 * s} ${GROUND} Q${x - 3 * s} ${top + 14 * s} ${x - 2} ${top} L${x + 2} ${top} Q${x + 3 * s} ${top + 14 * s} ${x + 6 * s} ${GROUND} Z`}
        fill="var(--bark)"
      />
      <path
        d={`M${x} ${top + 8 * s} Q${x - 14 * s} ${top} ${x - 24 * s} ${cy - 2}`}
        stroke="var(--bark)"
        strokeWidth={3 * s}
        fill="none"
        strokeLinecap="round"
      />
      <path
        d={`M${x} ${top + 4 * s} Q${x + 14 * s} ${top - 4} ${x + 22 * s} ${cy - 8 * s}`}
        stroke="var(--bark)"
        strokeWidth={3 * s}
        fill="none"
        strokeLinecap="round"
      />
      <g className={sway ? 'tree-crown tree-crown--sway' : 'tree-crown'}>
        {clusters.map(([dx, dy, r], i) => (
          <g key={i}>
            <circle cx={x + dx * s} cy={cy + dy * s} r={r * s} fill="var(--blossom-dim)" />
            <circle cx={x + dx * s - 2 * s} cy={cy + dy * s - 2 * s} r={r * s * 0.72} fill="var(--blossom)" opacity="0.85" />
            <circle cx={x + dx * s - 4 * s} cy={cy + dy * s - 4 * s} r={r * s * 0.22} fill="var(--blossom-soft)" opacity="0.8" />
          </g>
        ))}
      </g>
    </g>
  );
}

export function RigVisual({ rigs: seeds, running }: { rigs: bigint; running: boolean }) {
  const tier = rigTier(seeds);
  const named = groveTier(seeds);
  const next = nextGroveTier(seeds);
  const stageName = named?.label ?? 'Bare soil';
  const cx = WIDTH / 2;
  const canopyY = GROUND - tier.trunk - 6 * tier.scale;
  const blooming = tier.clusters > 0;
  const growing = running && seeds > 0n;
  // Bare soil and a seedling have nothing above the ground worth a tall
  // frame: crop the empty sky so the card stays compact.
  const viewTop = tier.key === 'empty' || tier.key === 'hand' ? 70 : 0;

  // Progress from the current named stage to the next, in whole seeds.
  const from = named?.minSeeds ?? 0n;
  const progress =
    next === null ? 100 : Number(((seeds - from) * 100n) / (next.tier.minSeeds - from));

  return (
    <figure className="grove-visual" aria-label={`Your grove: ${stageName.toLowerCase()}, ${seeds.toString()} seeds`}>
      <svg viewBox={`0 ${viewTop} ${WIDTH} ${HEIGHT - viewTop}`} width="100%" role="img" aria-hidden="true" className="grove-svg">
        {/* Soft ground and a low hill the tree stands on. */}
        <ellipse cx={cx} cy={GROUND + 8} rx="112" ry="12" fill="var(--bg-raised)" />
        <line x1="12" y1={GROUND} x2={WIDTH - 12} y2={GROUND} stroke="var(--rule-strong)" strokeWidth="1" />

        {tier.key === 'empty' ? (
          // Bare soil: a planting mark waiting for the first seed.
          <g>
            <ellipse cx={cx} cy={GROUND - 1} rx="16" ry="4" fill="var(--bark)" opacity="0.7" />
            <circle cx={cx} cy={GROUND - 12} r="3.5" fill="none" stroke="var(--blossom-dim)" strokeDasharray="2 2" />
          </g>
        ) : null}

        {/* A full grove: two younger trees either side of the main one. */}
        {tier.key === 'facility' ? (
          <>
            <Tree x={cx - 82} tier={rigTier(5n)} sway={running} />
            <Tree x={cx + 82} tier={rigTier(5n)} sway={running} />
          </>
        ) : null}

        {tier.key !== 'empty' ? <Tree x={cx} tier={tier} sway={running} /> : null}

        {running && blooming
          ? PETALS.map(([dx, dy, delay], i) => (
              <ellipse
                key={i}
                className="petal"
                cx={cx + dx * tier.scale}
                cy={canopyY + dy * tier.scale}
                rx="2.6"
                ry="1.6"
                fill="var(--blossom-soft)"
                style={{ animationDelay: `${delay}s` }}
              />
            ))
          : null}
      </svg>
      <figcaption className="grove-caption">
        <div className="grove-caption-row">
          <span className="grove-stage" data-testid="grove-stage">
            {stageName}
            <span className="grove-seeds">
              {' '}
              · {formatCount(seeds)} seed{seeds === 1n ? '' : 's'}
            </span>
          </span>
          <span className={growing ? 'grove-state grove-state--live' : 'grove-state'}>
            {seeds === 0n ? 'Not planted' : growing ? 'Growing' : 'Resting'}
          </span>
        </div>
        {next ? (
          <div className="grove-next">
            <div className="grove-meter" aria-hidden="true">
              <span style={{ width: `${Math.max(progress, 2)}%` }} />
            </div>
            <span>
              {seeds === 0n
                ? 'Plant your first seed to sprout a seedling'
                : `${next.tier.label} at ${formatCount(next.tier.minSeeds)} seeds`}
            </span>
          </div>
        ) : null}
      </figcaption>
    </figure>
  );
}
