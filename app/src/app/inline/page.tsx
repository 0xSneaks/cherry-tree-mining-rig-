/**
 * `/inline?token=…` — the server-rendered blink card.
 *
 * A server component, so the card is HTML on first byte: the receiver's chat
 * shows the snapshot without waiting for a client bundle to mount.
 *
 * The snapshot is read from the *signed* token's `params`. Unsigned query
 * fields are ignored entirely — passing snapshot data as separate query
 * fields would make it forgeable, so tampering with the URL outside the token
 * changes nothing on the card (spec §8, §12.27).
 *
 * Keyed by `payload.message_id`, so a given blink message always renders the
 * same snapshot.
 */
import { PRODUCT_NAME } from '../../lib/brand';
import { ResultCard, type ResultParams } from '../../components/ResultCard';
import { verifySession } from '../../lib/server/launchToken';

export const dynamic = 'force-dynamic';

function str(v: unknown): string | undefined {
  return typeof v === 'string' ? v : typeof v === 'number' ? String(v) : undefined;
}

export default async function InlinePage({
  searchParams,
}: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const sp = await searchParams;
  const raw = sp.token;
  const token = Array.isArray(raw) ? raw[0] : raw;

  if (!token) return <Expired reason="This card has no launch token." />;

  let session;
  try {
    session = await verifySession(token);
  } catch {
    // Launch tokens live five minutes. An old card is expired, not broken.
    return <Expired reason="This card has expired." />;
  }

  const signed = session.params ?? {};
  const params: ResultParams = {
    kind: str(signed.kind),
    rigs: str(signed.rigs),
    compounded: str(signed.compounded),
    claimed: str(signed.claimed),
    rooted: str(signed.rooted),
    milestone: str(signed.milestone),
    ref: str(signed.ref),
  };

  return (
    <div data-cherry-message-id={session.messageId ?? undefined}>
      <ResultCard params={params} />
    </div>
  );
}

function Expired({ reason }: { reason: string }) {
  return (
    <main className="shell" style={{ maxWidth: 480 }}>
      <div className="panel">
        <p className="section-title" style={{ marginBottom: 6 }}>
          {PRODUCT_NAME}
        </p>
        <p className="note">{reason}</p>
      </div>
    </main>
  );
}
