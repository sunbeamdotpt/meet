import { test, expect } from '@playwright/test';
import { signInWithTestAccount, joinRoom, captureFeatureScreenshot } from './helpers';

test('host can assign a guest to a breakout room', async ({ browser }) => {
  const roomName = `breakouts-${Date.now()}`;

  const hostContext = await browser.newContext();
  const guestContext = await browser.newContext();

  const hostPage = await hostContext.newPage();
  const guestPage = await guestContext.newPage();

  await signInWithTestAccount(hostPage, {
    name: 'Host',
    email: 'host@example.com',
    roomName,
    role: 'host',
  });
  await joinRoom(hostPage);
  await hostPage
    .locator('[data-testid="meeting-controls"]')
    .waitFor({ state: 'visible', timeout: 20000 });

  await signInWithTestAccount(guestPage, {
    name: 'Guest',
    email: 'guest@example.com',
    roomName,
    role: 'guest',
  });
  await joinRoom(guestPage);
  await guestPage
    .locator('[data-testid="waiting-screen"]')
    .waitFor({ state: 'visible', timeout: 20000 });

  // Admit the guest so they can receive data-channel messages.
  await hostPage.getByRole('button', { name: 'Admit', exact: true }).click();
  await guestPage
    .locator('[data-testid="meeting-controls"]')
    .waitFor({ state: 'visible', timeout: 20000 });

  // Host creates and opens a breakout room.
  await hostPage.locator('[data-testid="breakout-toggle"]').click();
  await hostPage.locator('[data-testid="breakout-room-label"]').fill('VIP Room');
  await hostPage.locator('[data-testid="breakout-add-room"]').click();

  // Assign the only participant (Guest) to the room via the multi-select.
  await hostPage.locator('select.lk-form-control >> nth=0').selectOption({ label: 'Guest' });
  await hostPage.locator('[data-testid="breakout-open"]').click();

  // Guest should see the breakout assignment banner.
  await expect(guestPage.locator('[data-testid="breakout-banner"]')).toBeVisible({
    timeout: 10000,
  });
  await expect(guestPage.locator('[data-testid="breakout-banner"]')).toContainText('VIP Room');
  await expect(guestPage.locator('button:has-text("Join breakout room")')).toBeVisible();
  await captureFeatureScreenshot(guestPage, 'breakouts-guest-assigned');

  // Host should see the active state.
  await expect(hostPage.locator('[data-testid="breakout-controls"]')).toContainText(
    '1 active breakout room',
  );

  // Host closes breakouts.
  await hostPage.locator('[data-testid="breakout-close-all"]').click();
  await expect(guestPage.locator('[data-testid="breakout-banner"]')).toContainText('closed');

  await hostContext.close();
  await guestContext.close();
});
