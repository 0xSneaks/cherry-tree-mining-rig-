'use client';

/**
 * The game. Identical in both modes — everything mode-specific arrives through
 * the `host` prop, so there is exactly one implementation of the loop.
 */
import { useCallback, useEffect, useMemo, useState } from 'react';
import { isAddress, type Address } from '@solana/kit';

import { buyRigs, claim, compound } from '../lib/actions';
import { PRODUCT_NAME, PRODUCT_NAME_SHORT } from '../lib/brand';
import { rpc } from '../lib/chain';
import { formatCount, formatSol } from '../lib/format';
import { estimatedOutputPerDay } from '../lib/program/accrual';
import { rigsAffordable } from '../lib/program/pricing';
import { discoverIdentity } from '../lib/program/identity';
import { accessPhase, purchaseAllowed, type Identity } from '../lib/program/launch';
import { useLiveAccrual } from '../hooks/useLiveAccrual';
import { useRigsState, type LifecycleSource } from '../hooks/useRigsState';
import type { RigsSigner } from '../lib/signer';
import { BuyPanel } from './BuyPanel';
import { OreBin } from './OreBin';
import { Readout } from './Readout';
import { RigVisual } from './RigVisual';
import { RoomLeaderboard } from './RoomLeaderboard';
import { ShareCard, type SharePayload } from './ShareCard';
import { TransparencyPanel } from './TransparencyPanel';
import { FounderActivation } from './FounderActivation';
import { LaunchPanel } from './LaunchPanel';
import { YieldPanel } from './YieldPanel';

/**
 * `?ref=<wallet>` — a proposed referrer, nothing more. The program decides
 * whether it binds (first purchase only, an existing planter, not yourself).
 */
function referrerFromUrl(self: Address | null): Address | null {
  if (typeof window === 'undefined') return null;
  const raw = new URLSearchParams(window.location.search).get('ref');
  if (!raw || !isAddress(raw) || raw === self) return null;
  return raw as Address;
}

export type CherryRoomInfo = { id: string; title: string; memberCount: number };

export type Host = {
  mode: 'embedded' | 'standalone';
  address: Address | null;
  signer: RigsSigner | null;
  /** False after `walletDisconnected`, or before a standalone user connects. */
  walletConnected: boolean;
  room: CherryRoomInfo | null;
  launchToken: string | null;
  lifecycle: LifecycleSource;
  openProfile?: (address: string) => void;
  share?: (payload: SharePayload) => Promise<void>;
  /** Rendered only when `mode === 'standalone'`. Never in the embed. */
  connectSlot?: React.ReactNode;
};

/** A claim worth offering to share, rather than nagging on every dust claim. */
const LARGE_CLAIM_LAMPORTS = 50_000_000n; // 0.05 SOL

