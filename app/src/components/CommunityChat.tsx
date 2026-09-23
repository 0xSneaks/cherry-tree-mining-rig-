'use client';

/**
 * Cherry Chat, embedded in the standalone app.
 *
 * Two hard rules, both enforced here rather than by convention:
 *
 *  1. It never mounts inside the Cherry mini app. `useCherryEnvironment`
 *     decides, strictly. Inside Cherry the user is already in the room, so
 *     framing Cherry inside a Cherry mini app would be recursive — and the
 *     mini-app path (launch token, wallet bridge, blink card) is not to be
 *     disturbed by this surface at all.
 *  2. It renders nothing when unconfigured. No `appId`/`roomId` means no
 *     iframe, no placeholder and no layout shift.
 *
 * Auth is `wallet-only`: the iframe runs its own wallet connect and signing.
 * The game's own wallet — Cherry's bridge in the embed, wallet-standard in
 * standalone — is deliberately not wired into it. Chat identity and the
 * signer that moves SOL stay separate concerns, and nothing about the mining
 * session is handed to the chat.
 */
import { useEffect, useRef, useState } from 'react';
import { useCherryEnvironment } from '@cherrydotfun/miniapp-sdk/react';

import {
  CHERRY_CHAT_LAYOUT,
  CHERRY_CHAT_THEME,
  cherryChatConfig,
} from '../lib/cherryChat';

/** What the SDK exposes, narrowed to what this component uses. */
type Embed = {
  mount(): Promise<void>;
  destroy(): void;
};

export function CommunityChat() {
  const { isEmbedded } = useCherryEnvironment({ strict: true });
  const container = useRef<HTMLDivElement | null>(null);
  const [failed, setFailed] = useState(false);

  const config = cherryChatConfig();
  const active = !isEmbedded && config !== null;

  useEffect(() => {
    if (!active || !container.current) return;

    // Imported lazily: the SDK touches `document` at construction, and this
    // keeps it out of the bundle entirely for a build with no chat configured.
    let embed: Embed | null = null;
    let cancelled = false;

    void (async () => {
      try {
        const { CherryEmbed } = await import('@cherrydotfun/chat-embed-sdk');
        if (cancelled || !container.current) return;

        embed = new CherryEmbed({
          appId: config.appId,
          roomId: config.roomId,
          container: container.current,
          position: 'inline',
          theme: { ...CHERRY_CHAT_THEME },
          layout: { ...CHERRY_CHAT_LAYOUT },
        }) as Embed;

        await embed.mount();
        // A late unmount can land between the await and here.
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
  }, [active, config?.appId, config?.roomId]);

  if (!active) return null;

  return (
    <section className="panel" style={{ marginTop: 18 }} data-testid="community-chat">
      <p className="section-title" style={{ marginBottom: 10 }}>
        Community
      </p>
      {failed ? (
        <p className="note">Chat is unavailable right now.</p>
      ) : (
        <div
          ref={container}
          // The embed sizes itself to its container, so the container has to
          // have a size of its own or it collapses to nothing.
          style={{ width: '100%', height: 520, borderRadius: 2, overflow: 'hidden' }}
        />
      )}
    </section>
  );
}
