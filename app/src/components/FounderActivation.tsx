'use client';

/**
 * FOUNDER ACTIVATION — the one deliberate first planting.
 *
 * Rendered only for the pinned founder wallet while the grove is pre-launch.
 * Nothing here activates anything on its own:
 *
 *   1. the founder types the seed count (there is no default);
 *   2. "Run preflight" reads the chain and simulates the exact instruction;
 *   3. only a fully passing preflight unlocks the confirmation field, which
 *      must read exactly `ACTIVATE <count>`;
 *   4. submitting re-runs the whole preflight and simulation, and sends ONE
 *      transaction only if that fresh preflight passes too.
 *
 * The program is still the authority: it re-verifies the founder, the live
 * `sneaks.skr` ownership and the funded schedule inside the purchase, and
 * writes T0 only if the purchase succeeds.
 */
import { useRef, useState } from 'react';
import type { Address } from '@solana/kit';

import { confirmSignature, rpc } from '../lib/chain';
import { CLUSTER } from '../lib/env';
import { formatSol } from '../lib/format';
import { gatherPreflight } from '../lib/launch/gather';
import { GENESIS_HASH, type Cluster } from '../lib/launch/expected';
import {
  confirmationPhrase,
  evaluatePreflight,
  type PreflightResult,
} from '../lib/launch/preflight';
import type { RigsSigner } from '../lib/signer';
import { formatAprBps } from '../lib/program/yield';

type Stage = 'idle' | 'checking' | 'ready' | 'submitting' | 'sent' | 'failed';

const INTENDED: Cluster | null = CLUSTER in GENESIS_HASH ? (CLUSTER as Cluster) : null;

export function FounderActivation({
  founder,
  signer,
  onDone,
}: {
  founder: Address;
  signer: RigsSigner;
  onDone: () => void;
}) {
  const [seedsText, setSeedsText] = useState('');
  const [phrase, setPhrase] = useState('');
  const [stage, setStage] = useState<Stage>('idle');
  const [result, setResult] = useState<PreflightResult | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [signature, setSignature] = useState<string | null>(null);
  // Belt and braces: one submission per mounted panel, whatever the UI does.
  const submitted = useRef(false);

  const seeds = /^[1-9][0-9]*$/.test(seedsText) ? BigInt(seedsText) : null;

  const preflight = async () => {
    const { input, founderMiner } = await gatherPreflight(rpc(), founder, seeds, INTENDED);
    return { r: evaluatePreflight(input, founderMiner), ix: input.instruction };
  };

  const runPreflight = async () => {
    setStage('checking');
    setMessage(null);
    setPhrase('');
    try {
      const { r } = await preflight();
      setResult(r);
      setStage(r.ok ? 'ready' : 'idle');
    } catch (e) {
      setMessage(e instanceof Error ? e.message : String(e));
      setStage('idle');
    }
  };

  const activate = async () => {
    if (submitted.current || seeds === null || phrase !== confirmationPhrase(seeds)) return;
    setStage('submitting');
    setMessage(null);
    try {
      // Simulate again, on fresh state, immediately before the one send.
      const { r, ix } = await preflight();
      setResult(r);
      if (!r.ok || !ix) {
        setStage('idle');
        setMessage('The fresh preflight did not pass. Nothing was sent.');
        return;
      }
      submitted.current = true;
      const sig = await signer.signAndSend(ix);
      setSignature(sig);
      await confirmSignature(sig, rpc());
      setStage('sent');
      onDone();
    } catch (e) {
      setStage(submitted.current ? 'failed' : 'idle');
      setMessage(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <section className="panel founder" data-testid="founder-activation">
      <h2 className="section-title">Founder activation</h2>
      <p className="note">
        One deliberate first planting by sneaks.skr activates the grove. T0 is
        that transaction&apos;s chain time and never changes.
      </p>

      {stage !== 'sent' && stage !== 'failed' ? (
        <>
          <label className="readout-label" htmlFor="founder-seeds">
            Seeds to plant
          </label>
          <div className="stepper founder-input">
            <input
              id="founder-seeds"
              inputMode="numeric"
              pattern="[0-9]*"
              placeholder="enter an amount"
              value={seedsText}
              disabled={stage === 'checking' || stage === 'submitting'}
              onChange={(e) => {
                setSeedsText(e.target.value.replace(/\D/g, ''));
                setResult(null);
                setStage('idle');
              }}
            />
          </div>
          <button
            type="button"
            onClick={runPreflight}
            disabled={seeds === null || stage === 'checking' || stage === 'submitting'}
          >
            {stage === 'checking' ? 'Checking…' : 'Run preflight'}
          </button>
        </>
      ) : null}

      {result ? (
        <ul className="preflight" data-testid="preflight-checks">
          {result.checks.map((c) => (
            <li key={c.name} className={c.pass ? 'pass' : 'fail'}>
              <span>{c.pass ? 'PASS' : 'FAIL'}</span> {c.name}
              <span className="note"> — {c.detail}</span>
            </li>
          ))}
        </ul>
      ) : null}

      {result?.ok && result.quote && seeds !== null && stage === 'ready' ? (
        <>
          <p className="note">
            {seeds.toString()} seeds · {formatSol(result.quote.totalLamports, 4)} SOL incl.{' '}
            {formatSol(result.quote.feeLamports, 4)} fee · APR after planting{' '}
            {result.aprBpsAfter !== null ? formatAprBps(result.aprBpsAfter) : '—'}
          </p>
          <label className="readout-label" htmlFor="founder-confirm">
            Type {confirmationPhrase(seeds)} to confirm the one live transaction
          </label>
          <div className="stepper founder-input">
            <input
              id="founder-confirm"
              value={phrase}
              autoComplete="off"
              onChange={(e) => setPhrase(e.target.value)}
            />
          </div>
          <button
            type="button"
            className="primary"
            onClick={activate}
            disabled={phrase !== confirmationPhrase(seeds)}
          >
            Simulate again &amp; activate
          </button>
        </>
      ) : null}

      {stage === 'submitting' ? <p className="note">Simulating, then submitting once…</p> : null}
      {signature ? (
        <p className="note">
          Signature {signature}. Verify with scripts/ops/verify-first-buy.ts — then stop.
        </p>
      ) : null}
      {message ? <div className="banner banner--alert">{message}</div> : null}
    </section>
  );
}
