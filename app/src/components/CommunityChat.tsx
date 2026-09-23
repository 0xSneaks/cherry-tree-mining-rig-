'use client';

/**
 * Cherry Chat, embedded in the standalone app. Auth mode `app-trusted+wallet`.
 *
 * Two hard boundaries, enforced here rather than by convention:
 *
 *  1. It never mounts inside the Cherry mini app. `useCherryEnvironment`
 *     decides, strictly. Inside Cherry the user is already in the room, so
 *     framing Cherry inside a Cherry mini app would be recursive — and the
 *     mini-app path (launch token, wallet bridge, blink card) is not to be
 *     disturbed by this surface at all.
 *  2. It renders nothing when unconfigured. No `appId`/`roomId` means no
 *     iframe, no placeholder, no layout shift, and the SDK is never imported.
 *
 * The auth shape Cherry specifies:
 *
 *   - `signChallengeHandler` is registered in the CONSTRUCTOR, never after
 *     `mount()`. The iframe can ask for a signature the moment its bridge is
 *     ready, and `chat.onSignChallenge(...)` after mounting races that.
 *   - The handler is stable and reads a ref, so it exists before any wallet
 *     does. Connecting later fills the ref rather than re-registering.
 *   - `walletConnectRequested` fires when the user clicks connect inside the
 *     read-only preview. Without a handler that re-arms auth, that button is a
 *     no-op — so it mints a fresh token and calls `setToken` +
 *     `setWalletAddress`.
 *
 * Signing here is a proof of wallet ownership for chat identity. It authorises
 * nothing on chain and shares nothing with the mining signer.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { createSignableMessage } from '@solana/kit';
import { useWalletAccountMessageSigner } from '@solana/react';
import { useCherryEnvironment } from '@cherrydotfun/miniapp-sdk/react';
import type { UiWalletAccount } from '@wallet-standard/react';

import {
  CHERRY_CHAT_LAYOUT,
  CHERRY_CHAT_THEME,
  CHERRY_EMBED_ORIGIN,
  EMBED_NONCE_ROUTE,
  EMBED_TOKEN_ROUTE,
  cherryChatConfig,
} from '../lib/cherryChat';

/** What this component uses from the SDK, narrowed. */
type Embed = {
  mount(): Promise<void>;
  destroy(): void;
  setToken(token: string): void;
  setWalletAddress(address: string): void;
  on(event: string, handler: (...args: unknown[]) => void): void;
};

/** Signs a message and returns the raw 64-byte signature. */
type SignMessage = (message: Uint8Array) => Promise<Uint8Array>;

/**
 * Prove the wallet, then ask the backend for a token.
 *
 * The nonce is the server's; the signature is the wallet's. Neither the token
 * nor the secret that signs it is ever constructed here.
 */
async function mintToken(
  walletAddress: string,
  signMessage: SignMessage,
): Promise<string> {
  const nonceRes = await fetch(EMBED_NONCE_ROUTE, { method: 'POST' });
  if (!nonceRes.ok) throw new Error(`nonce request failed: ${nonceRes.status}`);
  const { nonce, message } = (await nonceRes.json()) as {
    nonce: string;
    message: string;
  };

  const signature = await signMessage(new TextEncoder().encode(message));

  const tokenRes = await fetch(EMBED_TOKEN_ROUTE, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      walletAddress,
      nonce,
      signature: btoa(String.fromCharCode(...signature)),
    }),
  });
  if (!tokenRes.ok) throw new Error(`token request failed: ${tokenRes.status}`);
  const { token } = (await tokenRes.json()) as { token: string };
  return token;
}

