import { Page } from '@playwright/test';

export async function signInWithTestAccount(
  page: Page,
  options: { name: string; email: string; roomName: string; role: 'host' | 'guest' },
) {
  const callbackUrl = `/rooms/${encodeURIComponent(options.roomName)}?role=${options.role}`;
  await page.goto(`/api/auth/signin?callbackUrl=${encodeURIComponent(callbackUrl)}`);
  await page.locator('input[name="name"]').fill(options.name);
  await page.locator('input[name="email"]').fill(options.email);
  await page.locator('button[type="submit"]').click();
  await page.waitForURL(new RegExp(`^.*${callbackUrl.replace(/\?/g, '\\?')}.*`), {
    timeout: 10000,
  });
}

export async function joinRoom(page: Page) {
  // The PreJoin component renders a "Join" button to enter the room.
  const joinButton = page.locator('button:has-text("Join")');
  await joinButton.waitFor({ state: 'visible', timeout: 10000 });
  await joinButton.click();
}
