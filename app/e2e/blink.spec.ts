/**
 * Spec §12.27: a shared blink renders from the snapshot inside the *signed*
 * token, and tampering with query params outside that token changes nothing.
 *
 * Runs against a real RS256 keypair served from a local JWKS stub, so the
 * server does genuine signature verification.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import * as jose from 'jose';
import { expect, test } from '@playwright/test';

const APP_ID = 'sol-rigs-e2e';
const WALLET = '7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU';

function privateJwk(): jose.JWK {
  const raw = readFileSync(join(import.meta.dirname, '.tmp', 'keys.json'), 'utf8');
  return (JSON.parse(raw) as { privateJwk: jose.JWK }).privateJwk;
}

async function mintInlineToken(
  params: Record<string, unknown>,
  over: { appId?: string; messageId?: string; expSeconds?: number } = {},
): Promise<string> {
  const key = await jose.importJWK(privateJwk(), 'RS256');
  return new jose.SignJWT({
    sub: WALLET,
    app_id: over.appId ?? APP_ID,
    message_id: over.messageId ?? 'msg-abc',
    route: '/result',
    params,
  })
    .setProtectedHeader({ alg: 'RS256', kid: 'e2e-1' })
    .setIssuedAt()
    .setIssuer('https://chat.cherry.fun')
    .setExpirationTime(`${over.expSeconds ?? 300}s`)
    .sign(key);
}

test.describe('inline blink card', () => {
  test('renders the snapshot carried inside the signed token', async ({ page }) => {
    const token = await mintInlineToken({
      kind: 'compound',
      rigs: '54',
      compounded: '12',
    });
    await page.goto(`/inline?token=${token}`);

    await expect(page.getByText('+12 SEEDS')).toBeVisible();
    await expect(page.getByText('54')).toBeVisible();
  });

  /** The whole point: unsigned query fields are not data. */
  test('ignores query params that are not inside the signed token', async ({ page }) => {
    const token = await mintInlineToken({
      kind: 'compound',
      rigs: '54',
      compounded: '12',
    });

    const honest = await (async () => {
      await page.goto(`/inline?token=${token}`);
      return page.locator('body').innerText();
    })();

    // Every tamper attempt: overriding, appending, and re-ordering.
    for (const tamper of [
      'rigs=999999',
      'compounded=999999',
      'claimed=1000000000000',
      'kind=claim&rigs=1&compounded=1',
    ]) {
      await page.goto(`/inline?token=${token}&${tamper}`);
      expect(await page.locator('body').innerText(), tamper).toBe(honest);
    }

    // And the same params in front of the token change nothing either.
    await page.goto(`/inline?rigs=7&token=${token}`);
    expect(await page.locator('body').innerText()).toBe(honest);
  });

  test('rejects a token minted for another app', async ({ page }) => {
    const token = await mintInlineToken({ rigs: '54' }, { appId: 'someone-else' });
    await page.goto(`/inline?token=${token}`);
    await expect(page.getByText(/expired/i)).toBeVisible();
    await expect(page.getByText('54')).toHaveCount(0);
  });

  test('rejects an expired token', async ({ page }) => {
    const key = await jose.importJWK(privateJwk(), 'RS256');
    const now = Math.floor(Date.now() / 1000);
    const stale = await new jose.SignJWT({
      sub: WALLET,
      app_id: APP_ID,
      params: { rigs: '54' },
    })
      .setProtectedHeader({ alg: 'RS256', kid: 'e2e-1' })
      .setIssuedAt(now - 3_600)
      .setExpirationTime(now - 600)
      .sign(key);

    await page.goto(`/inline?token=${stale}`);
    await expect(page.getByText(/expired/i)).toBeVisible();
  });

  test('rejects a card with no token at all', async ({ page }) => {
    await page.goto('/inline');
    await expect(page.getByText(/no launch token/i)).toBeVisible();
  });
});
