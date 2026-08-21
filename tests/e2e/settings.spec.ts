import { test, expect } from '@playwright/test';
import { signInWithTestAccount, joinRoom, captureFeatureScreenshot } from './helpers';

test('host can open settings and sees recording tab', async ({ page }) => {
  const roomName = `settings-${Date.now()}`;
  await signInWithTestAccount(page, {
    name: 'Settings Host',
    email: 'settings-host@example.com',
    roomName,
    role: 'host',
  });
  await joinRoom(page);
  await page
    .locator('[data-testid="meeting-controls"]')
    .waitFor({ state: 'visible', timeout: 20000 });

  // Open the settings panel rendered by the LiveKit control bar.
  const settingsButton = page.getByRole('button', { name: 'Settings' });
  await expect(settingsButton).toBeVisible({ timeout: 10000 });
  await settingsButton.click();

  // The settings menu tabs should be visible.
  await expect(page.locator('text=Media Devices')).toBeVisible({ timeout: 10000 });
  await expect(page.locator('text=Recording')).toBeVisible({ timeout: 10000 });

  // Switch to the recording tab.
  await page.locator('button:has-text("Recording")').click();
  await expect(page.locator('text=Record Meeting')).toBeVisible();
  await expect(page.locator('text=No active recordings for this meeting')).toBeVisible();

  // E2EE is enabled by default, so recording should be rejected.
  await page.locator('button:has-text("Start Recording")').click();
  await expect(
    page.locator('text=Recording of encrypted meetings is currently not supported'),
  ).toBeVisible({
    timeout: 5000,
  });
  await captureFeatureScreenshot(page, 'settings-recording-blocked');
});
