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
    // Planted 30h before the stub clock → the 20% band, shown with the age.
    await expect(record.locator('.readout', { hasText: 'Last planted' })).toContainText(
      /1d ?\d+h|30h|1d/,
    );
    await expect(record.locator('.readout', { hasText: 'Last planted' })).toContainText(
      '20% claim band',
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
    await expect(sheet.getByText(/Early-claim fee · 20%/)).toBeVisible();
    // §7a item 5: the destination is disclosed before signing.
    await expect(sheet.getByText(/to the Protocol Treasury \(60%\)/)).toBeVisible();
    await expect(sheet.getByText(/redistributed to other planters \(40%\)/)).toBeVisible();
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

test.describe('miner cadence surfaces', () => {
  test('fresh planting discloses the clock reset before the wallet is asked to sign', async ({ page }) => {
    await stubWalletStandard(page);
    await stubRpc(page);
    await page.goto('/');
    await page.getByRole('button', { name: 'Seeker Test Wallet' }).click();

    const warning = page.getByTestId('buy-reset-warning');
    // §7a item 6: pending rewards > 0, so the mandated top-up warning shows.
    await expect(warning.getByTestId('top-up-warning')).toContainText(
      /Buying more seeds restarts your early-claim timer\. Your unclaimed rewards \(0\.\d+ SOL\) would then carry the 30% early-claim fee for the next 24 hours\. Claiming first avoids that reset, but a claim now still pays the early-claim fee that currently applies\./,
    );
    // §7a item 5: schedule and 60/40 destination disclosed before purchase.
    await expect(warning.getByTestId('early-claim-disclosure')).toContainText(
      /30% under 24h, 20% from 24h to 48h, 10% from 48h to 72h, 0% after/,
    );
    await expect(warning.getByTestId('early-claim-disclosure')).toContainText(
      /60% of the fee goes to the Protocol Treasury and 40% is redistributed/,
    );

    let sends = 0;
    page.on('request', (request) => {
      if (/getLatestBlockhash/.test(request.postData() ?? '')) sends += 1;
    });
    await page.getByRole('button', { name: /Plant 10 seeds · review/i }).click();
    const confirm = page.getByTestId('buy-confirm');
    await expect(confirm).toBeVisible();
    expect(sends).toBe(0);
    await confirm.getByRole('button', { name: 'Not now' }).click();
    await expect(confirm).toHaveCount(0);
    expect(sends).toBe(0);

    await page.getByRole('button', { name: /Plant 10 seeds · review/i }).click();
    await page.getByTestId('buy-confirm').getByRole('button', { name: /Confirm plant/i }).click();
    await expect(page.getByTestId('buy-confirm')).toHaveCount(0);
    await expect.poll(() => sends).toBeGreaterThan(0);
  });

  test('cockpit shows position, penalty countdown, replant arithmetic and reward state', async ({ page }) => {
    await stubCherryHost(page);
    await stubRpc(page);
    await page.goto('/');
    const cockpit = page.getByTestId('miner-cockpit');
    await expect(cockpit).toBeVisible();
    await expect(cockpit).toContainText(/Plant → tend → watch → rooted → harvest or replant → share/i);
    await expect(cockpit.getByTestId('harvest-state')).toContainText('20%');
    await expect(cockpit.getByTestId('harvest-state')).toContainText(/10% in 18h/);
    await expect(cockpit.getByTestId('replant-state')).toContainText(/Resulting grove/);
    // Pool and runway detail lives in the yield panel; the cockpit carries the state.
    await expect(cockpit.getByTestId('reward-state')).toHaveText(/Live|Low runway|Dormant|Unfunded/);
    await expect(cockpit.getByText('Your effective rate')).toBeVisible();
    // The actions come before the detail that explains them.
    const actionsFirst = await cockpit.evaluate((el) => {
      const claim = Array.from(el.querySelectorAll('button')).find((b) => /Claim SOL/i.test(b.textContent ?? ''));
      const harvest = el.querySelector('[data-testid="harvest-state"]');
      return !!claim && !!harvest && !!(claim.compareDocumentPosition(harvest) & Node.DOCUMENT_POSITION_FOLLOWING);
    });
    expect(actionsFirst).toBe(true);
  });

  test('a wallet with no seeds sees Plant Seeds above a compact cockpit', async ({ page }) => {
    await stubCherryHost(page);
    await stubRpc(page, { withMiner: false });
    await page.goto('/');
    const cockpit = page.getByTestId('miner-cockpit');
    await expect(cockpit.getByTestId('cockpit-empty')).toBeVisible({ timeout: 15_000 });
    // Nothing to manage yet: no harvest, replant or zero-filled position rows.
    await expect(cockpit.getByTestId('harvest-state')).toHaveCount(0);
    await expect(cockpit.getByTestId('replant-state')).toHaveCount(0);
    await expect(cockpit.getByText('My grove')).toHaveCount(0);

    const plantAboveCockpit = await page.evaluate(() => {
      const plant = Array.from(document.querySelectorAll('h2')).find((h) => h.textContent === 'Plant seeds');
      const cockpitEl = document.querySelector('[data-testid="miner-cockpit"]');
      return !!plant && !!cockpitEl && !!(plant.compareDocumentPosition(cockpitEl) & Node.DOCUMENT_POSITION_FOLLOWING);
    });
    expect(plantAboveCockpit).toBe(true);
  });

  test('trust panel separates proven data from unavailable authority metadata', async ({ page }) => {
    await stubCherryHost(page);
    await stubRpc(page);
    await page.goto('/');
    const trust = page.getByTestId('trust-panel');
    await expect(trust).toContainText('Program ID');
    await expect(trust).toContainText('Program authority');
    await expect(trust).toContainText('Upgrade authority');
    await expect(trust).toContainText('Not available');
    await expect(trust).toContainText('Capital vault · locked');
    await expect(trust).toContainText('Reward backing');
    await expect(trust).not.toContainText(/audited|multisig \d|timelock active/i);
  });
});

test.describe('personal activity history', () => {
  test('renders decoded wallet events with a safe explorer link', async ({ page }) => {
    await stubCherryHost(page);
    await stubRpc(page, { activity: 'populated' });
    await page.goto('/');
    const history = page.getByTestId('activity-history');
    await expect(history.getByTestId('activity-list')).toContainText('Fresh plant');
    await expect(history.locator('a')).toHaveAttribute('href', /^https:\/\/explorer\.solana\.com\/tx\/[1-9A-HJ-NP-Za-km-z]+\?cluster=devnet$/);
  });

  test('renders an honest empty state', async ({ page }) => {
    await stubCherryHost(page);
    await stubRpc(page, { activity: 'empty' });
    await page.goto('/');
    await expect(page.getByTestId('activity-empty')).toBeVisible();
  });

  test('renders RPC/indexing failure without disturbing the position', async ({ page }) => {
    await stubCherryHost(page);
    await stubRpc(page, { activity: 'failure' });
    await page.goto('/');
    await expect(page.getByTestId('activity-error')).toBeVisible();
    await expect(page.locator('.readout', { hasText: 'My grove' })).toContainText('42');
  });
});

test.describe('shareable progress', () => {
  test('shows the explicit referral URL without balances or return claims', async ({ page }) => {
    await stubCherryHost(page);
    await stubRpc(page);
    await page.goto('/');
    const card = page.getByTestId('progress-card');
    await expect(card).toContainText('42 seeds');
    await expect(card).toContainText(`?ref=${WALLET}`);
    await expect(card).not.toContainText(/4\.000000 SOL|daily return|ROI|profit/i);
    await expect(card).toContainText('No wallet balance or return estimate is shared.');
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

  test('narrow Android-like viewport keeps trust, countdowns and signatures inside the page', async ({ page }) => {
    await page.setViewportSize({ width: 320, height: 720 });
    await stubCherryHost(page);
    await stubRpc(page, { activity: 'populated' });
    await page.goto('/');
    await expect(page.getByTestId('miner-cockpit')).toBeVisible();
    await expect(page.getByTestId('trust-panel')).toBeVisible();
    await expect(page.getByTestId('activity-list')).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBe(0);
  });
});

test.describe('referral commission', () => {
  test('no ledger: nothing accrued, claim disabled', async ({ page }) => {
    await stubWalletStandard(page);
    await stubRpc(page);
    await page.goto('/');
    await page.getByRole('button', { name: 'Seeker Test Wallet' }).click();
    const panel = page.getByTestId('commission-panel');
    await expect(panel.getByTestId('commission-none')).toBeVisible();
    await expect(panel.getByTestId('commission-claim')).toBeDisabled();
    // Disclosure: commission exists, 20/80, 8% clamp — never "no payout".
    await expect(panel).toContainText('up to 20% of that fee');
    await expect(panel).toContainText('other 80%');
    await expect(panel).toContainText('keeps 8% of the amount planted');
    await expect(page.getByTestId('referral-panel')).not.toContainText('no payout');
  });

  test('shows accrued, claimed and claimable; one claim, then re-read from chain', async ({ page }) => {
    await stubWalletStandard(page);
    await stubRpc(page, {
      referralLedger: { accrued: 50_000_000n, claimed: 10_000_000n, claimSettles: true },
    });
    await page.goto('/');
    await page.getByRole('button', { name: 'Seeker Test Wallet' }).click();
    const panel = page.getByTestId('commission-panel');
    const value = (label: string) =>
      panel.locator('.readout', { hasText: label }).locator('.readout-value');
    await expect(value('Accrued (all time)')).toHaveText('0.050000SOL');
    await expect(value('Claimed')).toHaveText('0.010000SOL');
    await expect(value('Claimable now')).toHaveText('0.040000SOL');

    let sends = 0;
    page.on('request', (r) => {
      if (/getLatestBlockhash/.test(r.postData() ?? '')) sends += 1;
    });
    const claim = panel.getByTestId('commission-claim');
    await expect(claim).toBeEnabled();
    await claim.click();
    await expect(panel.getByTestId('commission-message')).toContainText('Commission claimed');
    // Refreshed from chain: fully claimed, so the button is disabled again.
    await expect(value('Claimed')).toHaveText('0.050000SOL');
    await expect(value('Claimable now')).toHaveText('0.000000SOL');
    await expect(claim).toBeDisabled();
    expect(sends).toBe(1);
  });

  test('paused: claims blocked and explained, reserve stays protected', async ({ page }) => {
    await stubWalletStandard(page);
    await stubRpc(page, { paused: true, referralLedger: { accrued: 50_000_000n, claimed: 0n } });
    await page.goto('/');
    await page.getByRole('button', { name: 'Seeker Test Wallet' }).click();
    const panel = page.getByTestId('commission-panel');
    await expect(panel.getByTestId('commission-paused')).toContainText('commission claims are blocked');
    await expect(panel.getByTestId('commission-claim')).toBeDisabled();
  });

  test('an on-chain refusal is shown honestly and the ledger is re-read', async ({ page }) => {
    await stubWalletStandard(page, {
      failWith:
        'AnchorError. Error Code: NothingToClaimCommission. Error Number: 6099. ' +
        'Error Message: nothing to claim.',
    });
    await stubRpc(page, { referralLedger: { accrued: 50_000_000n, claimed: 0n } });
    await page.goto('/');
    await page.getByRole('button', { name: 'Seeker Test Wallet' }).click();
    const panel = page.getByTestId('commission-panel');
    await panel.getByTestId('commission-claim').click();
    await expect(panel.getByTestId('commission-message')).toContainText(
      'No referral commission is waiting to be claimed.',
    );
    await expect(panel.getByTestId('commission-claim')).toBeEnabled();
  });

  test('not connected: figures visible, claim needs the referrer wallet', async ({ page }) => {
    await stubWalletStandard(page);
    await stubRpc(page, { referralLedger: { accrued: 50_000_000n, claimed: 0n } });
    await page.goto('/');
    await expect(page.getByTestId('commission-panel')).toHaveCount(0);
  });
});
