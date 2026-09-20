import { GameRoot } from '../components/GameRoot';

/**
 * Environment detection depends on `window`, so the whole root is client-side.
 * The embedded path must reach first paint with the user's rigs and no user
 * action, which means no server-side wallet guessing (spec §12.21).
 */
export default function Page() {
  return <GameRoot />;
}
