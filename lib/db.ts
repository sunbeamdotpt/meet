// SPDX-License-Identifier: AGPL-3.0-or-later
import { Pool, PoolClient, QueryResult } from 'pg';
import { randomBytes } from 'crypto';
import { readFile } from 'fs/promises';
import { join } from 'path';

export interface Meeting {
  id: string;
  room_name: string;
  event_title: string;
  event_uid: string | null;
  organizer_email: string;
  organizer_name: string | null;
  start_time: Date | null;
  end_time: Date | null;
  url: string;
  created_at: Date;
}

export interface CreateMeetingInput {
  roomName: string;
  eventTitle: string;
  eventUid?: string;
  organizerEmail: string;
  organizerName?: string | null;
  startTime?: Date | string | null;
  endTime?: Date | string | null;
  url: string;
}

let pool: Pool | null = null;
let migrationsRan = false;

export function getPool(): Pool {
  if (!pool) {
    const connectionString = process.env.DATABASE_URL;
    if (!connectionString) {
      throw new Error('DATABASE_URL is not defined');
    }
    pool = new Pool({ connectionString });
  }
  return pool;
}

export async function withClient<T>(fn: (client: PoolClient) => Promise<T>): Promise<T> {
  const client = await getPool().connect();
  try {
    return await fn(client);
  } finally {
    client.release();
  }
}

export async function ensureMigrated(): Promise<void> {
  if (migrationsRan) return;
  await migrate();
  migrationsRan = true;
}

export async function migrate(): Promise<void> {
  await withClient(async (client) => {
    await client.query(`
      CREATE TABLE IF NOT EXISTS schema_migrations (
        filename TEXT PRIMARY KEY,
        applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
      );
    `);

    const migrations = ['001_meetings.sql'];

    for (const filename of migrations) {
      const { rows } = await client.query<{ filename: string }>(
        'SELECT filename FROM schema_migrations WHERE filename = $1',
        [filename],
      );
      if (rows.length > 0) continue;

      const path = join(process.cwd(), 'migrations', filename);
      const sql = await readFile(path, 'utf-8');

      await client.query(sql);
      await client.query('INSERT INTO schema_migrations (filename) VALUES ($1)', [filename]);
    }
  });
}

export async function createMeeting(input: CreateMeetingInput): Promise<Meeting> {
  await ensureMigrated();
  const startTime =
    input.startTime instanceof Date ? input.startTime.toISOString() : input.startTime || null;
  const endTime =
    input.endTime instanceof Date ? input.endTime.toISOString() : input.endTime || null;

  const result = await getPool().query<Meeting>(
    `
    INSERT INTO meetings (
      room_name, event_title, event_uid, organizer_email, organizer_name,
      start_time, end_time, url
    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
    ON CONFLICT (event_uid) DO UPDATE SET
      room_name = EXCLUDED.room_name,
      event_title = EXCLUDED.event_title,
      organizer_email = EXCLUDED.organizer_email,
      organizer_name = EXCLUDED.organizer_name,
      start_time = EXCLUDED.start_time,
      end_time = EXCLUDED.end_time,
      url = EXCLUDED.url
    RETURNING *
    `,
    [
      input.roomName,
      input.eventTitle,
      input.eventUid || null,
      input.organizerEmail,
      input.organizerName || null,
      startTime,
      endTime,
      input.url,
    ],
  );

  return result.rows[0];
}

export async function getUpcomingMeetings(organizerEmail: string): Promise<Meeting[]> {
  await ensureMigrated();
  const result = await getPool().query<Meeting>(
    `
    SELECT *
    FROM meetings
    WHERE organizer_email = $1
      AND (start_time IS NULL OR start_time > NOW() - INTERVAL '1 hour')
    ORDER BY start_time ASC NULLS LAST, created_at ASC
    `,
    [organizerEmail],
  );
  return result.rows;
}

export function generateRoomName(eventTitle: string): string {
  const slug = eventTitle
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, 60);

  const suffix = randomBytes(4).toString('hex');
  return slug ? `${slug}-${suffix}` : suffix;
}

export function roomNameFromEventUid(eventUid: string): string {
  // Produce a stable, URL-safe room name from an event UID without exposing
  // the original UID.
  const hash = randomBytes(8).toString('hex');
  const slug = eventUid
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, 40);
  return slug ? `${slug}-${hash}` : hash;
}
