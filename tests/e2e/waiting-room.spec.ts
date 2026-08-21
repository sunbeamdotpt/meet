import { test, expect } from '@playwright/test';
import { signInWithTestAccount, joinRoom } from './helpers';

test('host can admit a waiting guest', async ({ browser }) => {
  const roomName = `waiting-${Date.now()}`;

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
  await expect(guestPage.locator('[data-testid="waiting-screen"]')).toBeVisible({ timeout: 20000 });
  await expect(guestPage.locator('text=Waiting for host')).toBeVisible();

  const admitButton = hostPage.getByRole('button', { name: 'Admit', exact: true });
  await expect(admitButton).toBeVisible({ timeout: 20000 });

  await admitButton.click();

  await expect(guestPage.locator('[data-testid="waiting-screen"]')).not.toBeVisible({
    timeout: 20000,
  });
  await expect(guestPage.locator('[data-testid="meeting-controls"]')).toBeVisible({
    timeout: 20000,
  });

  await hostContext.close();
  await guestContext.close();
});
