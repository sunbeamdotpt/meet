import { test, expect } from '@playwright/test';
import { signInWithTestAccount } from './helpers';

test('E2EE passphrase is derived and returned by connection details', async ({ page }) => {
  const roomName = `e2ee-${Date.now()}`;
  await signInWithTestAccount(page, {
    name: 'E2EE Host',
    email: 'e2ee-host@example.com',
    roomName,
    role: 'host',
  });

  const details = await page.evaluate(async (name) => {
    const url = new URL('/api/connection-details', window.location.origin);
    url.searchParams.set('roomName', name);
    url.searchParams.set('participantName', 'E2EE Host');
    url.searchParams.set('role', 'host');
    const res = await fetch(url.toString());
    return res.json();
  }, roomName);

  expect(details).toMatchObject({
    roomName,
    participantName: 'E2EE Host',
  });
  expect(typeof details.e2eePassphrase).toBe('string');
  expect(details.e2eePassphrase.length).toBeGreaterThan(0);
});
