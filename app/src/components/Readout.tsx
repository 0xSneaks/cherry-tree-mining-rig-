import type { ReactNode } from 'react';

export function Readout({
  label,
  value,
  unit,
  hero,
  rigs,
}: {
  label: string;
  value: ReactNode;
  unit?: string;
  hero?: boolean;
  rigs?: boolean;
}) {
  const cls = hero
    ? 'readout-value readout-value--hero'
    : rigs
      ? 'readout-value readout-value--rigs'
      : 'readout-value';
  return (
    <div className="readout">
      <span className="readout-label">{label}</span>
      <span className={cls}>
        {value}
        {unit ? <span className="unit">{unit}</span> : null}
      </span>
    </div>
  );
}
