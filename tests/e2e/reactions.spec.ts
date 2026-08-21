import { test, expect } from '@playwright/test';
import { signInWithTestAccount, joinRoom, captureFeatureScreenshot } from './helpers';

test('host can send a reaction that appears in the overlay', async ({ page }) => {
  const roomName = `reactions-${Date.now()}`;
  await signInWithTestAccount(page, {
    name: 'Reaction Host',
    email: 'reaction-host@example.com',
    roomName,
    role: 'host',
  });
  await joinRoom(page);
  await page
    .locator('[data-testid="meeting-controls"]')
    .waitFor({ state: 'visible', timeout: 20000 });

  await page.locator('[data-testid="reaction-👍"]').click();
  await expect(page.locator('[data-testid="reaction-overlay"] span')).toContainText('👍', {
    timeout: 5000,
  });
  await captureFeatureScreenshot(page, 'reactions-thumbs-up');
});
