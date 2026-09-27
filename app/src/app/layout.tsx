import type { Metadata, Viewport } from 'next';

import { PRODUCT_DESCRIPTION, PRODUCT_NAME } from '../lib/brand';

import './globals.css';
import './grove.css';

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
  themeColor: '#0b0c0d',
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
