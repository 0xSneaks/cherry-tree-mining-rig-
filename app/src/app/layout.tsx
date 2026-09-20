import type { Metadata, Viewport } from 'next';

import './globals.css';

export const metadata: Metadata = {
  title: 'SOL RIGS',
  description:
    'A Solana-native mining simulation with SOL-denominated rewards, paid from a funded reward vault.',
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
