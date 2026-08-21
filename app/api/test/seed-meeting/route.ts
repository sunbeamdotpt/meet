// SPDX-License-Identifier: AGPL-3.0-or-later
// Test-only endpoint used by Playwright to seed a meeting for the signed-in user.
import { NextRequest, NextResponse } from 'next/server';
import { createMeeting } from '@/lib/db';

export async function POST(request: NextRequest) {
  if (process.env.ALLOW_TEST_AUTH !== 'true') {
    return new NextResponse('Test endpoints are disabled', { status: 403 });
  }

  const body = (await request.json()) as {
    roomName?: string;
    eventTitle?: string;
    organizerEmail?: string;
    url?: string;
  };

  const roomName = body.roomName ?? `test-room-${Date.now()}`;
  const eventTitle = body.eventTitle ?? 'Test Meeting';
  const organizerEmail = body.organizerEmail ?? 'test@example.com';
  const url = body.url ?? `/rooms/${encodeURIComponent(roomName)}?role=host`;

  const startTime = new Date(Date.now() - 5 * 60 * 1000);
  const endTime = new Date(Date.now() + 55 * 60 * 1000);

  try {
    const meeting = await createMeeting({
      roomName,
      eventTitle,
      organizerEmail,
      startTime,
      endTime,
      url,
    });
    return NextResponse.json(meeting);
  } catch (error) {
    console.error('Failed to seed meeting:', error);
    return NextResponse.json({ error: 'Failed to seed meeting' }, { status: 500 });
  }
}
