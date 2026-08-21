import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { POST } from '@/app/api/bulwark/rooms/route';

const createRoomMock = vi.fn();

vi.mock('@/lib/oidc', () => ({
  verifyBearerToken: vi.fn(),
}));

vi.mock('@/lib/livekit', () => ({
  getRoomServiceClient: vi.fn(() => ({
    createRoom: createRoomMock,
  })),
}));

vi.mock('@/lib/db', () => ({
  createMeeting: vi.fn(),
  generateRoomName: vi.fn(() => 'team-sync-abcdef12'),
  roomNameFromEventUid: vi.fn(() => 'event-uid-abcdef12'),
}));

import { verifyBearerToken } from '@/lib/oidc';
import { createMeeting } from '@/lib/db';

describe('POST /api/bulwark/rooms', () => {
  const originalEnv = process.env;

  beforeEach(() => {
    process.env = { ...originalEnv, MEET_BASE_URL: 'https://meet.example.com' };
    vi.clearAllMocks();
  });

  afterEach(() => {
    process.env = originalEnv;
  });

  function makeRequest(body: object, token: string) {
    return new Request('http://localhost/api/bulwark/rooms', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify(body),
    });
  }

  it('returns 401 without an authorization header', async () => {
    const request = new Request('http://localhost/api/bulwark/rooms', {
      method: 'POST',
      body: JSON.stringify({ eventTitle: 'Team Sync' }),
    });

    const response = await POST(request);
    expect(response.status).toBe(401);
  });

  it('returns 401 when the bearer token is invalid', async () => {
    vi.mocked(verifyBearerToken).mockResolvedValueOnce(null);

    const request = makeRequest({ eventTitle: 'Team Sync' }, 'bad-token');
    const response = await POST(request);

    expect(response.status).toBe(401);
  });

  it('returns 400 when the token has no email', async () => {
    vi.mocked(verifyBearerToken).mockResolvedValueOnce({ sub: 'user-1' });

    const request = makeRequest({ eventTitle: 'Team Sync' }, 'valid-token');
    const response = await POST(request);

    expect(response.status).toBe(400);
  });

  it('creates a meeting and returns the host URL', async () => {
    vi.mocked(verifyBearerToken).mockResolvedValueOnce({
      sub: 'user-1',
      email: 'host@example.com',
      name: 'Host User',
    });
    createRoomMock.mockResolvedValueOnce({ name: 'team-sync-abcdef12' });
    vi.mocked(createMeeting).mockResolvedValueOnce({
      id: 'meeting-1',
      room_name: 'team-sync-abcdef12',
      event_title: 'Team Sync',
      event_uid: null,
      organizer_email: 'host@example.com',
      organizer_name: 'Host User',
      start_time: null,
      end_time: null,
      url: 'https://meet.example.com/rooms/team-sync-abcdef12?role=host',
      created_at: new Date(),
    });

    const request = makeRequest({ eventTitle: 'Team Sync' }, 'valid-token');
    const response = await POST(request);
    const data = await response.json();

    expect(response.status).toBe(200);
    expect(data.url).toBe('https://meet.example.com/rooms/team-sync-abcdef12?role=host');
    expect(data.roomName).toBe('team-sync-abcdef12');
    expect(createMeeting).toHaveBeenCalledWith(
      expect.objectContaining({
        roomName: 'team-sync-abcdef12',
        eventTitle: 'Team Sync',
        organizerEmail: 'host@example.com',
        organizerName: 'Host User',
        url: 'https://meet.example.com/rooms/team-sync-abcdef12?role=host',
      }),
    );
  });
});
