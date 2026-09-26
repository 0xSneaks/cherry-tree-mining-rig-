/**
 * Grove visual: the miner's tree, not a mine shaft.
 *
 * Cosmetic only — tier is derived from the real rig/seed count and never
 * touches a number. Assets live in /grove.
 */

export type GroveTier = {
  key: 'seed' | 'sprout' | 'young' | 'bloom' | 'producing';
  label: string;
  caption: string;
  src: string;
  min: bigint;
  mark: string;
};

export const GROVE_TIERS: GroveTier[] = [
  { key: 'seed', label: 'Bare soil', caption: 'Plant your first seed to sprout a seedling', src: '/grove/stage-0-seed.svg', min: 0n, mark: '0' },
  { key: 'sprout', label: 'Sapling', caption: 'Taking root', src: '/grove/stage-1-sprout.svg', min: 1n, mark: '1+' },
  { key: 'young', label: 'Young tree', caption: 'Growing fast', src: '/grove/stage-2-young.svg', min: 100n, mark: '100+' },
  { key: 'bloom', label: 'Cherry tree', caption: 'In full bloom', src: '/grove/stage-3-bloom.svg', min: 1000n, mark: '1K+' },
  { key: 'producing', label: 'Producing', caption: 'Earning rewards', src: '/grove/stage-4-producing.svg', min: 10_000n, mark: '10K+' },
];

export function groveTier(rigs: bigint): GroveTier {
  let current = GROVE_TIERS[0];
  for (const tier of GROVE_TIERS) {
    if (rigs >= tier.min) current = tier;
  }
  return current;
}

function nextTier(rigs: bigint): GroveTier | null {
  for (const tier of GROVE_TIERS) {
    if (rigs < tier.min) return tier;
  }
  return null;
}

function progressToNext(rigs: bigint): number {
  const next = nextTier(rigs);
  if (!next) return 1;
  const current = groveTier(rigs);
  const span = Number(next.min - current.min);
  if (span <= 0) return 1;
  return Math.min(1, Number(rigs - current.min) / span);
}

export function RigVisual({ rigs, running }: { rigs: bigint; running: boolean }) {
  const tier = groveTier(rigs);
  const next = nextTier(rigs);
  const pct = Math.round(progressToNext(rigs) * 100);

  return (
    <figure className="grove" aria-label={`Grove: ${tier.label.toLowerCase()}`}>
      <div className="grove-hero">
        <img src={tier.src} alt="" width={400} height={320} />
      </div>

      <ol className="grove-stages">
        {GROVE_TIERS.map((t) => {
          const reached = rigs >= t.min;
          const active = t.key === tier.key;
          return (
            <li key={t.key} className={active ? 'is-active' : reached ? 'is-reached' : undefined}>
              <img src={t.src} alt="" />
              <span>{t.mark}</span>
            </li>
          );
        })}
      </ol>

      <figcaption className="grove-caption">
        <span>
          {tier.label} · {rigs.toString()} seeds
        </span>
        <span className={running ? 'grove-live' : undefined}>{running ? 'Growing' : 'Not planted'}</span>
      </figcaption>

      <div className="grove-bar" aria-hidden="true">
        <i style={{ width: `${pct}%` }} />
      </div>
      <p className="grove-hint">
        {next ? `${next.label} at ${next.min.toString()} seeds` : tier.caption}
      </p>
    </figure>
  );
}
