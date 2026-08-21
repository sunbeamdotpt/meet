import { test, expect } from '@playwright/test';
import { signInWithTestAccount, joinRoom } from './helpers';

test('host can raise and lower their hand', async ({ page }) => {
  const roomName = `raisehand-${Date.now()}`;
  await signInWithTestAccount(page, { name: 'Hand Host', email: 'hand-host@example.com', roomName, role: 'host' });
  await joinRoom(page);
  await page.locator('[data-testid="meeting-controls"]').waitFor({ state: 'visible', timeout: 20000 });

  await page.locator('[data-testid="raise-hand-button"]').click();
  await expect(page.locator('[data-testid="raised-hands-panel"]')).toBeVisible({ timeout: 5000 });
  await expect(page.locator('[data-testid="raised-hands-panel"]')).toContainText('Hand Host');

  await page.locator('[data-testid="raise-hand-button"]').click();
  await expect(page.locator('[data-testid="raised-hands-panel"]')).not.toBeVisible({ timeout: 5000 });
});
