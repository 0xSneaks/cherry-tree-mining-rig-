'use client';

/**
 * Whether `wallet` is an existing planter on chain — the condition the
 * program checks before it binds a referrer. `null` while unknown or when the
 * read fails; callers treat unknown as "cannot verify" (the official fallback
 * fails closed on it).
 */
import { useEffect, useState } from 'react';
import { getBase64Encoder, type Address } from '@solana/kit';

import { rpc } from '../lib/chain';
import { decodeMiner } from '../lib/program/accounts';
import { PROGRAM_ID } from '../lib/program/constants';
import { minerPda } from '../lib/program/pdas';

export async function fetchIsPlanter(wallet: Address): Promise<boolean> {
  const pda = await minerPda(wallet);
  const acc = await rpc().getAccountInfo(pda, { encoding: 'base64' }).send();
  if (!acc.value || acc.value.owner !== PROGRAM_ID) return false;
  try {
    const m = decodeMiner(new Uint8Array(getBase64Encoder().encode(acc.value.data[0])));
    return m.owner === wallet && m.rigs > 0n;
  } catch {
    return false;
  }
}

export function usePlanterStatus(wallet: Address | null): boolean | null {
  const [state, setState] = useState<{ wallet: Address | null; value: boolean | null }>({
    wallet: null,
    value: null,
  });
  useEffect(() => {
    if (!wallet) return;
    let cancelled = false;
    fetchIsPlanter(wallet)
      .then((v) => {
        if (!cancelled) setState({ wallet, value: v });
      })
      .catch(() => {
        if (!cancelled) setState({ wallet, value: null });
      });
    return () => {
      cancelled = true;
    };
  }, [wallet]);
  return wallet && state.wallet === wallet ? state.value : null;
}
