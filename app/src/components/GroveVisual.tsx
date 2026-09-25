/**
 * The grove: a cherry tree that grows with the real seed count.
 *
 * Deterministic tier from the seed count, cosmetic only — it never affects a
 * number. Individual seeds are deliberately not drawn: at 500 seeds that is a
 * pointless render, and a tier reads faster anyway. Blossom placement comes
 * from a fixed-seed PRNG, so the same tier always draws the same tree.
 *
 * The canvas is sized to its content so a seedling does not sit in a field of
 * empty sky.
 */
export type GroveTier = {
  key: string;
  label: string;
  /** Trunk height in viewBox units; 0 means nothing has sprouted. */
  trunk: number;
  /** Branch pairs off the trunk. */
  branches: number;
  /** Blossom clusters in the canopy. */
  blossoms: number;
  /** Smaller trees either side of the main one. */
  companions: boolean;
};

export function groveTier(seeds: bigint): GroveTier {
  if (seeds >= 500n)
    return { key: 'grove', label: 'CHERRY GROVE', trunk: 92, branches: 3, blossoms: 40, companions: true };
  if (seeds >= 100n)
    return { key: 'tree', label: 'CHERRY TREE', trunk: 92, branches: 3, blossoms: 40, companions: false };
  if (seeds >= 25n)
    return { key: 'young', label: 'YOUNG TREE', trunk: 68, branches: 2, blossoms: 22, companions: false };
  if (seeds >= 5n)
    return { key: 'sapling', label: 'SAPLING', trunk: 44, branches: 1, blossoms: 9, companions: false };
  if (seeds >= 1n)
    return { key: 'seedling', label: 'SEEDLING', trunk: 22, branches: 0, blossoms: 1, companions: false };
  return { key: 'empty', label: 'BARE SOIL', trunk: 0, branches: 0, blossoms: 0, companions: false };
}

// A narrow viewBox on purpose: the SVG scales to the column width, so fewer
// user units means a larger tree on a 390px phone.
const WIDTH = 300;
const CENTRE = WIDTH / 2;
const TOP_MARGIN = 26;
const ROOT_DEPTH = 22;

/** mulberry32 — tiny, fixed-seed, so every render of a tier is identical. */
function prng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Five petals round a pale centre. */
function Blossom({ x, y, r, lit }: { x: number; y: number; r: number; lit: boolean }) {
  const petals = [0, 72, 144, 216, 288].map((deg) => {
    const rad = (deg * Math.PI) / 180;
    return { cx: x + Math.cos(rad) * r * 0.62, cy: y + Math.sin(rad) * r * 0.62 };
  });
  return (
    <g>
      {petals.map((p, i) => (
        <circle
          key={i}
          cx={p.cx}
          cy={p.cy}
          r={r * 0.55}
          fill={lit ? (i % 2 ? 'var(--blossom-soft)' : 'var(--blossom)') : 'var(--blossom-dim)'}
          opacity={lit ? 0.92 : 0.7}
        />
      ))}
      <circle cx={x} cy={y} r={r * 0.22} fill={lit ? 'var(--text)' : 'var(--text-faint)'} />
    </g>
  );
}

/**
 * One tree, rooted at (x, ground). `scale` shrinks the companions of a grove
 * without a second set of geometry.
 */
function Tree({
  tier,
  x,
  ground,
  scale,
  seed,
  running,
}: {
  tier: GroveTier;
  x: number;
  ground: number;
  scale: number;
  seed: number;
  running: boolean;
}) {
  const rand = prng(seed);
  const h = tier.trunk * scale;
  const topY = ground - h;
  const base = Math.max(1.6, (tier.trunk / 11) * scale);

  if (tier.key === 'seedling') {
    return (
      <g>
        <path
          d={`M${x} ${ground} Q${x - 3} ${ground - h / 2} ${x} ${topY}`}
          fill="none"
          stroke="var(--bark)"
          strokeWidth="2"
          strokeLinecap="round"
        />
        <path d={`M${x} ${ground - h * 0.55} q-9 -3 -11 -9 q8 0 11 9`} fill="var(--blossom-dim)" />
        <path d={`M${x} ${ground - h * 0.7} q9 -3 11 -9 q-8 0 -11 9`} fill="var(--blossom-dim)" />
        <Blossom x={x} y={topY - 4} r={5.5} lit={running} />
      </g>
    );
  }

  // Branches: pairs up the trunk, alternating sides, reaching out and up.
  const branches: { x1: number; y1: number; cx: number; cy: number; x2: number; y2: number }[] = [];
  for (let i = 0; i < tier.branches; i++) {
    const at = ground - h * (0.42 + (0.4 * i) / Math.max(1, tier.branches));
    for (const side of [-1, 1]) {
      const reach = (h * (0.5 - i * 0.07) + rand() * 5) * side;
      const lift = h * (0.3 - i * 0.04) + rand() * 4;
      branches.push({
        x1: x,
        y1: at,
        cx: x + reach * 0.45,
        cy: at - lift * 0.15,
        x2: x + reach,
        y2: at - lift,
      });
    }
  }

  // Canopy: an ellipse over the branch tips, blossoms scattered inside it.
  const canopyRx = h * 0.78;
  const canopyRy = h * 0.46;
  const canopyCy = topY + canopyRy * 0.35;
  const blossoms = Array.from({ length: Math.max(1, Math.round(tier.blossoms * (scale < 1 ? 0.6 : 1))) }, () => {
    const a = rand() * Math.PI * 2;
    const d = Math.sqrt(rand());
    return {
      x: x + Math.cos(a) * canopyRx * d,
      y: canopyCy + Math.sin(a) * canopyRy * d,
      r: (4.2 + rand() * 2.6) * Math.max(0.8, scale),
    };
  });

  return (
    <g>
      {/* Canopy haze behind the blossoms, only while the grove is being paid */}
      <ellipse
        cx={x}
        cy={canopyCy}
        rx={canopyRx * 1.05}
        ry={canopyRy * 1.05}
        fill={running ? 'var(--blossom-wash-strong)' : 'var(--blossom-wash)'}
      />
      {/* Trunk, flaring at the roots */}
      <path
        d={`M${x - base} ${ground} C${x - base * 0.6} ${ground - h * 0.4} ${x - base * 0.35} ${ground - h * 0.7} ${x - 0.8} ${topY}
            L${x + 0.8} ${topY} C${x + base * 0.35} ${ground - h * 0.7} ${x + base * 0.6} ${ground - h * 0.4} ${x + base} ${ground} Z`}
        fill="var(--bark)"
      />
      {branches.map((b, i) => (
        <path
          key={i}
          d={`M${b.x1} ${b.y1} Q${b.cx} ${b.cy} ${b.x2} ${b.y2}`}
          fill="none"
          stroke="var(--bark)"
          strokeWidth={Math.max(1.2, base * 0.45)}
          strokeLinecap="round"
        />
      ))}
      <g>
        {blossoms.map((b, i) => (
          <Blossom key={i} x={b.x} y={b.y} r={b.r} lit={running} />
        ))}
        {running ? (
          <animate attributeName="opacity" values="0.86;1;0.86" dur="4.8s" repeatCount="indefinite" />
        ) : null}
      </g>
    </g>
  );
}

