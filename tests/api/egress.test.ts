import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { POST as startRecording } from '@/app/api/rooms/[roomName]/record/start/route';

const mockStartRoomCompositeEgress = vi.fn();
const mockStopEgress = vi.fn();

vi.mock('@/auth', () => ({
  auth: vi.fn(() =>
    Promise.resolve({
      user: { id: 'user-123', email: 'host@example.com', name: 'Host User' },
    }),
  ),
}));

vi.mock('@/lib/livekit', () => ({
  getRoomServiceClient: vi.fn(),
  getEgressClient: vi.fn(() => ({
    startRoomCompositeEgress: mockStartRoomCompositeEgress,
    stopEgress: mockStopEgress,
  })),
}));

describe('Egress recording API', () => {
  const OLD_ENV = process.env;

  beforeEach(() => {
    process.env = {
      ...OLD_ENV,
      LIVEKIT_URL: 'wss://test.livekit.cloud',
      S3_BUCKET: 'recordings',
      S3_KEY_ID: 'key',
      S3_KEY_SECRET: 'secret',
      S3_REGION: 'us-east-1',
    };
    mockStartRoomCompositeEgress.mockReset();
    mockStopEgress.mockReset();
  });

  afterEach(() => {
    process.env = OLD_ENV;
  });

  it('starts room composite egress and returns egressId', async () => {
    mockStartRoomCompositeEgress.mockResolvedValue({
      egressId: 'EG_abc',
      status: 'EGRESS_STARTING',
    });

    const response = await startRecording(
      new Request('http://localhost/api/rooms/room-1/record/start', { method: 'POST' }),
      { params: Promise.resolve({ roomName: 'room-1' }) },
    );

    expect(response.status).toBe(200);
    const data = await response.json();
    expect(data.egressId).toBe('EG_abc');
    expect(mockStartRoomCompositeEgress).toHaveBeenCalledWith(
      'room-1',
      expect.anything(),
      expect.objectContaining({ layout: 'grid' }),
    );
  });

  it('returns 503 when S3 bucket is not configured', async () => {
    process.env.S3_BUCKET = '';

    const response = await startRecording(
      new Request('http://localhost/api/rooms/room-1/record/start', { method: 'POST' }),
      { params: Promise.resolve({ roomName: 'room-1' }) },
    );

    expect(response.status).toBe(503);
  });
});
