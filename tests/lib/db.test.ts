import { describe, it, expect, vi, beforeEach } from 'vitest';
import { Pool } from 'pg';

const queryMock = vi.fn();
const connectMock = vi.fn();
const releaseMock = vi.fn();

vi.mock('pg', () => ({
  Pool: vi.fn(() => ({
    query: queryMock,
    connect: connectMock,
  })),
}));

describe('database helpers', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    process.env.DATABASE_URL = 'postgres://test:test@localhost:5432/test';
    connectMock.mockResolvedValue({
      query: queryMock,
      release: releaseMock,
    });
  });

  it('generateRoomName creates a URL-safe slug with suffix', async () => {
    const { generateRoomName } = await import('@/lib/db');
    const name = generateRoomName('Team Sync: Q3 Review!');
    expect(name).toMatch(/^team-sync-q3-review-[a-f0-9]{8}$/);
  });

  it('roomNameFromEventUid creates a stable room name', async () => {
    const { roomNameFromEventUid } = await import('@/lib/db');
    const name = roomNameFromEventUid('event-uid-123@calendar');
    expect(name).toMatch(/^event-uid-123-calendar-[a-f0-9]{16}$/);
  });

  it('createMeeting inserts a meeting', async () => {
    const { createMeeting } = await import('@/lib/db');

    const meeting = {
      id: 'meeting-1',
      room_name: 'team-sync-abcdef12',
      event_title: 'Team Sync',
      event_uid: 'uid-1',
      organizer_email: 'host@example.com',
      organizer_name: 'Host',
      start_time: new Date('2026-08-22T10:00:00Z'),
      end_time: new Date('2026-08-22T11:00:00Z'),
      url: 'https://meet.example.com/rooms/team-sync-abcdef12?role=host',
      created_at: new Date(),
    };

    queryMock
      .mockResolvedValueOnce({}) // CREATE TABLE schema_migrations
      .mockResolvedValueOnce({ rows: [] }) // SELECT filename
      .mockResolvedValueOnce({}) // CREATE TABLE meetings
      .mockResolvedValueOnce({}) // INSERT schema_migrations
      .mockResolvedValueOnce({ rows: [meeting] }); // INSERT meetings

    const result = await createMeeting({
      roomName: meeting.room_name,
      eventTitle: meeting.event_title,
      eventUid: meeting.event_uid,
      organizerEmail: meeting.organizer_email,
      organizerName: meeting.organizer_name,
      startTime: meeting.start_time,
      endTime: meeting.end_time,
      url: meeting.url,
    });

    expect(result).toEqual(meeting);
    expect(queryMock).toHaveBeenCalledTimes(5); // migrations (4) + insert
  });

  it('getUpcomingMeetings queries by organizer email', async () => {
    const { getUpcomingMeetings } = await import('@/lib/db');

    const meetings = [
      {
        id: 'meeting-1',
        room_name: 'team-sync-abcdef12',
        event_title: 'Team Sync',
        event_uid: 'uid-1',
        organizer_email: 'host@example.com',
        organizer_name: 'Host',
        start_time: new Date('2026-08-22T10:00:00Z'),
        end_time: new Date('2026-08-22T11:00:00Z'),
        url: 'https://meet.example.com/rooms/team-sync-abcdef12?role=host',
        created_at: new Date(),
      },
    ];

    queryMock
      .mockResolvedValueOnce({}) // CREATE TABLE schema_migrations
      .mockResolvedValueOnce({ rows: [] }) // SELECT filename
      .mockResolvedValueOnce({}) // CREATE TABLE meetings
      .mockResolvedValueOnce({}) // INSERT schema_migrations
      .mockResolvedValueOnce({ rows: meetings }); // SELECT meetings

    const result = await getUpcomingMeetings('host@example.com');

    expect(result).toEqual(meetings);
    expect(queryMock).toHaveBeenCalledTimes(5); // migrations (4) + select
  });
});
