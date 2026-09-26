/**
 * The grove: one cherry tree that grows with the seeds this wallet holds.
 *
 * Cosmetic only — it never affects a number. The painting comes from the
 * seed count (five illustrated stages: seed, sprout, young tree, blossoming
 * tree, fruiting tree — `public/grove/*.webp`, transparent); within the
 * sprout stage it grows with `rigTier` so the first few plantings visibly
 * change it. The stage *name* and next-stage progress come from the growth
 * layer (`groveTier`), the same names the share card and milestones use.
 *
 * The painting stands in a small night scene: a blossom glow behind it,
 * soft out-of-focus petals in the air, and its own reflection on the water
 * below (a mirrored, masked copy of the same image — nothing extra to load).
 *
 * Petals fall and the crown sways only while the funded schedule is paying;
 * those and the drifting bokeh stop under prefers-reduced-motion.
 */
import type { CSSProperties } from 'react';

import { formatCount } from '../lib/format';
import { groveTier, nextGroveTier } from '../lib/growth/milestones';

export type RigTier = {
  key: 'empty' | 'hand' | 'small' | 'industrial' | 'multi' | 'facility';
};

export function rigTier(rigs: bigint): RigTier {
  if (rigs >= 500n) return { key: 'facility' };
  if (rigs >= 100n) return { key: 'multi' };
  if (rigs >= 25n) return { key: 'industrial' };
  if (rigs >= 5n) return { key: 'small' };
  if (rigs >= 1n) return { key: 'hand' };
  return { key: 'empty' };
}

type Art = { id: string; w: number; h: number; from: bigint; mark: string };

/** The five paintings, their pixel size, and the seed count each starts at. */
const ART: readonly Art[] = [
  { id: 'seed', w: 335, h: 260, from: 0n, mark: '0' },
  { id: 'sprout', w: 347, h: 362, from: 1n, mark: '1+' },
  { id: 'young', w: 351, h: 490, from: 100n, mark: '100+' },
  { id: 'bloom', w: 459, h: 538, from: 1_000n, mark: '1K+' },
  { id: 'fruit', w: 547, h: 581, from: 10_000n, mark: '10K+' },
];

const BASE = process.env.NEXT_PUBLIC_BASE_PATH ?? '';
const src = (id: string) => `${BASE}/grove/${id}.webp`;

/** The latest stage this seed count has reached, and its place in the row. */
function artFor(seeds: bigint): { art: Art; index: number } {
  let found = { art: ART[0] as Art, index: 0 };
  ART.forEach((art, index) => {
    if (seeds >= art.from) found = { art, index };
  });
  return found;
}

/** Share of the frame width the painting fills: bigger stages stand wider. */
function artWidth(seeds: bigint, index: number): number {
  if (index === 1) {
    const k = rigTier(seeds).key;
    return k === 'hand' ? 0.4 : k === 'small' ? 0.48 : 0.56;
  }
  return [0.46, 0, 0.6, 0.78, 0.92][index] ?? 0.92;
}

/** Share of the frame's height given to the reflection under the island. */
const WATER = 0.1;

/** Falling petals: horizontal start (%), start height (%), delay (s), size (px). */
const PETALS: readonly [number, number, number, number][] = [
  [30, 30, 0, 7],
  [62, 22, 1.4, 6],
  [44, 40, 2.9, 5],
  [72, 38, 4.1, 8],
  [24, 48, 5.3, 6],
  [55, 18, 6.2, 7],
  [16, 26, 7.6, 5],
  [82, 30, 3.4, 6],
  [38, 12, 8.8, 8],
  [66, 50, 2.2, 5],
];

export function RigVisual({ rigs: seeds, running }: { rigs: bigint; running: boolean }) {
  const { art, index } = artFor(seeds);
  const width = artWidth(seeds, index);
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
      <div
        className="grove-art"
        data-stage={art.id}
        // The frame fits the painting at its stage width, plus a little
        // headroom above and the reflection strip below.
        style={{ aspectRatio: `1 / ${(((width * art.h) / art.w + 0.05) / (1 - WATER)).toFixed(3)}` }}
      >
        <span className="grove-moon" aria-hidden="true" />
        <span className="grove-bokeh" aria-hidden="true" />
        <span className="grove-halo" aria-hidden="true" style={{ width: `${Math.min(width * 1.3, 1) * 100}%` }} />
        {/* eslint-disable-next-line @next/next/no-img-element -- pre-sized transparent WebP; static export serves images unoptimised */}
        <img
          className={[
            'grove-tree',
            growing ? 'grove-tree--sway' : '',
            seeds === 0n ? 'grove-tree--dormant' : '',
          ]
            .filter(Boolean)
            .join(' ')}
          src={src(art.id)}
          width={art.w}
          height={art.h}
          alt=""
          aria-hidden="true"
          draggable={false}
          style={{ width: `${width * 100}%` }}
        />
        <div className="grove-water" aria-hidden="true">
          {/* eslint-disable-next-line @next/next/no-img-element -- the same painting, mirrored into the water */}
          <img
            className="grove-mirror"
            src={src(art.id)}
            width={art.w}
            height={art.h}
            alt=""
            draggable={false}
            style={{ width: `${width * 100}%` }}
          />
        </div>
        <span className="grove-horizon" aria-hidden="true" />
        {growing
          ? PETALS.map(([left, top, delay, size], i) => (
              <span
                key={i}
                className="grove-petal"
                aria-hidden="true"
                style={{
                  left: `${left}%`,
                  top: `${top}%`,
                  width: `${size}px`,
                  height: `${Math.round(size * 0.72)}px`,
                  animationDelay: `${delay}s`,
                  animationDuration: `${7 + (i % 4) * 1.3}s`,
                }}
              />
            ))
          : null}
      </div>
      <ol
        className="grove-track"
        aria-hidden="true"
        // The timeline under the cards lights up to the current stage's dot.
        style={{ '--grove-lit': `${index * 25}%` } as CSSProperties}
      >
        {ART.map((a, k) => (
          <li
            key={a.id}
            className={k === index ? 'grove-step grove-step--now' : k < index ? 'grove-step grove-step--done' : 'grove-step'}
          >
            {/* eslint-disable-next-line @next/next/no-img-element -- same stage paintings as above, shown small */}
            <img src={src(a.id)} alt="" width={a.w} height={a.h} loading="lazy" draggable={false} />
            <span>{a.mark}</span>
          </li>
        ))}
      </ol>
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

/**
 * The painted journey: all five stages on one moonlit lake. A banner only —
 * it shows where a grove can go, not where this wallet's grove is.
 */
export function GroveJourney() {
  return (
    <div className="grove-journey" aria-hidden="true">
      {/* eslint-disable-next-line @next/next/no-img-element -- pre-sized WebP pair; static export serves images unoptimised */}
      <img
        src={src('journey')}
        srcSet={`${src('journey-768')} 768w, ${src('journey')} 1536w`}
        sizes="(max-width: 452px) 100vw, 452px"
        width={1536}
        height={660}
        alt=""
        draggable={false}
        fetchPriority="high"
      />
    </div>
  );
}
