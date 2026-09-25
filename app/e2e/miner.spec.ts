/**
 * Phase 10 — the visual miner against a real production build.
 *
 * The standalone (Wallet Standard / Solana Mobile in-app-browser) connection
 * path is driven by an injected wallet, the RPC serves the same Rust-emitted
 * fixtures the rest of the suite uses, and the action paths — claim
 * confirmation, replant preview, error translation, post-transaction refresh —
 * run exactly as they do on a device: tap, sign, wait for confirmation,
 * re-read the chain.
 */
import { expect, test } from '@playwright/test';

import { formatCount, formatSol } from '../src/lib/format';
import { decodeMiner } from '../src/lib/program/accounts';
import { minerBytes, stubCherryHost, stubRpc, stubWalletStandard, WALLET } from './fixtures';

test.describe('visual miner', () => {
  test('the grove visual is the centerpiece, above the yield panel', async ({ page }) => {
    await stubCherryHost(page);
    await stubRpc(page);
    await page.goto('/');
    // Wait for the game itself before asserting layout — under a loaded
    // parallel run, first paint can trail the socket by seconds.
    await expect(page.getByTestId('yield-panel')).toBeVisible({ timeout: 15_000 });

    const figure = page.locator('figure[aria-label^="Grove:"]');
    await expect(figure).toBeVisible({ timeout: 15_000 });
    expect(await figure.getAttribute('aria-label')).toContain('young tree'); // 42 rigs → tier 3

    // DOM order: launch panel, then the grove, then the yield headline.
    const order = await page.evaluate(() =>
      Array.from(document.querySelectorAll<HTMLElement>('section, figure')).map((el, i) => ({
        i,
        tag: el.tagName,
        testid: el.dataset.testid ?? el.getAttribute('aria-label') ?? '',
      })),
    );
    const figureIndex = order.findIndex((e) => e.tag === 'FIGURE');
    const yieldIndex = order.findIndex((e) => e.testid === 'yield-panel');
    expect(figureIndex).toBeGreaterThan(-1);
    expect(yieldIndex).toBeGreaterThan(figureIndex);
  });

  test('the cluster this build talks to is identified on screen', async ({ page }) => {
    await stubCherryHost(page);
    await stubRpc(page);
    await page.goto('/');
    await expect(page.getByTestId('cluster')).toHaveText('DEVNET');
  });

  test('the grove record shows referral, weight, penalty clock and lifetime ledger', async ({
    page,
  }) => {
    await stubCherryHost(page);
    await stubRpc(page);
    await page.goto('/');
    const record = page.getByTestId('grove-record');
    await expect(record).toBeVisible();

    // Every figure is the decoded fixture itself — the screen adds nothing.
    const miner = decodeMiner(minerBytes());
    await expect(record).toContainText(`Reward weight`);
    await expect(record.locator('.readout', { hasText: 'Reward weight' })).toContainText(
      `${(miner.weight / 100).toFixed(2)}×`,
    );
    await expect(record.locator('.readout', { hasText: 'Referral' })).toContainText('Bound to');
    // Planted 30h before the stub clock → the 15% band, shown with the age.
    await expect(record.locator('.readout', { hasText: 'Last planted' })).toContainText(
      /1d ?\d+h|30h|1d/,
    );
    await expect(record.locator('.readout', { hasText: 'Last planted' })).toContainText(
      '15% claim band',
    );
    await expect(
      record.locator('.readout', { hasText: 'Seeds bought · replanted' }),
    ).toContainText(`${formatCount(miner.lifetimeRigsBought)} · ${formatCount(miner.lifetimeRigsCompounded)}`);
    await expect(record.locator('.readout', { hasText: 'SOL planted (lifetime)' })).toContainText(
      formatSol(miner.lifetimeSolSpent, 4),
    );
  });
});

test.describe('standalone wallet connection', () => {
  test('lists the injected wallet and renders the connected grove', async ({ page }) => {
    await stubWalletStandard(page);
    await stubRpc(page);
    await page.goto('/');

    const connect = page.getByRole('button', { name: 'Seeker Test Wallet' });
    await expect(connect).toBeVisible();
    await connect.click();

    // Connected: the wallet's position is on screen and the actions are live.
    await expect(
      page.locator('.readout', { hasText: 'My grove' }).locator('.readout-value'),
    ).toHaveText('42SEEDS');
    await expect(page.getByRole('button', { name: /Claim SOL/i })).toBeEnabled();
    await expect(page.getByRole('button', { name: /^Replant/i })).toBeEnabled();
  });
});

