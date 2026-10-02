import type { Metadata, Viewport } from 'next';
import { connection } from 'next/server';

import { PRODUCT_DESCRIPTION, PRODUCT_NAME } from '../lib/brand';

import './globals.css';

export const metadata: Metadata = {
  title: PRODUCT_NAME,
  description: PRODUCT_DESCRIPTION,
};

/** Mobile first, 390 × 844, inside the Cherry WebView on Seeker. */
export const viewport: Viewport = {
  width: 'device-width',
  initialScale: 1,
  maximumScale: 1,
  viewportFit: 'cover',
  themeColor: '#080c16',
};

export default async function RootLayout({ children }: { children: React.ReactNode }) {
  // On a Node deployment the CSP carries a per-request nonce (`src/proxy.ts`),
  // so every HTML route renders per request: touching the request opts the
  // route out of static rendering, because a build-time prerender cannot
  // carry a nonce and would arrive with every script in it blocked. A static
  // export has no proxy and no nonce — its scripts are admitted by hash
  // instead (`scripts/emit-static-headers.ts`) — so it keeps the prerender.
  if (process.env.STATIC_EXPORT !== '1') await connection();

  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
