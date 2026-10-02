import { test } from '@playwright/test';

import { stubCherryHost, stubRpc } from './fixtures';

/**
 * Captures the embedded screen at Cherry-WebView size. Not an assertion —
 * it exists so the machine-room look can be reviewed without a device.
 */
test('capture the embedded screen', async ({ page }) => {
  await stubCherryHost(page);
  await stubRpc(page);
  await page.goto('/');
  await page.getByText('My grove').waitFor();
  await page.screenshot({ path: 'e2e/__screenshots__/embedded.png', fullPage: true });
});
