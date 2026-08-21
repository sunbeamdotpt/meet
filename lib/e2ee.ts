import crypto from 'crypto';

const E2EE_SECRET = process.env.E2EE_SECRET;

export function deriveE2EEPassphrase(roomName: string): string {
  const secret = E2EE_SECRET ?? 'default-e2ee-secret-change-me';
  return crypto.createHmac('sha256', secret).update(roomName).digest('hex').slice(0, 64);
}