export function Game({ host }: { host: Host }) {
  const { snapshot, loading, error, live, refresh } = useRigsState(
    host.address,
    host.lifecycle,
  );
  const { claimable, chainTime } = useLiveAccrual(snapshot, live);

  // The name record this wallet can present. Display and account selection
  // only: the program re-verifies it on every purchase.
  const [identity, setIdentity] = useState<Identity | null>(null);
  useEffect(() => {
    let cancelled = false;
    setIdentity(null);
    if (!host.address) return;
    void discoverIdentity(rpc(), host.address).then((p) => {
      if (!cancelled) setIdentity(p.kind);
    });
    return () => {
      cancelled = true;
    };
  }, [host.address]);

  const [busy, setBusy] = useState<null | 'buy' | 'claim' | 'compound'>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [walletLamports, setWalletLamports] = useState<bigint | null>(null);
  const [sharePayload, setSharePayload] = useState<SharePayload | null>(null);

  // Wallet balance backs the MAX button only; it is never used in accounting.
  useEffect(() => {
    let cancelled = false;
    if (!host.address) {
      setWalletLamports(null);
      return;
    }
    void rpc()
      .getBalance(host.address)
      .send()
      .then((r) => {
        if (!cancelled) setWalletLamports(BigInt(r.value));
      })
      .catch(() => {
        if (!cancelled) setWalletLamports(null);
      });
    return () => {
      cancelled = true;
    };
  }, [host.address, snapshot]);

  // Narrowed together so TypeScript can see that a present config implies a
  // present snapshot.
  const snap = snapshot;
  const config = snap?.config ?? null;
  const miner = snap?.miner ?? null;
  const rigs = miner?.rigs ?? 0n;

  const outputPerDay = useMemo(
    () => (config ? estimatedOutputPerDay(config, miner, chainTime) : 0n),
    [config, miner, chainTime],
  );

  const scheduleLive = !!config && chainTime < config.rewardPeriodFinish;
  const paused = config?.paused ?? false;

  // Spec §8: `walletDisconnected` disables all three actions but keeps the
  // read-only state on screen. State is never unmounted.
  const canAct = host.walletConnected && !!host.signer && !paused && busy === null;

  // Planting follows the launch phase. Before T0 nobody plants through the
  // app — the founder activation is the one deliberate first-buy procedure
  // (app/scripts/ops/first-buy.ts), with preflight and explicit confirmation.
  const phase = config ? accessPhase(config.launchT0, chainTime) : 'preLaunch';
  const plantOpen = phase !== 'preLaunch' && purchaseAllowed(phase, identity ?? 'none');
  const referrer = !miner ? referrerFromUrl(host.address) : null;

  const compoundPreview = useMemo(() => {
    if (!config) return 0n;
    try {
      return rigsAffordable(
        claimable,
        config.rigPriceLamports,
        BigInt(config.purchaseFeeBps),
      );
    } catch {
      return 0n;
    }
  }, [claimable, config]);

  const run = useCallback(
    async (kind: 'buy' | 'claim' | 'compound', fn: () => Promise<unknown>) => {
      if (!host.signer) return;
      setBusy(kind);
      setActionError(null);
      try {
        await fn();
        // Reconcile against the chain before anything on screen moves.
        await refresh();
      } catch (e) {
        setActionError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(null);
      }
    },
    [host.signer, refresh],
  );

  const onBuy = (count: bigint) =>
    void run('buy', () => buyRigs(host.signer!, count, { referrer }));

  const onClaim = () =>
    void run('claim', async () => {
      const amount = claimable;
      await claim(host.signer!);
      if (host.share && amount >= LARGE_CLAIM_LAMPORTS) {
        setSharePayload({ kind: 'claim', rigs: rigs.toString(), claimed: amount.toString() });
      }
    });

  const onCompound = () =>
    void run('compound', async () => {
      const minted = compoundPreview;
      await compound(host.signer!);
      if (host.share && minted > 0n) {
        setSharePayload({
          kind: 'compound',
          rigs: (rigs + minted).toString(),
          compounded: minted.toString(),
        });
      }
    });

  const preLaunch = !!config && phase === 'preLaunch';
  const statusClass = !live
    ? 'status status--offline'
    : paused || !scheduleLive || preLaunch
      ? 'status status--paused'
      : 'status status--live';
  const statusLabel = !live
    ? 'Paused'
    : paused
      ? 'Halted'
      : preLaunch
        ? 'Pre-launch'
        : scheduleLive
          ? 'Live'
          : 'Unfunded';

  return (
    <main className="shell">
      <header className="masthead">
        <h1 className="wordmark">{PRODUCT_NAME}</h1>
        {host.mode === 'standalone' ? host.connectSlot : null}
      </header>
      <div className="subhead">
        <span>Grove status</span>
        <span className={statusClass}>
          <span className="status-dot" />
          {statusLabel}
        </span>
      </div>

      {host.room ? (
        <p className="note" style={{ marginTop: 6 }}>
          {host.room.title} · {formatCount(host.room.memberCount)} members
        </p>
      ) : null}

      <hr className="rule" />

      {loading && !snapshot ? (
        <p className="note">Reading the chain…</p>
      ) : error && !snapshot ? (
        <div className="banner banner--alert">
          Could not read program state: {error.message}
        </div>
      ) : !config || !snap ? (
        <div className="banner banner--alert">
          {PRODUCT_NAME_SHORT} program is not initialised on this cluster yet.
        </div>
      ) : (
        <>
          <LaunchPanel
            config={config}
            chainTime={chainTime}
            wallet={host.address}
            identity={identity}
          />

          {phase === 'preLaunch' &&
          host.address !== null &&
          host.address === config.founder &&
          host.signer ? (
            <FounderActivation founder={host.address} signer={host.signer} onDone={refresh} />
          ) : null}

          <YieldPanel
            config={config}
            miner={miner}
            claimable={claimable}
            rewardVaultLamports={snap.rewardVaultDistributable}
            chainTime={chainTime}
            snapshotTime={snap.chainTimeSeconds}
            weight={miner?.weight ?? 0}
          />

          <Readout label="My grove" value={formatCount(rigs)} unit="SEEDS" rigs />
          <Readout
            label="Current growth"
            value={`≈ ${formatSol(outputPerDay, 5)}`}
            unit="SOL / DAY"
          />
          <OreBin lamports={claimable} />

          <p className="note">
            Based on the current funded reward schedule and seeds planted.
            Changes when seeds planted or reward funding changes.
          </p>

          {!host.walletConnected ? (
            <div className="banner">
              Wallet disconnected. Your grove keeps growing — reconnect to claim,
              replant or plant.
            </div>
          ) : null}
          {paused ? (
            <div className="banner banner--alert">
              The program is halted by its authority. Existing state is
              unaffected; actions resume when it is unhalted.
            </div>
          ) : null}
          {actionError ? (
            <div className="banner banner--alert">{actionError}</div>
          ) : null}

          <div className="actions">
            <button type="button" onClick={onCompound} disabled={!canAct || compoundPreview === 0n}>
              {busy === 'compound'
                ? 'Replanting…'
                : compoundPreview > 0n
                  ? `Replant · ${formatCount(compoundPreview)}`
                  : 'Replant'}
            </button>
            <button
              type="button"
              className="primary"
              onClick={onClaim}
              disabled={!canAct || claimable === 0n}
            >
              {busy === 'claim' ? 'Claiming…' : 'Claim SOL'}
            </button>
          </div>

          <RigVisual rigs={rigs} running={scheduleLive && !paused} />

          <hr className="rule" />

          <BuyPanel
            config={config}
            chainTime={chainTime}
            walletLamports={walletLamports}
            disabled={!canAct || !plantOpen}
            closedReason={
              plantOpen
                ? null
                : phase === 'preLaunch'
                  ? 'Planting opens when the founder activates the grove.'
                  : 'Planting is not open to this wallet in the current access phase.'
            }
            referrer={referrer}
            busy={busy === 'buy'}
            onBuy={onBuy}
          />

          {host.room ? (
            <>
              <hr className="rule" />
              <RoomLeaderboard
                roomId={host.room.id}
                roomTitle={host.room.title}
                launchToken={host.launchToken}
                viewer={host.address}
                onOpenProfile={host.openProfile}
              />
            </>
          ) : null}

          <hr className="rule" />

          <TransparencyPanel
            config={config}
            miner={miner}
            rewardVaultLamports={snap.rewardVaultDistributable}
            rewardVaultAddress={snap.rewardVaultAddress}
            chainTime={chainTime}
          />

          <p className="disclosure">
            {PRODUCT_NAME_SHORT} is a planting <em>simulation</em>. SOL is not proof-of-work
            mined: seeds do not participate in Solana consensus and do not
            cryptographically produce new SOL. Rewards are paid from a funded
            reward vault, are denominated in SOL, and vary with total seeds planted
            and funding. Seed purchases are non-redeemable.
          </p>
        </>
      )}

      {sharePayload && host.share ? (
        <ShareCard
          payload={sharePayload}
          onShare={host.share}
          onDismiss={() => setSharePayload(null)}
        />
      ) : null}
    </main>
  );
}
