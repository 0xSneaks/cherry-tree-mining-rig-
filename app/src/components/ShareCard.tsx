'use client';

/**
 * Inline share prompt.
 *
 * Spec §8: one tap, pre-filled, dismissible — and never automatic. This
 * component only appears after a successful compound or a large claim, and
 * the user has to press it.
 */
import { useState, useSyncExternalStore } from 'react';

import { formatCount, formatSol } from '../lib/format';
import { groveTier, isRooted } from '../lib/growth/milestones';
import type { Miner } from '../lib/program/accounts';

export type SharePayload =
  | { kind: 'compound'; rigs: string; compounded: string }
  | { kind: 'claim'; rigs: string; claimed: string }
  | { kind: 'progress'; rigs: string; rooted: 'yes' | 'no'; milestone: string; ref?: string };

const subscribeLocation = () => () => undefined;
const browserLocation = () => window.location.origin + window.location.pathname;
const serverLocation = () => null;

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
  const progressSeeds = payload.kind === 'progress' ? payload.rigs : null;

  const headline =
    payload.kind === 'compound'
      ? `+${formatCount(BigInt(payload.compounded))} SEEDS REPLANTED`
      : payload.kind === 'claim'
        ? `CLAIMED ${formatSol(BigInt(payload.claimed), 4)} SOL`
        : `${formatCount(BigInt(progressSeeds!))} SEEDS · ${payload.rooted === 'yes' ? 'ROOTED' : 'TENDING'}`;

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

/** Always-available, position-only share surface. It never includes a balance or return claim. */
export function ProgressShareCard({
  wallet,
  miner,
  chainTime,
  onShare,
}: {
  wallet: string | null;
  miner: Miner | null;
  chainTime: bigint;
  onShare?: (payload: SharePayload) => Promise<void>;
}) {
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);
  const originPath = useSyncExternalStore(subscribeLocation, browserLocation, serverLocation);

  if (!miner || !wallet) return null;
  const tier = groveTier(miner.rigs)?.label ?? 'New grove';
  const rooted = isRooted(miner, chainTime);
  const referralLink = originPath ? `${originPath}?ref=${encodeURIComponent(wallet)}` : null;
  const payload: SharePayload = {
    kind: 'progress',
    rigs: miner.rigs.toString(),
    rooted: rooted ? 'yes' : 'no',
    milestone: tier,
    ref: wallet,
  };

  return (
    <section className="progress-card" data-testid="progress-card">
      <div>
        <p className="section-title">Share the grove</p>
        <p className="progress-card-title">{formatCount(miner.rigs)} seeds · {tier}</p>
        <p className="note">
          {rooted ? 'Rooted · no early-claim fee' : 'Tending toward Rooted'}
        </p>
      </div>
      <div className="progress-card-actions">
        {onShare ? (
          <button
            type="button"
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              try {
                await onShare(payload);
              } finally {
                setBusy(false);
              }
            }}
          >
            {busy ? 'Sharing…' : 'Share progress'}
          </button>
        ) : null}
        <button
          type="button"
          disabled={!referralLink}
          onClick={() => {
            if (!referralLink) return;
            void navigator.clipboard?.writeText(referralLink).then(
              () => setCopied(true),
              () => setCopied(false),
            );
          }}
        >
          {copied ? 'Referral link copied' : 'Copy referral link'}
        </button>
      </div>
      {referralLink ? <p className="share-link">{referralLink}</p> : null}
      <p className="note">Position progress only. No wallet balance or return estimate is shared.</p>
    </section>
  );
}
