/**
 * The mine shaft: a cross-section, not a scene.
 *
 * Deterministic tier from the real rig count, cosmetic only — it never
 * affects a number. Individual machines are deliberately not drawn: at 500
 * rigs that is a pointless render, and a tier reads faster anyway.
 *
 * The canvas is sized to its content so a small operation does not sit in a
 * field of empty rock.
 */
export type RigTier = {
  key: string;
  label: string;
  /** Gallery levels cut into the rock. */
  levels: number;
  /** Machines drawn per gallery, split either side of the hoist shaft. */
  perLevel: number;
};

export function rigTier(rigs: bigint): RigTier {
  if (rigs >= 500n)
    return { key: 'facility', label: 'CHERRY GROVE', levels: 4, perLevel: 6 };
  if (rigs >= 100n)
    return { key: 'multi', label: 'CHERRY TREE', levels: 3, perLevel: 4 };
  if (rigs >= 25n)
    return { key: 'industrial', label: 'YOUNG TREE', levels: 2, perLevel: 3 };
  if (rigs >= 5n)
    return { key: 'small', label: 'SAPLING', levels: 1, perLevel: 2 };
  if (rigs >= 1n)
    return { key: 'hand', label: 'SEEDLING', levels: 1, perLevel: 1 };
  return { key: 'empty', label: 'BARE SOIL', levels: 1, perLevel: 0 };
}

// A narrow viewBox on purpose: the SVG scales to the column width, so fewer
// user units means larger machines on a 390px phone.
const WIDTH = 300;
const SURFACE_Y = 44;
const LEVEL_GAP = 40;
const CENTRE = WIDTH / 2;
/** Half-width of the hoist shaft; galleries start clear of it. */
const SHAFT_CLEARANCE = 22;
const MARGIN = 22;

/**
 * Machine positions along one gallery: evenly spaced either side of the
 * hoist shaft, with the left bank filled first so an odd count leans the way
 * the headframe does.
 */
function galleryPositions(count: number): number[] {
  if (count === 0) return [];
  if (count === 1) return [CENTRE - SHAFT_CLEARANCE - 26];

  const left = Math.ceil(count / 2);
  const right = count - left;

  const spread = (n: number, from: number, to: number): number[] => {
    if (n === 0) return [];
    if (n === 1) return [(from + to) / 2];
    const step = (to - from) / (n - 1);
    return Array.from({ length: n }, (_, i) => from + step * i);
  };

  return [
    ...spread(left, MARGIN + 14, CENTRE - SHAFT_CLEARANCE - 14),
    ...spread(right, CENTRE + SHAFT_CLEARANCE + 14, WIDTH - MARGIN - 14),
  ];
}

function Machine({
  x,
  y,
  running,
  delay,
}: {
  x: number;
  y: number;
  running: boolean;
  delay: number;
}) {
  return (
    <g>
      {/* Drill head biting into the gallery floor */}
      <line x1={x} y1={y - 4} x2={x} y2={y} stroke="var(--steel)" strokeWidth="1.2" />
      <line x1={x - 3} y1={y - 1} x2={x + 3} y2={y - 1} stroke="var(--steel)" strokeWidth="1.2" />
      {/* Housing */}
      <rect
        x={x - 11}
        y={y - 17}
        width="22"
        height="13"
        fill="var(--bg)"
        stroke="var(--steel)"
        strokeWidth="1.2"
      />
      {/* Status lamp — amber only while the schedule is actually paying */}
      <rect
        x={x - 8}
        y={y - 14}
        width="5"
        height="7"
        fill={running ? 'var(--blossom)' : 'var(--rule-strong)'}
      >
        {running ? (
          <animate
            attributeName="opacity"
            values="0.25;1;0.25"
            dur="1.8s"
            begin={`${delay}s`}
            repeatCount="indefinite"
          />
        ) : null}
      </rect>
      {/* Vent slots */}
      <line x1={x - 1} y1={y - 13} x2={x + 7} y2={y - 13} stroke="var(--rule-strong)" strokeWidth="1" />
      <line x1={x - 1} y1={y - 10} x2={x + 7} y2={y - 10} stroke="var(--rule-strong)" strokeWidth="1" />
      <line x1={x - 1} y1={y - 7} x2={x + 7} y2={y - 7} stroke="var(--rule-strong)" strokeWidth="1" />
    </g>
  );
}

