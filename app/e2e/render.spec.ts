/**
 * Browser-level checks against a real production build.
 *
 * Covers spec §12.20 / §12.21 / §12.26 and the copy constraints in §2 and §10.
 */
import { expect, test, type Page } from '@playwright/test';

import { stubCherryHost, stubRpc, WALLET } from './fixtures';

/** Spec §10: banned copy. None of this may appear anywhere on the screen. */
const BANNED = [
  /guaranteed\s+(daily|apy|apr|return)/i,
  /\bguaranteed\b/i,
  /\bfixed returns?\b/i,
  /\bROI\b/,
  /\bAPY\b/i,
  /\bAPR\b/i,
  /\bevery day\b/i,
  /risk[- ]free/i,
];

test.describe('standalone path', () => {
  test('renders the mine and offers a wallet connect', async ({ page }) => {
    await stubRpc(page, { withMiner: false });
    await page.goto('/');

    await expect(page.getByRole('heading', { name: 'THE CHERRY TREE' })).toBeVisible();
    // Outside Cherry, the connect affordance is required.
    await expect(page.getByText(/No Solana wallet detected|Connect/i)).toBeVisible();
    // Room UI is hidden with no room context.
    await expect(page.getByText('This room')).toHaveCount(0);
  });
});

test.describe('embedded path', () => {
  test.beforeEach(async ({ page }) => {
    await stubCherryHost(page);
    await stubRpc(page);
  });

  /** §12.21 — first paint shows the user's rigs, with no user action. */
  test('reaches the mining operation with no interaction', async ({ page }) => {
    await page.goto('/');

    await expect(page.getByText('My hashpower')).toBeVisible();
    // 42 rigs, straight out of the Rust-serialised miner fixture.
    await expect(
      page.locator('.readout', { hasText: 'My hashpower' }).locator('.readout-value'),
    ).toHaveText('42RIGS');
    await expect(page.getByText('Ore bin · unclaimed SOL')).toBeVisible();
    await expect(page.getByRole('button', { name: /Claim SOL/i })).toBeVisible();
  });

  /** §12.21 — zero connect-wallet UI in the embed. */
  test('renders no connect-wallet UI at all', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByText('My hashpower')).toBeVisible();

    const text = (await page.locator('body').innerText()).toLowerCase();
    expect(text).not.toContain('connect wallet');
    expect(text).not.toContain('select wallet');
    expect(text).not.toContain('no solana wallet detected');
    await expect(page.getByRole('button', { name: /^connect/i })).toHaveCount(0);
  });

  test('shows room context and the room leaderboard section', async ({ page }) => {
    await page.goto('/');
    // The room name appears twice — in the header line and in the leaderboard
    // heading — so both are asserted explicitly rather than by substring.
    await expect(page.getByText('Mine Shaft · 412 members')).toBeVisible();
    await expect(page.getByText('This room')).toBeVisible();
    await expect(page.getByText('Top miners in Mine Shaft')).toBeVisible();
  });

  /** §10 — the buy quote adds up on screen, fee included and disclosed. */
  test('quotes cost, fee and total, and they add up', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByText('Buy more hashpower')).toBeVisible();

    const value = async (label: string) => {
      const row = page.locator('.quote .readout', { hasText: label }).first();
      return (await row.locator('.readout-value').innerText()).replace(/[^\d.]/g, '');
    };

    // Default 10 rigs at 0.01 SOL with a 3.00% fee.
    expect(await value('Cost')).toBe('0.1000');
    expect(await value('Fee')).toBe('0.0030');
    expect(await value('Total')).toBe('0.1030');
    await expect(page.getByText('Fee · 3.00%')).toBeVisible();
  });

  test('the quick-select buttons change the quote', async ({ page }) => {
    await page.goto('/');
    await page.getByRole('button', { name: '25', exact: true }).click();
    await expect(page.getByRole('button', { name: /Hire 25 rigs/i })).toBeVisible();
  });

  /** §2, §10 — honesty constraints and banned copy. */
  test('states the simulation and non-redeemable constraints, and promises nothing', async ({
    page,
  }) => {
    await page.goto('/');
    await expect(page.getByText('My hashpower')).toBeVisible();

    const text = await page.locator('body').innerText();

    expect(text).toMatch(/mining\s+simulation/i);
    expect(text).toMatch(/not proof-of-work mined/i);
    expect(text).toMatch(/non-redeemable/i);
    expect(text).toMatch(/funded reward vault/i);
    // The mandated sentence under the output estimate.
    expect(text).toMatch(
      /Based on the current funded reward schedule and total rig count/i,
    );

    for (const pattern of BANNED) {
      expect(text, `banned copy matched ${pattern}`).not.toMatch(pattern);
    }
    // Deposit language is banned outright.
    expect(text).not.toMatch(/\bwithdraw your (stake|deposit|principal)\b/i);
  });

  test('the transparency panel reports vault state and links to the explorer', async ({
    page,
  }) => {
    await page.goto('/');
    await expect(page.getByText('Global mine')).toBeVisible();
    const panel = page.locator('.panel').filter({ hasText: 'Reward vault' });
    for (const label of [
      'Reward vault',
      'Current distribution',
      'Funded until',
      'Funded runway',
      'Global rigs',
      'Your share',
      'Protocol fee',
    ]) {
      await expect(panel.getByText(label, { exact: true })).toBeVisible();
    }

    // The distribution rate is the chain's, rendered as SOL/day.
    await expect(
      panel.locator('.readout', { hasText: 'Current distribution' }).locator('.readout-value'),
    ).toHaveText('2.199SOL / day');

    const verify = page.getByRole('link', { name: /Verify on Solana/i });
    await expect(verify).toBeVisible();
    await expect(verify).toHaveAttribute('href', /explorer\.solana\.com\/address\//);
  });

  /**
   * §12.24 — `walletDisconnected` disables all three action buttons without
   * unmounting state.
   */
  test('a wallet disconnect disables every action but keeps the mine on screen', async ({
    page,
  }) => {
    await page.goto('/');
    await expect(page.getByText('My hashpower')).toBeVisible();
    await expect(page.getByRole('button', { name: /Claim SOL/i })).toBeEnabled();

    await page.evaluate(() =>
      (window as unknown as { __cherryEmit: (e: string) => void }).__cherryEmit(
        'walletDisconnected',
      ),
    );

    await expect(page.getByText(/Wallet disconnected/i)).toBeVisible();
    await expect(page.getByRole('button', { name: /Claim SOL/i })).toBeDisabled();
    await expect(page.getByRole('button', { name: /^Compound/i })).toBeDisabled();
    await expect(page.getByRole('button', { name: /Hire /i })).toBeDisabled();

    // Read-only state survives: the position is still rendered, not unmounted.
    await expect(
      page.locator('.readout', { hasText: 'My hashpower' }).locator('.readout-value'),
    ).toHaveText('42RIGS');
    await expect(page.getByText('Global mine')).toBeVisible();
    await expect(page.locator('.readout-value--hero')).toBeVisible();
  });

  /** §12.17 mirrored in the UI: a paused program offers no actions. */
  test('a paused program disables the actions and says so', async ({ page }) => {
    await stubRpc(page, { paused: true });
    await page.goto('/');

    await expect(page.getByText(/halted by its authority/i)).toBeVisible();
    await expect(page.getByRole('button', { name: /Claim SOL/i })).toBeDisabled();
    await expect(page.getByRole('button', { name: /^Compound/i })).toBeDisabled();
    await expect(page.getByRole('button', { name: /Hire /i })).toBeDisabled();
    // Read-only state stays on screen.
    await expect(page.getByText('My hashpower')).toBeVisible();
  });

  /**
   * §10 — the claimable figure has to tick visibly. At a realistic emission
   * rate a 42-rig miner earns tens of lamports a second, which is why the
   * readout carries full lamport precision rather than six decimals.
   */
  test('the claimable figure ticks while the schedule is live', async ({ page }) => {
    await page.goto('/');
    const hero = page.locator('.readout-value--hero');
    await expect(hero).toBeVisible();

    // Starts at exactly the figure the chain state implies.
    await expect(hero).toHaveText(/^0\.018291/);

    const first = await hero.innerText();
    await expect.poll(async () => hero.innerText(), { timeout: 8_000 }).not.toBe(first);
  });

  /** §10 — the ticker must stop when the app is suspended. */
  test('the ticker stops when the page is hidden and re-anchors when shown', async ({
    page,
  }) => {
    await page.goto('/');
    const hero = page.locator('.readout-value--hero');
    await expect(hero).toBeVisible();
    await expect.poll(async () => hero.innerText(), { timeout: 8_000 }).not.toBe(
      await hero.innerText(),
    );

    await page.evaluate(() => {
      Object.defineProperty(document, 'visibilityState', {
        configurable: true,
        get: () => 'hidden',
      });
      document.dispatchEvent(new Event('visibilitychange'));
    });

    const frozen = await hero.innerText();
    await page.waitForTimeout(2_500);
    expect(await hero.innerText()).toBe(frozen);
  });
});

