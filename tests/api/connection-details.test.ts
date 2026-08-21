import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { NextRequest } from 'next/server';
import { GET } from '@/app/api/connection-details/route';

vi.mock('@/auth', () => ({
  auth: vi.fn(() =>
    Promise.resolve({
      user: { id: 'user-123', email: 'host@example.com', name: 'Host User' },
    }),
  ),
}));

vi.mock('@/lib/token', () => ({
  createParticipantToken: vi.fn(() => Promise.resolve('fake-jwt-token')),
}));

describe('GET /api/connection-details', () => {
  const OLD_ENV = process.env;

  beforeEach(() => {
    process.env = { ...OLD_ENV, LIVEKIT_URL: 'wss://test.livekit.cloud' };
  });

  afterEach(() => {
    process.env = OLD_ENV;
  });

  it('returns connection details with token', async () => {
    const request = new NextRequest(
      'http://localhost/api/connection-details?roomName=room-1&role=host',
    );
    const response = await GET(request);
    expect(response.status).toBe(200);

    const data = await response.json();
    expect(data.roomName).toBe('room-1');
    expect(data.serverUrl).toBe('wss://test.livekit.cloud');
    expect(data.participantName).toBe('Host User');
    expect(data.participantToken).toBe('fake-jwt-token');
  });

  it('defaults to guest role when role is omitted', async () => {
    const { createParticipantToken } = await import('@/lib/token');
    const request = new NextRequest('http://localhost/api/connection-details?roomName=room-1');
    await GET(request);

    expect(createParticipantToken).toHaveBeenCalledWith(
      expect.objectContaining({ role: 'guest' }),
      'room-1',
    );
  });

  it('returns 401 when not authenticated', async () => {
    const { auth } = await import('@/auth');
    vi.mocked(auth).mockResolvedValueOnce(null as never);

    const request = new NextRequest('http://localhost/api/connection-details?roomName=room-1');
    const response = await GET(request);
    expect(response.status).toBe(401);
  });
});
