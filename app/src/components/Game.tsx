'use client';

/**
 * The game. Identical in both modes — everything mode-specific arrives through
 * the `host` prop, so there is exactly one implementation of the loop.
 */
import { useCallback, useEffect, useMemo, useState } from 'react';
import type { Address } from '@solana/kit';

import { buyRigs, claim, compound } from '../lib/actions';
import { PRODUCT_NAME, PRODUCT_NAME_SHORT } from '../lib/brand';
import { rpc } from '../lib/chain';
import { CLUSTER } from '../lib/env';
import { explainActionError } from '../lib/errors';
import { formatCount } from '../lib/format';
import { estimatedOutputPerDay } from '../lib/program/accrual';
import { quoteClaim } from '../lib/program/penalty';
import { rigsAffordable } from '../lib/program/pricing';
import { discoverIdentity } from '../lib/program/identity';
import {
  accessPhase,
  genesisAllowance,
  purchaseAllowed,
  type Identity,
} from '../lib/program/launch';
import { useLiveAccrual } from '../hooks/useLiveAccrual';
import { useRigsState, type LifecycleSource } from '../hooks/useRigsState';
import type { RigsSigner } from '../lib/signer';
import { BuyPanel } from './BuyPanel';
import { ClaimConfirm } from './ClaimConfirm';
import { GroveRecord } from './GroveRecord';
import { RigVisual } from './RigVisual';
import { RoomLeaderboard } from './RoomLeaderboard';
import { ProgressShareCard, ShareCard, type SharePayload } from './ShareCard';
import { TransparencyPanel } from './TransparencyPanel';
import { FounderActivation } from './FounderActivation';
import { LaunchPanel } from './LaunchPanel';
import { YieldPanel } from './YieldPanel';
import { DilutionPanel } from './DilutionPanel';
import { DistributionPanel } from './DistributionPanel';
import { GroveStatus } from './GroveStatus';
import { MilestonesPanel } from './MilestonesPanel';
import { CommissionPanel } from './CommissionPanel';
import { ReferralPanel } from './ReferralPanel';
import { MinerCockpit } from './MinerCockpit';
import { ActivityHistory } from './ActivityHistory';
import { usePlanterStatus } from '../hooks/usePlanterStatus';
import {
  OFFICIAL_REFERRER,
  referralCandidates,
  referralDisclosure,
  referrerToSend,
  resolveReferral,
  suppliedReferral,
} from '../lib/growth/referral';

/**
 * `?ref=<wallet>` — a proposed referrer, nothing more. The program decides
 * whether it binds (first purchase only, an existing planter, not yourself);
 * `lib/growth/referral.ts` decides what the app proposes and discloses.
 */
