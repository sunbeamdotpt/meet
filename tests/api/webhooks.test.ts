import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { POST as webhookPOST } from '@/app/api/webhooks/livekit/route';

const mockReceive = vi.fn();

vi.mock('livekit-server-sdk', async () => {
  const actual = await vi.importActual<typeof import('livekit-server-sdk')>('livekit-server-sdk');
  return {
    ...actual,
    WebhookReceiver: vi.fn().mockImplementation(() => ({
      receive: mockReceive,
    })),
  };
});

describe('LiveKit webhook handler', () => {
  const OLD_ENV = process.env;

  beforeEach(() => {
    process.env = { ...OLD_ENV, LIVEKIT_API_KEY: 'key', LIVEKIT_API_SECRET: 'secret' };
    mockReceive.mockReset();
  });

  afterEach(() => {
    process.env = OLD_ENV;
  });

  it('returns 200 for verified webhook events', async () => {
    mockReceive.mockResolvedValue({
      event: 'room_started',
      room: { name: 'room-1', sid: 'RM_abc' },
    });

    const request = new Request('http://localhost/api/webhooks/livekit', {
      method: 'POST',
      body: '{"event":"room_started"}',
      headers: { Authorization: 'Bearer fake-signature' },
    });

    const response = await webhookPOST(request as unknown as import('next/server').NextRequest);
    expect(response.status).toBe(200);
    const data = await response.json();
    expect(data.received).toBe(true);
    expect(mockReceive).toHaveBeenCalledWith('{"event":"room_started"}', 'Bearer fake-signature');
  });

  it('returns 400 for invalid signatures', async () => {
    mockReceive.mockRejectedValue(new Error('invalid signature'));

    const request = new Request('http://localhost/api/webhooks/livekit', {
      method: 'POST',
      body: '{"event":"room_started"}',
      headers: { Authorization: 'Bearer bad-signature' },
    });

    const response = await webhookPOST(request as unknown as import('next/server').NextRequest);
    expect(response.status).toBe(400);
  });
});
