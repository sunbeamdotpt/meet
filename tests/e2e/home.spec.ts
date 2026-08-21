import { test, expect } from '@playwright/test';
import { signInWithTestAccount, captureFeatureScreenshot } from './helpers';

test('home page prompts sign in when signed out', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('h1')).toContainText('Video Calls');
  await expect(page.locator('text=Sign in to join a meeting')).toBeVisible();
  await expect(page.locator('button:has-text("Sign in")')).toBeVisible();
  await captureFeatureScreenshot(page, 'home-signed-out');
});

test('home page shows upcoming meetings link when signed in', async ({ page }) => {
  await signInWithTestAccount(page, {
    name: 'Home User',
    email: 'home-user@example.com',
    roomName: 'home-test-room',
    role: 'host',
  });

  // The callbackUrl redirect lands on the room page; navigate back to home.
  await page.goto('/');
  await expect(page.locator('h1')).toContainText('Video Calls');
  await expect(page.locator('text=home-user@example.com')).toBeVisible();
  await expect(page.locator('a:has-text("Upcoming meetings")')).toBeVisible();
  await captureFeatureScreenshot(page, 'home-signed-in');
});