function suppliedFromUrl(): string | null {
  if (typeof window === 'undefined') return null;
  return suppliedReferral(window.location.search);
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
const CLAIM_ACTION_LABEL = 'Claim SOL';

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
  // Kept with the action that failed, so the message renders beside the
  // button the user just pressed rather than somewhere else on the page.
  const [actionError, setActionError] = useState<
    null | { kind: 'buy' | 'claim' | 'compound'; message: string }
  >(null);
  const [confirmingClaim, setConfirmingClaim] = useState(false);
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
  const genesisRemaining = config
    ? genesisAllowance(
        config.launchT0,
        chainTime,
        miner?.lifetimeRigsBought ?? 0n,
        config.genesisCapRigs,
      )
    : null;
  const freshBuyOpen = plantOpen && (genesisRemaining === null || genesisRemaining > 0n);
  // Referral: explicit link wins; the official fallback only with no link and
  // no position, verified on chain first. Nothing binds on a later purchase.
  const [supplied] = useState<string | null>(suppliedFromUrl);
  const hasPosition = !!miner;
  const candidates = referralCandidates({
    supplied,
    self: host.address,
    hasPosition,
    official: OFFICIAL_REFERRER,
  });
  const suppliedIsPlanter = usePlanterStatus(snap ? candidates.supplied : null);
  const officialIsPlanter = usePlanterStatus(snap ? candidates.official : null);
  const referral = snap
    ? resolveReferral({
        supplied,
        self: host.address,
        hasPosition,
        boundReferrer: miner?.referrer ?? null,
        official: OFFICIAL_REFERRER,
        suppliedIsPlanter,
        officialIsPlanter,
      })
    : null;
  const referrer = referral ? referrerToSend(referral) : null;
  const referralNote = referral ? referralDisclosure(referral) : null;
  const claimQuote = miner ? quoteClaim(claimable, miner.lastPlantedAt, chainTime) : null;

  const compoundPreview = useMemo(() => {
    if (!config) return 0n;
    try {
      // Compound is priced at the compound fee (architecture §7a item 2).
      return rigsAffordable(
        claimable,
        config.rigPriceLamports,
        BigInt(config.compoundFeeBps),
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
        // Expected protocol errors are said in product language on screen;
        // the raw error stays in the developer console, never hidden.
        const explained = explainActionError(e);
        console.error(`[cherry-tree] ${kind} failed:`, explained.raw);
        setActionError({ kind, message: explained.message });
      } finally {
        setBusy(null);
      }
    },
    [host.signer, refresh],
  );

  const onBuy = (count: bigint) =>
    void run('buy', () =>
      buyRigs(host.signer!, count, { referrer, boundReferrer: miner?.referrer ?? null }),
    );

  const onClaim = () =>
    void run('claim', async () => {
      const amount = claimQuote?.net ?? 0n;
      await claim(host.signer!);
      if (host.share && amount >= LARGE_CLAIM_LAMPORTS) {
        setSharePayload({ kind: 'claim', rigs: rigs.toString(), claimed: amount.toString() });
      }
    });

  const onCompound = () =>
    void run('compound', async () => {
      const minted = compoundPreview;
      await compound(host.signer!, { boundReferrer: miner?.referrer ?? null });
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

  const buyError = actionError?.kind === 'buy' ? actionError.message : null;
  const positionError = actionError && actionError.kind !== 'buy' ? actionError.message : null;

  return (
    <main className="shell">
      {/* 1 · What this is, which network, whether it is live. */}
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
        {/* The cluster this build talks to, from the environment — during
            rehearsal that is Devnet, and the screen says so. */}
        <span className="cluster" data-testid="cluster">
          {CLUSTER.toUpperCase()}
        </span>
      </div>

      {host.room ? (
        <p className="note" style={{ marginTop: 6 }}>
          {host.room.title} · {formatCount(host.room.memberCount)} members
        </p>
      ) : null}

      <p className="lede">
        Plant seeds with SOL. They share a funded SOL reward pool at a variable
        rate.
      </p>

      {/* State-wide conditions that change what every button below does.
          "Disconnected" only when there is a wallet to reconnect — a
          first-time standalone visitor has the connect button above. */}
      {!host.walletConnected && host.address !== null ? (
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
          {/* The grove itself, from the real seed count. */}
          <RigVisual rigs={rigs} running={scheduleLive && !paused} />

          {/* 2 · The opportunity: rate, pool, runway, price. */}
          <YieldPanel
            config={config}
            miner={miner}
            rewardVaultLamports={snap.rewardVaultDistributable}
            chainTime={chainTime}
            snapshotTime={snap.chainTimeSeconds}
          />

          {/* 3 · The rules that decide whether and how this wallet can plant. */}
          <LaunchPanel
            config={config}
            chainTime={chainTime}
            wallet={host.address}
            identity={identity}
            lifetimeRigsBought={miner?.lifetimeRigsBought ?? 0n}
          />
          <GroveStatus
            config={config}
            rewardVaultLamports={snap.rewardVaultDistributable}
            chainTime={chainTime}
          />

          {phase === 'preLaunch' &&
          host.address !== null &&
          host.address === config.founder &&
          host.signer ? (
            <FounderActivation founder={host.address} signer={host.signer} onDone={refresh} />
          ) : null}

          {/* 4 · The primary action. */}
          <BuyPanel
            config={config}
            chainTime={chainTime}
            walletLamports={walletLamports}
            disabled={!canAct || !freshBuyOpen}
            closedReason={
              freshBuyOpen
                ? null
                : genesisRemaining === 0n
                  ? 'This wallet has used its Genesis fresh-buy allowance. Replanting remains available.'
                : phase === 'preLaunch'
                  ? 'Planting opens when the founder activates the grove.'
                  : 'Planting is not open to this wallet in the current access phase.'
            }
            genesisRemaining={genesisRemaining}
            referralNote={referralNote}
            officialFallback={referral?.kind === 'official'}
            hasPosition={hasPosition}
            accruedRewardLamports={claimable}
            busy={busy === 'buy'}
            onBuy={onBuy}
            error={buyError}
          />

          {/* 5 · The position and its two actions. */}
          <MinerCockpit
            config={config}
            miner={miner}
            claimable={claimable}
            rewardVaultLamports={snap.rewardVaultDistributable}
            chainTime={chainTime}
            currentOutputPerDay={outputPerDay}
            canAct={canAct}
            busy={busy}
            onReplant={onCompound}
            onHarvest={() => setConfirmingClaim(true)}
            claimLabel={CLAIM_ACTION_LABEL}
            error={positionError}
          />

          {confirmingClaim && claimQuote ? (
            <ClaimConfirm
              quote={claimQuote}
              compoundFeeBps={config.compoundFeeBps}
              busy={busy === 'claim'}
              onConfirm={() => {
                setConfirmingClaim(false);
                onClaim();
              }}
              onCancel={() => setConfirmingClaim(false)}
            />
          ) : null}

          {/* 6 · The decoded account behind the position. */}
          {miner ? <GroveRecord miner={miner} chainTime={chainTime} /> : null}

          <hr className="rule" />

          {/* 7 · Status and social: milestones, referrals, the room. */}
          {miner ? <MilestonesPanel config={config} miner={miner} chainTime={chainTime} /> : null}
          <ReferralPanel wallet={host.address} hasPosition={hasPosition} />
          {host.address ? (
            <CommissionPanel
              wallet={host.address}
              signer={host.walletConnected ? host.signer : null}
              paused={paused}
              onClaimed={() => void refresh()}
            />
          ) : null}
          {host.room ? (
            <RoomLeaderboard
              roomId={host.room.id}
              roomTitle={host.room.title}
              launchToken={host.launchToken}
              viewer={host.address}
              onOpenProfile={host.openProfile}
            />
          ) : null}

          {/* 8 · Share the position; see how it dilutes. */}
          <ProgressShareCard
            wallet={host.address}
            miner={miner}
            chainTime={chainTime}
            onShare={host.share}
          />
          <DilutionPanel config={config} miner={miner} chainTime={chainTime} />

          <hr className="rule" />

          {/* 9 · History: this wallet's, then the whole grove's. */}
          <ActivityHistory wallet={host.address} refreshKey={miner?.lastActionAt ?? 0n} />
          <DistributionPanel viewer={host.address} />

          <hr className="rule" />

          {/* 10 · Verification: vaults, accounts, authorities. */}
          <TransparencyPanel
            config={config}
            miner={miner}
            rewardVaultLamports={snap.rewardVaultDistributable}
            rewardVaultAddress={snap.rewardVaultAddress}
            capitalVaultLamports={snap.capitalVaultDistributable ?? 0n}
            capitalVaultAddress={snap.capitalVaultAddress ?? null}
            feeVaultLamports={snap.feeVaultDistributable ?? 0n}
            feeVaultAddress={snap.feeVaultAddress ?? null}
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