export function RigVisual({ rigs, running }: { rigs: bigint; running: boolean }) {
  const tier = rigTier(rigs);
  const height = SURFACE_Y + tier.levels * LEVEL_GAP + 16;
  const shaftBottom = SURFACE_Y + tier.levels * LEVEL_GAP;

  return (
    <figure style={{ margin: '14px 0 0' }} aria-label={`Grove: ${tier.label.toLowerCase()}`}>
      <svg
        viewBox={`0 0 ${WIDTH} ${height}`}
        width="100%"
        role="img"
        aria-hidden="true"
        style={{ display: 'block', background: 'var(--bg-inset)' }}
      >
        {/* Headframe: winding wheel and A-frame above the surface */}
        <circle cx={CENTRE} cy="13" r="8" fill="none" stroke="var(--steel)" strokeWidth="1.4" />
        <circle cx={CENTRE} cy="13" r="2" fill="var(--steel)" />
        <path
          d={`M${CENTRE - 24} ${SURFACE_Y} L${CENTRE} 20 L${CENTRE + 24} ${SURFACE_Y}`}
          fill="none"
          stroke="var(--steel)"
          strokeWidth="1.4"
        />
        <line
          x1={CENTRE - 12}
          y1={SURFACE_Y - 11}
          x2={CENTRE + 12}
          y2={SURFACE_Y - 11}
          stroke="var(--rule-strong)"
          strokeWidth="1"
        />

        {/* Surface line */}
        <line x1="0" y1={SURFACE_Y} x2={WIDTH} y2={SURFACE_Y} stroke="var(--rule-strong)" strokeWidth="1" />

        {/* Hoist shaft */}
        <line x1={CENTRE - 7} y1={SURFACE_Y} x2={CENTRE - 7} y2={shaftBottom} stroke="var(--rule-strong)" strokeWidth="1.2" />
        <line x1={CENTRE + 7} y1={SURFACE_Y} x2={CENTRE + 7} y2={shaftBottom} stroke="var(--rule-strong)" strokeWidth="1.2" />
        {/* Cage, riding the shaft only while the mine is producing */}
        <rect x={CENTRE - 5} y={SURFACE_Y + 6} width="10" height="12" fill="var(--steel)" opacity="0.5">
          {running ? (
            <animate
              attributeName="y"
              values={`${SURFACE_Y + 6};${shaftBottom - 18};${SURFACE_Y + 6}`}
              dur="7s"
              repeatCount="indefinite"
            />
          ) : null}
        </rect>

        {Array.from({ length: tier.levels }, (_, level) => {
          const y = SURFACE_Y + (level + 1) * LEVEL_GAP;
          return (
            <g key={level}>
              {/* Gallery floor */}
              <line x1={MARGIN} y1={y} x2={WIDTH - MARGIN} y2={y} stroke="var(--rule-strong)" strokeWidth="1.2" />
              {/* Rock either side, hatched lightly */}
              <line x1="0" y1={y} x2={MARGIN} y2={y} stroke="var(--rule)" strokeWidth="1" strokeDasharray="2 4" />
              <line
                x1={WIDTH - MARGIN}
                y1={y}
                x2={WIDTH}
                y2={y}
                stroke="var(--rule)"
                strokeWidth="1"
                strokeDasharray="2 4"
              />
              {galleryPositions(tier.perLevel).map((x, i) => (
                <Machine key={i} x={x} y={y} running={running} delay={(level * 3 + i) * 0.23} />
              ))}
            </g>
          );
        })}
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
        <span>{running ? 'PRODUCING' : 'IDLE'}</span>
      </figcaption>
    </figure>
  );
}