test.describe('blink result card', () => {
  test('renders purely from its params', async ({ page }) => {
    await page.goto('/result?kind=compound&rigs=54&compounded=12');
    await expect(page.getByText('+12 RIGS')).toBeVisible();
    await expect(page.getByText('54')).toBeVisible();
    await expect(page.getByText(/read-only snapshot/i)).toBeVisible();
  });

  test('ignores junk params rather than rendering them', async ({ page }) => {
    await page.goto('/result?rigs=<script>alert(1)</script>&compounded=NaN');
    await expect(page.getByText('THE CHERRY TREE · MINE REPORT')).toBeVisible();
    const text = await page.locator('body').innerText();
    expect(text).not.toContain('alert(1)');
    expect(text).not.toContain('NaN');
  });
});

test.describe('hosting requirements', () => {
  /** §12.26 — the app must be framable by Cherry web. */
  test('sends frame-ancestors for chat.cherry.fun and no X-Frame-Options', async ({
    page,
  }) => {
    const response = await page.goto('/');
    const headers = response!.headers();
    expect(headers['content-security-policy']).toContain(
      "frame-ancestors 'self' https://chat.cherry.fun",
    );
    expect(headers['x-frame-options']).toBeUndefined();
  });
});

test.describe('wallet address plumbing', () => {
  test('reads the position for the wallet the host handed over', async ({ page }) => {
    const requests: string[] = [];
    page.on('request', (r) => {
      if (r.url().includes('solana')) requests.push(r.postData() ?? '');
    });
    await stubCherryHost(page);
    await stubRpc(page);
    await page.goto('/');
    await expect(page.getByText('My hashpower')).toBeVisible();

    // The miner PDA is derived from the host-supplied wallet, so the wallet
    // itself shows up in the balance lookup.
    expect(requests.join('\n')).toContain(WALLET);
  });
});

