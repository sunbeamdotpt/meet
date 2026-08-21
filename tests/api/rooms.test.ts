import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { NextRequest } from 'next/server';
import { GET as listRooms, POST as createRoom } from '@/app/api/rooms/route';

const mockListRooms = vi.fn();
const mockCreateRoom = vi.fn();
const mockDeleteRoom = vi.fn();

vi.mock('@/auth', () => ({
  auth: vi.fn(() =>
    Promise.resolve({
      user: { id: 'user-123', email: 'host@example.com', name: 'Host User' },
    }),
  ),
}));

vi.mock('@/lib/livekit', () => ({
  getRoomServiceClient: vi.fn(() => ({
    listRooms: mockListRooms,
    createRoom: mockCreateRoom,
    deleteRoom: mockDeleteRoom,
  })),
  getEgressClient: vi.fn(),
}));

describe('Rooms API', () => {
  const OLD_ENV = process.env;

  beforeEach(() => {
    process.env = { ...OLD_ENV, LIVEKIT_URL: 'wss://test.livekit.cloud' };
    mockListRooms.mockReset();
    mockCreateRoom.mockReset();
    mockDeleteRoom.mockReset();
  });

  afterEach(() => {
    process.env = OLD_ENV;
  });

  it('lists rooms', async () => {
    mockListRooms.mockResolvedValue([{ name: 'room-1', numParticipants: 2 }]);

    const response = await listRooms();
    expect(response.status).toBe(200);
    const data = await response.json();
    expect(data).toEqual([{ name: 'room-1', numParticipants: 2 }]);
  });

  it('creates a room with metadata', async () => {
    mockCreateRoom.mockResolvedValue({ name: 'room-2', metadata: '{"eventId":"cal-1"}' });

    const request = new NextRequest('http://localhost/api/rooms', {
      method: 'POST',
      body: JSON.stringify({
        roomName: 'room-2',
        metadata: '{"eventId":"cal-1"}',
        emptyTimeout: 300,
        maxParticipants: 50,
      }),
    });

    const response = await createRoom(request);
    expect(response.status).toBe(200);
    const data = await response.json();
    expect(data.name).toBe('room-2');
    expect(mockCreateRoom).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'room-2',
        metadata: '{"eventId":"cal-1"}',
        emptyTimeout: 300,
        maxParticipants: 50,
      }),
    );
  });
});
