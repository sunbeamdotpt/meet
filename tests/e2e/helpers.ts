import { Page } from '@playwright/test';
import { promises as fs } from 'fs';
import path from 'path';

export async function captureFeatureScreenshot(page: Page, name: string) {
  const dir = path.join(process.cwd(), 'test-results', 'screenshots');
  await fs.mkdir(dir, { recursive: true });
  await page.screenshot({ path: path.join(dir, `${name}.png`) });
}

export async function signInWithTestAccount(
  page: Page,
  options: { name: string; email: string; roomName: string; role: 'host' | 'guest' },
) {
  const callbackUrl = `/rooms/${encodeURIComponent(options.roomName)}?role=${options.role}`;
  const signInUrl = `/test-signin?name=${encodeURIComponent(options.name)}&email=${encodeURIComponent(options.email)}&callbackUrl=${encodeURIComponent(callbackUrl)}`;
  await page.goto(signInUrl);
  await page.waitForURL(callbackUrl, { timeout: 15000 });
}

export async function joinRoom(page: Page) {
  // The PreJoin component renders a "Join" button to enter the room.
  const joinButton = page.locator('button:has-text("Join")');
  await joinButton.waitFor({ state: 'visible', timeout: 10000 });
  await joinButton.click();
}

export async function seedMeeting(options: {
  roomName: string;
  eventTitle: string;
  organizerEmail: string;
  url: string;
}) {
  const response = await fetch('http://localhost:3000/api/test/seed-meeting', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(options),
  });
  if (!response.ok) {
    throw new Error(`Failed to seed meeting: ${await response.text()}`);
  }
  return response.json();
}
