'use client';

/**
 * REFERRAL COMMISSION — the referrer's own on-chain ledger and its claim
 * (architecture §7 / §7a item 4).
 *
 * Every figure is read from the canonical ledger PDA `["referral", wallet]`
 * of the connected wallet; nothing is estimated. The claim instruction is
 * built for that same PDA and signed by that same wallet, and the program
 * re-derives the PDA from the signer and checks `has_one = referrer`, so no
 * other ledger or recipient can be substituted. One submission at a time;
 * the ledger is re-read from chain after every attempt, success or failure.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import type { Address } from '@solana/kit';

import { claimCommission } from '../lib/actions';
import { fetchReferralLedger } from '../lib/chain';
import { explainActionError } from '../lib/errors';
import { formatSol } from '../lib/format';
import { commissionClaimBlock, commissionView, type CommissionView } from '../lib/program/fees';
import type { RigsSigner } from '../lib/signer';
import { Readout } from './Readout';

type Load = { key: string; view: CommissionView } | { key: string; error: string };

export function CommissionPanel({
  wallet,
  signer,
  paused,
  onClaimed,
}: {
  wallet: Address;
  signer: RigsSigner | null;
  paused: boolean;
  onClaimed?: () => void;
}) {
  const [load, setLoad] = useState<Load | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  // One submission at a time, whatever the rendering does.
  const inFlight = useRef(false);

  const read = useCallback(
    (): Promise<Load> =>
      fetchReferralLedger(wallet).then(
        (ledger) => ({ key: wallet, view: commissionView(ledger, wallet) }),
        (e: unknown) => ({ key: wallet, error: e instanceof Error ? e.message : String(e) }),
      ),
    [wallet],
  );
  const reload = useCallback(async () => setLoad(await read()), [read]);

  useEffect(() => {
    let cancelled = false;
    void read().then((l) => {
      if (!cancelled) setLoad(l);
    });
    return () => {
      cancelled = true;
    };
  }, [read]);

  const current = load?.key === wallet ? load : null;
  const view = current && 'view' in current ? current.view : null;
  const block = view
    ? commissionClaimBlock({
        view,
        wallet,
        signerAddress: signer?.address ?? null,
        paused,
        busy,
      })
    : 'Loading…';

  const onClaim = async () => {
    if (!signer || block !== null || inFlight.current) return;
    inFlight.current = true;
    setBusy(true);
    setMessage(null);
    try {
      const sig = await claimCommission(signer);
      setMessage({ ok: true, text: `Commission claimed · ${sig.slice(0, 8)}…` });
      onClaimed?.();
    } catch (e) {
      const explained = explainActionError(e);
      console.error('[cherry-tree] claim_commission failed:', explained.raw);
      setMessage({
        ok: false,
        text: `${explained.message} The figures below are re-read from chain.`,
      });
    } finally {
      await reload();
      inFlight.current = false;
      setBusy(false);
    }
  };

  return (
    <section className="panel" data-testid="commission-panel" aria-label="Referral commission">
      <h2 className="section-title">Referral commission</h2>
      <p className="note">
        When a planter who joined with your link pays a protocol fee on a seed purchase or a
        replant, you accrue up to 20% of that fee on-chain and the Protocol Treasury keeps the
        other 80%. The commission is reserved inside the fee vault until you claim it. It is
        clamped so the treasury always keeps 8% of the amount planted: at a 10% fee you get the
        full 20% of the fee; at a lower fee it shrinks, and at 8% or below it is zero. Early-claim
        fees never earn commission. One level only.
      </p>
      {current && 'error' in current ? (
        <p className="note" role="alert">
          Could not read your commission ledger: {current.error}
        </p>
      ) : null}
      {view?.state === 'none' ? (
        <p className="note" data-testid="commission-none">
          No commission has accrued to this wallet yet.
        </p>
      ) : null}
      {view?.state === 'invalid' ? (
        <p className="note" role="alert">
          The commission account for this wallet does not match it. Nothing can be claimed.
        </p>
      ) : null}
      {view?.state === 'ready' ? (
        <>
          <Readout label="Accrued (all time)" value={formatSol(view.accrued, 6)} unit="SOL" />
          <Readout label="Claimed" value={formatSol(view.claimed, 6)} unit="SOL" />
          <Readout label="Claimable now" value={formatSol(view.claimable, 6)} unit="SOL" />
        </>
      ) : null}
      {paused ? (
        <p className="note" data-testid="commission-paused">
          The program is paused, so commission claims are blocked until it resumes. Your reserved
          commission stays in the fee vault meanwhile; fee withdrawals can never touch it.
        </p>
      ) : null}
      <div className="actions">
        <button
          type="button"
          className="primary"
          data-testid="commission-claim"
          onClick={() => void onClaim()}
          disabled={block !== null}
          title={block ?? undefined}
        >
          {busy ? 'Claiming…' : 'Claim commission'}
        </button>
      </div>
      {block && view && !busy ? <p className="note">{block}</p> : null}
      {message ? (
        <p className="note" role={message.ok ? 'status' : 'alert'} data-testid="commission-message">
          {message.text}
        </p>
      ) : null}
    </section>
  );
}
