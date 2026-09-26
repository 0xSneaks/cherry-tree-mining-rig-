/**
 * The grove: one cherry tree that grows with the seeds this wallet holds.
 *
 * Cosmetic only — it never affects a number. The drawing stage comes from the
 * real seed count (`rigTier`, finer steps early so the first few plantings
 * visibly change the tree); the stage *name* and the next-stage progress come
 * from the growth layer (`groveTier`), the same names the share card and
 * milestones use, so the screen never calls one grove two different things.
 *
 * Bare soil and the seedling are drawn inline; from a sapling up the tree is
 * the painted illustration (`public/grove/cherry-tree.webp`, transparent),
 * sized by stage, and a full grove adds two younger trees behind it.
 *
 * Petals fall and the crown sways only while the funded schedule is paying;
 * both stop under prefers-reduced-motion.
 */
import { formatCount } from '../lib/format';
import { groveTier, nextGroveTier } from '../lib/growth/milestones';

export type RigTier = {
  key: 'empty' | 'hand' | 'small' | 'industrial' | 'multi' | 'facility';
  /** Width of the painted tree as a share of the frame; 0 = drawn inline. */
  art: number;
};

export function rigTier(rigs: bigint): RigTier {
  if (rigs >= 500n) return { key: 'facility', art: 0.9 };
  if (rigs >= 100n) return { key: 'multi', art: 0.94 };
  if (rigs >= 25n) return { key: 'industrial', art: 0.74 };
  if (rigs >= 5n) return { key: 'small', art: 0.52 };
  if (rigs >= 1n) return { key: 'hand', art: 0 };
  return { key: 'empty', art: 0 };
}

/** Height ÷ width of `cherry-tree.webp` (720 × 496). */
const ART_ASPECT = 496 / 720;

const TREE_SRC = `${process.env.NEXT_PUBLIC_BASE_PATH ?? ''}/grove/cherry-tree.webp`;

/** Falling petals: horizontal start (% of frame), start height (%), delay (s). */
const PETALS: readonly [number, number, number][] = [
  [30, 30, 0],
  [62, 22, 1.4],
  [44, 40, 2.9],
  [72, 38, 4.1],
  [24, 48, 5.3],
  [55, 18, 6.2],
];

/**
 * The painted tree. A plain <img>: the asset is already a sized, transparent
 * WebP, and the static export serves images unoptimised anyway.
 */
function TreeArt({ className, width }: { className: string; width?: string }) {
  // eslint-disable-next-line @next/next/no-img-element
  return <img className={className} src={TREE_SRC} alt="" aria-hidden="true" draggable={false} style={width ? { width } : undefined} />;
}

/** Bare soil or a seedling: small enough to draw inline. */
function Sprout({ seedling }: { seedling: boolean }) {
  return (
    <svg viewBox="0 0 260 80" width="100%" role="img" aria-hidden="true" className="grove-svg">
      <ellipse cx="130" cy="72" rx="112" ry="10" fill="var(--bg-raised)" />
      <line x1="12" y1="64" x2="248" y2="64" stroke="var(--rule-strong)" strokeWidth="1" />
      {seedling ? (
        <g>
          <path d="M130 64 Q127 52 130 42" stroke="var(--foliage)" strokeWidth="2.4" fill="none" />
          <ellipse cx="122" cy="45" rx="8" ry="3.6" fill="var(--foliage)" transform="rotate(-25 122 45)" />
          <ellipse cx="138" cy="42" rx="8" ry="3.6" fill="var(--foliage)" transform="rotate(25 138 42)" />
          <circle cx="130" cy="40" r="3" fill="var(--blossom-soft)" />
        </g>
      ) : (
        <g>
          <ellipse cx="130" cy="63" rx="16" ry="4" fill="var(--bark)" opacity="0.7" />
          <circle cx="130" cy="52" r="3.5" fill="none" stroke="var(--blossom-dim)" strokeDasharray="2 2" />
        </g>
      )}
    </svg>
  );
}

export function RigVisual({ rigs: seeds, running }: { rigs: bigint; running: boolean }) {
  const tier = rigTier(seeds);
  const named = groveTier(seeds);
  const next = nextGroveTier(seeds);
  const stageName = named?.label ?? 'Bare soil';
  const growing = running && seeds > 0n;

  // Progress from the current named stage to the next, in whole seeds.
  const from = named?.minSeeds ?? 0n;
  const progress =
    next === null ? 100 : Number(((seeds - from) * 100n) / (next.tier.minSeeds - from));

  return (
    <figure className="grove-visual" aria-label={`Your grove: ${stageName.toLowerCase()}, ${seeds.toString()} seeds`}>
      {tier.art === 0 ? (
        <Sprout seedling={tier.key === 'hand'} />
      ) : (
        <div
          className="grove-art"
          data-stage={tier.key}
          // Frame fits the tree: its height is the art's (720×496) plus the ground.
          style={{ aspectRatio: `1 / ${(tier.art * ART_ASPECT + 0.07).toFixed(3)}` }}
        >
          <span className="grove-ground" aria-hidden="true" />
          {tier.key === 'facility' ? (
            <>
              <TreeArt className="grove-tree grove-tree--left" />
              <TreeArt className="grove-tree grove-tree--right" />
            </>
          ) : null}
          <TreeArt
            className={growing ? 'grove-tree grove-tree--main grove-tree--sway' : 'grove-tree grove-tree--main'}
            width={`${tier.art * 100}%`}
          />
          {growing
            ? PETALS.map(([left, top, delay], i) => (
                <span
                  key={i}
                  className="grove-petal"
                  aria-hidden="true"
                  style={{ left: `${left}%`, top: `${top}%`, animationDelay: `${delay}s` }}
                />
              ))
            : null}
        </div>
      )}
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