test.describe('harvest confirmation', () => {
  test('claim shows gross, penalty band and net, and requires a confirming tap', async ({
    page,
  }) => {
    await stubWalletStandard(page);
    await stubRpc(page);
    await page.goto('/');
    await page.getByRole('button', { name: 'Seeker Test Wallet' }).click();
    await expect(page.getByText('My grove')).toBeVisible();

    // No sheet before the tap; nothing sends without it.
    await expect(page.getByTestId('claim-confirm')).toHaveCount(0);
    await page.getByRole('button', { name: /Claim SOL/i }).click();

    const sheet = page.getByTestId('claim-confirm');
    await expect(sheet).toBeVisible();
    await expect(sheet.getByText('Gross harvest')).toBeVisible();
    await expect(sheet.getByText(/Early-claim penalty · 15%/)).toBeVisible();
    await expect(sheet.getByText('You receive')).toBeVisible();
    await expect(sheet.getByRole('button', { name: 'Not now' })).toBeVisible();

    // Cancelling leaves everything untouched.
    await sheet.getByRole('button', { name: 'Not now' }).click();
    await expect(page.getByTestId('claim-confirm')).toHaveCount(0);
    await expect(page.getByRole('button', { name: /Claim SOL/i })).toBeEnabled();
  });

  test('confirming waits for chain confirmation and refreshes from chain state', async ({
    page,
  }) => {
    await stubWalletStandard(page);
    await stubRpc(page);
    await page.goto('/');
    await page.getByRole('button', { name: 'Seeker Test Wallet' }).click();
    await expect(page.getByText('My grove')).toBeVisible();

    let statusPolls = 0;
    page.on('request', (r) => {
      if (/getSignatureStatuses/.test(r.postData() ?? '')) statusPolls += 1;
    });

    await page.getByRole('button', { name: /Claim SOL/i }).click();
    await page.getByTestId('claim-confirm').getByRole('button', { name: /Confirm harvest/i }).click();

    // The sheet closes, no error banner, and the action settled only after
    // the signature was confirmed on chain.
    await expect(page.getByTestId('claim-confirm')).toHaveCount(0);
    await expect(page.locator('.banner--alert')).toHaveCount(0);
    await expect(page.getByRole('button', { name: /Claim SOL/i })).toBeEnabled();
    expect(statusPolls).toBeGreaterThan(0);
  });

  test('an on-chain refusal is translated; the raw error stays in the console', async ({ page }) => {
    await stubWalletStandard(page, {
      failWith:
        'AnchorError thrown in programs/sol-rigs/src/error.rs:97. Error Code: AccessDenied. ' +
        'Error Number: 6023. Error Message: Buying is not open to this wallet in the current launch phase.',
    });
    await stubRpc(page);
    await page.goto('/');
    await page.getByRole('button', { name: 'Seeker Test Wallet' }).click();
    await expect(page.getByText('My grove')).toBeVisible();

    const consoleErrors: string[] = [];
    page.on('console', (m) => {
      if (m.type() === 'error') consoleErrors.push(m.text());
    });

    await page.getByRole('button', { name: /Claim SOL/i }).click();
    await page.getByTestId('claim-confirm').getByRole('button', { name: /Confirm harvest/i }).click();

    // Product language on screen, raw Anchor error in the developer console.
    await expect(page.locator('.banner--alert')).toContainText(
      'This wallet is not eligible during the current access phase.',
    );
    await expect
      .poll(() => consoleErrors.join('\n'), { timeout: 5_000 })
      .toContain('AccessDenied');
  });
});

test.describe('replant preview', () => {
  test('shows the seeds, their cost from accrued harvest and the remainder', async ({ page }) => {
    await stubCherryHost(page);
    await stubRpc(page);
    await page.goto('/');
    const preview = page.getByTestId('compound-preview');
    await expect(preview).toBeVisible();
    await expect(preview).toContainText(/Replant plants \d+ more seeds? for 0\./);
    await expect(preview).toContainText(/stays ready to harvest/);
  });
});

test.describe('responsive', () => {
  test('desktop viewport: no overflow, miner centered, no console errors', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    const errors: string[] = [];
    page.on('console', (m) => {
      // `Failed to load resource` is the network layer (an unauthenticated
      // API route failing closed under the stubbed host), not a JS error.
      if (m.type() === 'error' && !/Failed to load resource/.test(m.text()))
        errors.push(m.text());
    });
    page.on('pageerror', (e) => errors.push(String(e)));

    await stubCherryHost(page);
    await stubRpc(page);
    await page.goto('/');
    await expect(page.getByText('My grove')).toBeVisible();

    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - window.innerWidth,
    );
    expect(overflow).toBe(0);
    expect(errors).toEqual([]);
  });

  test('phone viewport: every control is a touch-safe target', async ({ page }) => {
    await stubCherryHost(page);
    await stubRpc(page);
    await page.goto('/');
    await expect(page.getByText('My grove')).toBeVisible();

    const tooSmall = await page.evaluate(() => {
      const sub: string[] = [];
      for (const b of Array.from(document.querySelectorAll<HTMLButtonElement>('button'))) {
        if (b.getClientRects().length === 0) continue; // hidden
        const r = b.getBoundingClientRect();
        if (r.height > 0 && r.height < 43)
          sub.push(`${b.textContent ?? b.getAttribute('aria-label')}: ${r.height}px`);
      }
      return sub;
    });
    expect(tooSmall).toEqual([]);

    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - window.innerWidth,
    );
    expect(overflow).toBe(0);
  });
});
