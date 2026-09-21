'use client';

/**
 * Embedded path — primary.
 *
 * There is no connect-wallet step here, by design: the wallet arrives with the
 * Cherry handshake. Rendering a connect button in the embed would be wrong
 * (spec §8, §12.21).
 */
import { useEffect, useMemo, useRef, useState } from 'react';
import { address } from '@solana/kit';
import {
  useCherryApp,
  useCherryMiniApp,
  useCherryNavigate,
  useCherryShare,
  useCherryWallet,
} from '@cherrydotfun/miniapp-sdk/react';

import { PRODUCT_NAME } from '../lib/brand';
import { cherrySigner } from '../lib/signer';
import { Game, type Host } from './Game';
import type { SharePayload } from './ShareCard';

export function EmbeddedGame() {
  const app = useCherryApp();
  const { user, room, launchToken, isReady, error } = useCherryMiniApp();
  const wallet = useCherryWallet();
  const navigate = useCherryNavigate();
  const share = useCherryShare();

  // The SDK emits `walletDisconnected`; track it so the three action buttons
  // can be disabled without unmounting any state.
  const [walletLost, setWalletLost] = useState(false);
  useEffect(() => {
    if (!app) return;
    const onLost = () => setWalletLost(true);
    app.on('walletDisconnected', onLost);
    return () => app.off('walletDisconnected', onLost);
  }, [app]);

  // `CherryMiniAppProvider` owns init and calls `destroy()` on unmount; this
  // ref exists only so the lifecycle source identity stays stable.
  const lifecycle = useRef<Host['lifecycle']>(null);
  lifecycle.current = app
    ? {
        on: (e, fn) => app.on(e, fn),
        off: (e, fn) => app.off(e, fn),
      }
    : null;

  const walletAddress = wallet.publicKey ?? user?.publicKey ?? null;

  const host: Host = useMemo(() => {
    const addr = walletAddress ? address(walletAddress) : null;
    return {
      mode: 'embedded',
      address: addr,
      signer:
        addr && wallet.connected && !walletLost
          ? cherrySigner(
              {
                publicKey: wallet.publicKey,
                signAndSendTransaction: wallet.signAndSendTransaction,
              },
              addr,
            )
          : null,
      walletConnected: wallet.connected && !walletLost,
      room: room
        ? { id: room.id, title: room.title, memberCount: room.memberCount }
        : null,
      launchToken,
      lifecycle: lifecycle.current,
      openProfile: (a: string) => void navigate.userProfile(a),
      share: async (payload: SharePayload) => {
        await share({
          route: '/result',
          // Snapshot data travels inside the signed token's `params`, never as
          // separate query fields — those would be forgeable (spec §8).
          params: payload as unknown as Record<string, unknown>,
          height: 'medium',
          caption:
            payload.kind === 'compound'
              ? `+${payload.compounded} rigs`
              : 'Claimed from the mine',
          previewable: true,
        });
      },
    };
  }, [walletAddress, wallet, walletLost, room, launchToken, navigate, share]);

  if (error) {
    return (
      <main className="shell">
        <div className="banner banner--alert">
          Cherry handshake failed: {error.message}
        </div>
      </main>
    );
  }
  if (!isReady) {
    return (
      <main className="shell">
        <h1 className="wordmark">{PRODUCT_NAME}</h1>
        <p className="note" style={{ marginTop: 10 }}>
          Spinning up the shaft…
        </p>
      </main>
    );
  }

  return <Game host={host} />;
}
