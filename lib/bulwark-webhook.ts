// SPDX-License-Identifier: AGPL-3.0-or-later
import { createHmac, timingSafeEqual } from 'crypto';

export const BULWARK_WEBHOOK_HEADER = 'x-bulwark-signature';

export interface BulwarkWebhookPayload {
  event: string;
  roomName?: string;
  roomSid?: string;
  participantIdentity?: string;
  egressId?: string;
  startedAt?: string;
  endedAt?: string;
}

export function signBulwarkWebhookPayload(payload: BulwarkWebhookPayload, secret: string): string {
  const body = JSON.stringify(payload);
  return createHmac('sha256', secret).update(body, 'utf-8').digest('hex');
}

export function verifyBulwarkWebhookSignature(
  payload: BulwarkWebhookPayload,
  signature: string,
  secret: string,
): boolean {
  const expected = signBulwarkWebhookPayload(payload, secret);
  const expectedBuf = Buffer.from(expected, 'hex');
  const signatureBuf = Buffer.from(signature, 'hex');

  if (expectedBuf.length !== signatureBuf.length) {
    return false;
  }

  return timingSafeEqual(expectedBuf, signatureBuf);
}

export async function forwardToBulwark(payload: BulwarkWebhookPayload): Promise<void> {
  const url = process.env.BULWARK_WEBHOOK_URL;
  const secret = process.env.BULWARK_WEBHOOK_SECRET;

  if (!url || !secret) {
    return;
  }

  const signature = signBulwarkWebhookPayload(payload, secret);

  const res = await fetch(url, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      [BULWARK_WEBHOOK_HEADER]: signature,
    },
    body: JSON.stringify(payload),
  });

  if (!res.ok) {
    throw new Error(`Bulwark webhook returned HTTP ${res.status}`);
  }
}
