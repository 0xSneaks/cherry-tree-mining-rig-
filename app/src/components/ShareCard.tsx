'use client';

/**
 * Inline share prompt.
 *
 * Spec §8: one tap, pre-filled, dismissible — and never automatic. This
 * component only appears after a successful compound or a large claim, and
 * the user has to press it.
 */
import { useState } from 'react';

import { formatCount, formatSol } from '../lib/format';

export type SharePayload =
  | { kind: 'compound'; rigs: string; compounded: string }
  | { kind: 'claim'; rigs: string; claimed: string };

export function ShareCard({
  payload,
  onShare,
  onDismiss,
}: {
  payload: SharePayload;
  onShare: (payload: SharePayload) => Promise<void>;
  onDismiss: () => void;
}) {
  const [busy, setBusy] = useState(false);

  const headline =
    payload.kind === 'compound'
      ? `+${formatCount(BigInt(payload.compounded))} SEEDS`
      : `CLAIMED ${formatSol(BigInt(payload.claimed), 4)} SOL`;

  return (
    <div className="banner" role="status" style={{ marginTop: 18 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
        <strong style={{ color: 'var(--blossom)', letterSpacing: '0.08em' }}>
          {headline}
        </strong>
        <span style={{ marginLeft: 'auto', display: 'flex', gap: 8 }}>
          <button
            type="button"
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              try {
                await onShare(payload);
                onDismiss();
              } finally {
                setBusy(false);
              }
            }}
          >
            {busy ? 'Sharing…' : 'Share'}
          </button>
          <button type="button" onClick={onDismiss} aria-label="Dismiss">
            ✕
          </button>
        </span>
      </div>
    </div>
  );
}
