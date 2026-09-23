'use client';

/**
 * Standalone path — second-class but working (spec §12.25).
 *
 * Plain wallet-standard: list installed Solana wallets, connect one, and hand
 * the resulting account to kit's sending-signer plumbing. Everything after
 * that is the same `Game` component the embed renders.
 */
import { useCallback, useMemo, useState } from 'react';
import { address } from '@solana/kit';
import {
  useConnect,
  useWallets,
  type UiWallet,
  type UiWalletAccount,
} from '@wallet-standard/react';
import { useWalletAccountTransactionSendingSigner } from '@solana/react';

import { CLUSTER } from '../lib/env';
import { walletStandardSigner } from '../lib/signer';
import { CommunityChat } from './CommunityChat';
import { Game, type Host } from './Game';

const CHAIN = (CLUSTER === 'mainnet-beta'
  ? 'solana:mainnet'
  : `solana:${CLUSTER}`) as `solana:${string}`;

function supportsSolana(wallet: UiWallet): boolean {
  return wallet.chains.some((c) => c.startsWith('solana:'));
}

function ConnectButton({
  wallet,
  onConnected,
}: {
  wallet: UiWallet;
  onConnected: (account: UiWalletAccount) => void;
}) {
  const [isConnecting, connect] = useConnect(wallet);
  return (
    <button
      type="button"
      disabled={isConnecting}
      onClick={async () => {
        const accounts = await connect();
        const first = accounts[0];
        if (first) onConnected(first);
      }}
    >
      {isConnecting ? 'Connecting…' : wallet.name}
    </button>
  );
}

/**
 * Hooks cannot be called conditionally, so the signer lives in its own
 * component that only mounts once an account exists.
 */
function ConnectedGame({ account }: { account: UiWalletAccount }) {
  const sendingSigner = useWalletAccountTransactionSendingSigner(account, CHAIN);

  const host: Host = useMemo(
    () => ({
      mode: 'standalone',
      address: address(account.address),
      signer: walletStandardSigner(sendingSigner),
      walletConnected: true,
      // No room context outside Cherry — the room UI stays hidden.
      room: null,
      launchToken: null,
      lifecycle: null,
    }),
    [account.address, sendingSigner],
  );

  return (
    <>
      <Game host={host} />
      <CommunityChat />
    </>
  );
}

export function StandaloneGame() {
  const wallets = useWallets();
  const [account, setAccount] = useState<UiWalletAccount | null>(null);

  const solanaWallets = useMemo(() => wallets.filter(supportsSolana), [wallets]);
  const onConnected = useCallback((a: UiWalletAccount) => setAccount(a), []);

  if (account) return <ConnectedGame account={account} />;

  // Read-only view with a connect prompt. This branch never renders in the
  // embed — `GameRoot` picks `EmbeddedGame` there.
  const host: Host = {
    mode: 'standalone',
    address: null,
    signer: null,
    walletConnected: false,
    room: null,
    launchToken: null,
    lifecycle: null,
    connectSlot: (
      <span style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
        {solanaWallets.length === 0 ? (
          <span className="readout-label">No Solana wallet detected</span>
        ) : (
          solanaWallets.map((w) => (
            <ConnectButton key={w.name} wallet={w} onConnected={onConnected} />
          ))
        )}
      </span>
    ),
  };

  return (
    <>
      <Game host={host} />
      <CommunityChat />
    </>
  );
}
