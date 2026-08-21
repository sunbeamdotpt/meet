import { test, expect } from '@playwright/test';
import { signInWithTestAccount, seedMeeting, captureFeatureScreenshot } from './helpers';

test('meetings page shows empty state when signed in', async ({ page }) => {
  await signInWithTestAccount(page, {
    name: 'Meetings User',
    email: 'meetings-empty@example.com',
    roomName: 'meetings-empty-room',
    role: 'host',
  });

  await page.goto('/meetings');
  await expect(page.locator('h1')).toContainText('Upcoming meetings');
  await expect(page.locator('text=meetings-empty@example.com')).toBeVisible();
  await expect(page.locator('text=No upcoming meetings.')).toBeVisible();
  await captureFeatureScreenshot(page, 'meetings-empty');
});

test('meetings page lists an upcoming meeting', async ({ page }) => {
  const roomName = `meetings-seeded-${Date.now()}`;
  const email = `meetings-seeded-${Date.now()}@example.com`;
  const eventTitle = 'Quarterly Planning';

  await seedMeeting({
    roomName,
    eventTitle,
    organizerEmail: email,
    url: `/rooms/${encodeURIComponent(roomName)}?role=host`,
  });

  await signInWithTestAccount(page, {
    name: 'Seeded User',
    email,
    roomName,
    role: 'host',
  });

  await page.goto('/meetings');
  await expect(page.locator('h1')).toContainText('Upcoming meetings');
  await expect(page.locator(`text=${eventTitle}`)).toBeVisible();
  await expect(page.locator('a:has-text("Join")')).toHaveAttribute(
    'href',
    `/rooms/${encodeURIComponent(roomName)}?role=host`,
  );
  await captureFeatureScreenshot(page, 'meetings-with-meeting');
});