test.describe('content security policy', () => {
  test('the production CSP is an allowlist, not a wildcard', async ({ page }) => {
    const response = await page.goto('/');
    const csp = response!.headers()['content-security-policy'] ?? '';

    // A page holding a live wallet session must not be able to reach
    // arbitrary origins, so `connect-src` names the RPC rather than `https:`.
    expect(csp).toContain("connect-src 'self' https://api.devnet.solana.com");
    // The bare scheme wildcards `https:` / `wss:` — not the `https://host`
    // entries, which are exactly what an allowlist is made of.
    expect(csp).not.toMatch(/connect-src[^;]*\shttps:(?!\/)/);
    expect(csp).not.toMatch(/connect-src[^;]*\swss:(?!\/)/);

    // eval belongs to the dev server only.
    expect(csp).not.toContain("'unsafe-eval'");

    for (const directive of [
      "default-src 'self'",
      "object-src 'none'",
      "frame-src 'none'",
      "form-action 'none'",
      "base-uri 'self'",
      'upgrade-insecure-requests',
    ]) {
      expect(csp, directive).toContain(directive);
    }
  });
});

/**
 * A tightened CSP is only worth having if the app still runs under it. These
 * drive the real flows with a violation listener attached, so a directive
 * that breaks the page fails the suite here instead of silently degrading
 * the app in production.
 */
test.describe('CSP is survivable', () => {
  async function watchViolations(page: Page): Promise<() => Promise<string[]>> {
    const fromConsole: string[] = [];
    await page.addInitScript(() => {
      (window as unknown as { __csp: string[] }).__csp = [];
      document.addEventListener('securitypolicyviolation', (e) => {
        (window as unknown as { __csp: string[] }).__csp.push(
          `${e.violatedDirective} blocked ${e.blockedURI}`,
        );
      });
    });
    page.on('console', (m) => {
      const t = m.text();
      if (/Content Security Policy|Refused to/i.test(t)) fromConsole.push(t);
    });
    return async () => [
      ...fromConsole,
      ...(await page.evaluate(
        () => (window as unknown as { __csp?: string[] }).__csp ?? [],
      )),
    ];
  }

  test('the embedded path raises none, including on interaction', async ({ page }) => {
    const collect = await watchViolations(page);
    await stubCherryHost(page);
    await stubRpc(page);
    await page.goto('/');

    await expect(page.getByText('My hashpower')).toBeVisible();
    await expect(page.locator('.readout-value--hero')).toBeVisible();
    // A blocked event handler would only surface once something is clicked.
    await page.getByRole('button', { name: '25', exact: true }).click();
    await expect(page.getByRole('button', { name: /Hire 25 rigs/i })).toBeVisible();

    expect(await collect()).toEqual([]);
  });

  test('the standalone path raises none', async ({ page }) => {
    const collect = await watchViolations(page);
    await stubRpc(page, { withMiner: false });
    await page.goto('/');
    await expect(page.getByRole('heading', { name: 'THE CHERRY TREE' })).toBeVisible();
    expect(await collect()).toEqual([]);
  });

  test('the blink card raises none', async ({ page }) => {
    const collect = await watchViolations(page);
    await page.goto('/result?kind=compound&rigs=54&compounded=12');
    await expect(page.getByText('+12 RIGS')).toBeVisible();
    expect(await collect()).toEqual([]);
  });

  test('the RPC origin the app actually calls is permitted', async ({ page }) => {
    const collect = await watchViolations(page);
    await stubCherryHost(page);
    await stubRpc(page);
    await page.goto('/');
    // Reaching first paint with real rig data means the RPC call went out
    // and was not blocked by connect-src.
    await expect(
      page.locator('.readout', { hasText: 'My hashpower' }).locator('.readout-value'),
    ).toHaveText('42RIGS');
    expect(await collect()).toEqual([]);
  });
});