export function GroveVisual({ seeds, running }: { seeds: bigint; running: boolean }) {
  const tier = groveTier(seeds);
  const canopyHeight = tier.trunk * 1.35;
  const ground = TOP_MARGIN + Math.max(20, canopyHeight);
  const height = ground + ROOT_DEPTH + 10;
  const falling = running && tier.blossoms >= 6;

  return (
    <figure style={{ margin: '14px 0 0' }} aria-label={`Grove: ${tier.label.toLowerCase()}`}>
      <svg
        viewBox={`0 0 ${WIDTH} ${height}`}
        width="100%"
        role="img"
        aria-hidden="true"
        style={{ display: 'block', background: 'var(--bg-inset)' }}
      >
        {/* Soil line, with roots hatched lightly below it */}
        <line x1="0" y1={ground} x2={WIDTH} y2={ground} stroke="var(--rule-strong)" strokeWidth="1" />
        {tier.trunk > 0 ? (
          <g stroke="var(--rule-strong)" strokeWidth="1" fill="none" strokeDasharray="2 3">
            <path d={`M${CENTRE} ${ground} q-14 8 -34 ${ROOT_DEPTH - 4}`} />
            <path d={`M${CENTRE} ${ground} q2 10 -4 ${ROOT_DEPTH}`} />
            <path d={`M${CENTRE} ${ground} q14 8 34 ${ROOT_DEPTH - 4}`} />
          </g>
        ) : (
          /* Bare soil: a planted mound, waiting */
          <path
            d={`M${CENTRE - 16} ${ground} q16 -10 32 0`}
            fill="none"
            stroke="var(--rule-strong)"
            strokeWidth="1.2"
          />
        )}

        {tier.companions ? (
          <>
            <Tree tier={tier} x={CENTRE - 96} ground={ground} scale={0.55} seed={7} running={running} />
            <Tree tier={tier} x={CENTRE + 96} ground={ground} scale={0.6} seed={11} running={running} />
          </>
        ) : null}
        {tier.trunk > 0 ? (
          <Tree tier={tier} x={CENTRE} ground={ground} scale={1} seed={3} running={running} />
        ) : null}

        {/* Petals drifting down, only while the schedule is actually paying */}
        {falling
          ? [0, 1, 2, 3].map((i) => {
              const px = CENTRE - 60 + i * 38;
              const start = TOP_MARGIN + 20 + (i % 2) * 14;
              return (
                <ellipse key={i} cx={px} cy={start} rx="2.6" ry="1.6" fill="var(--blossom-soft)" opacity="0">
                  <animate
                    attributeName="cy"
                    values={`${start};${ground - 2}`}
                    dur={`${6 + i}s`}
                    begin={`${i * 1.7}s`}
                    repeatCount="indefinite"
                  />
                  <animate
                    attributeName="cx"
                    values={`${px};${px + 10};${px - 6};${px + 4}`}
                    dur={`${6 + i}s`}
                    begin={`${i * 1.7}s`}
                    repeatCount="indefinite"
                  />
                  <animate
                    attributeName="opacity"
                    values="0;0.8;0.8;0"
                    dur={`${6 + i}s`}
                    begin={`${i * 1.7}s`}
                    repeatCount="indefinite"
                  />
                </ellipse>
              );
            })
          : null}
      </svg>
      <figcaption
        style={{
          fontSize: 11,
          letterSpacing: '0.18em',
          color: 'var(--text-faint)',
          marginTop: 8,
          display: 'flex',
          justifyContent: 'space-between',
        }}
      >
        <span>{tier.label}</span>
        <span>{tier.trunk === 0 ? 'READY TO PLANT' : running ? 'BLOOMING' : 'DORMANT'}</span>
      </figcaption>
    </figure>
  );
}