function ChatEmbed({
  walletAddress,
  signMessage,
  onConnectRequested,
}: {
  walletAddress: string | null;
  signMessage: SignMessage | null;
  onConnectRequested?: () => void;
}) {
  const container = useRef<HTMLDivElement | null>(null);
  const [failed, setFailed] = useState(false);

  // The sign handler is registered once, in the constructor, but the wallet
  // can arrive later — so it reads the latest signer through a ref.
  const signer = useRef<SignMessage | null>(signMessage);
  signer.current = signMessage;
  const wallet = useRef<string | null>(walletAddress);
  wallet.current = walletAddress;

  const config = cherryChatConfig();

  const requestConnect = useCallback(() => onConnectRequested?.(), [onConnectRequested]);

  useEffect(() => {
    if (!config || !container.current) return;

    let embed: Embed | null = null;
    let cancelled = false;

    void (async () => {
      try {
        // Lazy: the SDK touches `document` on construction, and this keeps it
        // out of the bundle entirely when no embed is configured.
        const { CherryEmbed } = await import('@cherrydotfun/chat-embed-sdk');
        if (cancelled || !container.current) return;

        // A wallet already connected? Mint before mounting, as Cherry asks.
        let token: string | undefined;
        if (wallet.current && signer.current) {
          try {
            token = await mintToken(wallet.current, signer.current);
          } catch {
            // Fall through: the iframe opens read-only and the user can
            // re-arm auth from its own connect button.
          }
        }
        if (cancelled) return;

        embed = new CherryEmbed({
          appId: config.appId,
          roomId: config.roomId,
          embedUrl: CHERRY_EMBED_ORIGIN,
          container: container.current,
          mode: 'single',
          position: 'inline',
          ...(token ? { token } : {}),
          ...(wallet.current ? { walletAddress: wallet.current } : {}),
          theme: { ...CHERRY_CHAT_THEME },
          layout: { ...CHERRY_CHAT_LAYOUT },
          // Registered here, never after mount() — the iframe can ask for a
          // signature as soon as its bridge is ready.
          signChallengeHandler: async (message: Uint8Array) => {
            const sign = signer.current;
            if (!sign) throw new Error('no wallet is connected');
            return sign(message);
          },
        }) as unknown as Embed;

        // The in-iframe "connect wallet to send messages" button. Without
        // this the click does nothing.
        embed.on('walletConnectRequested', () => {
          void (async () => {
            requestConnect();
            const address = wallet.current;
            const sign = signer.current;
            if (!address || !sign || !embed) return;
            try {
              const fresh = await mintToken(address, sign);
              embed.setToken(fresh);
              embed.setWalletAddress(address);
            } catch {
              // Leave the preview as it is; the user can try again.
            }
          })();
        });

        await embed.mount();
        if (cancelled) {
          embed.destroy();
          embed = null;
        }
      } catch {
        // A chat that will not load must not take the mine down with it.
        if (!cancelled) setFailed(true);
      }
    })();

    return () => {
      cancelled = true;
      embed?.destroy();
      embed = null;
    };
    // Re-mounting on an account switch is intentional: it forces a fresh
    // token exchange rather than reusing the previous wallet's session.
  }, [config?.appId, config?.roomId, walletAddress, requestConnect]);

  if (!config) return null;

  return (
    <section className="panel community" data-testid="community-chat">
      <p className="section-title" style={{ marginBottom: 10 }}>
        Community
      </p>
      {failed ? (
        <p className="note">Chat is unavailable right now.</p>
      ) : (
        <div
          ref={container}
          // The embed fills its container, so the container needs a size of
          // its own or it collapses to nothing.
          style={{ width: '100%', height: 520, borderRadius: 2, overflow: 'hidden' }}
        />
      )}
    </section>
  );
}

/** Wallet connected: hooks are legal here because the account is non-null. */
function SignedChat({
  account,
  onConnectRequested,
}: {
  account: UiWalletAccount;
  onConnectRequested?: () => void;
}) {
  const messageSigner = useWalletAccountMessageSigner(account);

  const signMessage = useCallback<SignMessage>(
    async (message) => {
      const [signed] = await messageSigner.modifyAndSignMessages([
        createSignableMessage(message),
      ]);
      const signature = signed?.signatures[messageSigner.address];
      if (!signature) throw new Error('the wallet returned no signature');
      return signature;
    },
    [messageSigner],
  );

  return (
    <ChatEmbed
      walletAddress={account.address}
      signMessage={signMessage}
      onConnectRequested={onConnectRequested}
    />
  );
}

export function CommunityChat({
  account = null,
  onConnectRequested,
}: {
  account?: UiWalletAccount | null;
  onConnectRequested?: () => void;
}) {
  const { isEmbedded } = useCherryEnvironment({ strict: true });
  if (isEmbedded) return null;

  return account ? (
    <SignedChat account={account} onConnectRequested={onConnectRequested} />
  ) : (
    <ChatEmbed
      walletAddress={null}
      signMessage={null}
      onConnectRequested={onConnectRequested}
    />
  );
}
