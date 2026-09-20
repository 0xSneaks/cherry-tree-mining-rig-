/**
 * `/result` — the blink render target.
 *
 * A pure render of `params`. No chain reads, no state, no callbacks. The
 * authoritative copy of these params lives inside the signed launch token;
 * `/inline` renders that server-side. This page is the client-side equivalent
 * for a direct open, and it shows only what it is handed.
 */
import { ResultCard, type ResultParams } from '../../components/ResultCard';

export const dynamic = 'force-dynamic';

export default async function ResultPage({
  searchParams,
}: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const sp = await searchParams;
  const pick = (k: string): string | undefined => {
    const v = sp[k];
    return Array.isArray(v) ? v[0] : v;
  };

  return (
    <ResultCard
      params={
        {
          kind: pick('kind'),
          rigs: pick('rigs'),
          compounded: pick('compounded'),
          claimed: pick('claimed'),
        } satisfies ResultParams
      }
    />
  );
}
