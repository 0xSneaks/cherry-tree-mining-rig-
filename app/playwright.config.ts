import { defineConfig, devices } from '@playwright/test';

/**
 * Browser tests run against a real production build, so what is asserted is
 * what ships. The Solana RPC and the Cherry host are stubbed at the network
 * and window boundary respectively — the app code under test is untouched,
 * and the account bytes the stub serves are the same Borsh fixtures the Rust
 * program emits.
 */
export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  reporter: [['list']],
  use: {
    baseURL: 'http://127.0.0.1:3518',
    // Mobile first, 390 × 844 — the Cherry WebView on Seeker. The iPhone 14
    // device profile defaults to WebKit; Chromium is what this environment
    // has, and the WebView engine is not what these assertions are about.
    ...devices['iPhone 14'],
    browserName: 'chromium',
    defaultBrowserType: 'chromium',
    // This environment ships a pinned Chromium; point at it rather than
    // downloading a matching build. Override with PLAYWRIGHT_CHROMIUM_PATH.
    launchOptions: process.env.PLAYWRIGHT_CHROMIUM_PATH
      ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_PATH }
      : {},
    ignoreHTTPSErrors: true,
  },
  webServer: [
    {
      // Stands in for Cherry's JWKS endpoint so the signed-blink path can be
      // exercised end to end.
      command: 'node e2e/jwks-server.mjs',
      url: 'http://127.0.0.1:3519/jwks.json',
      reuseExistingServer: !process.env.CI,
      timeout: 60_000,
    },
    {
      command: 'npx next start -p 3518',
      url: 'http://127.0.0.1:3518',
      reuseExistingServer: !process.env.CI,
      timeout: 120_000,
      env: {
        CHERRY_APP_ID: 'sol-rigs-e2e',
        CHERRY_JWKS_URL: 'http://127.0.0.1:3519/jwks.json',
      },
    },
  ],
});
