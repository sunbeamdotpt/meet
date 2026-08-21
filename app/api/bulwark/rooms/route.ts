// SPDX-License-Identifier: AGPL-3.0-or-later
import { NextRequest, NextResponse } from 'next/server';
import { verifyBearerToken } from '@/lib/oidc';
import { createMeeting, generateRoomName, roomNameFromEventUid } from '@/lib/db';
import { buildMeetingUrl } from '@/lib/url';
import { getRoomServiceClient } from '@/lib/livekit';

interface CreateRoomBody {
  eventTitle?: unknown;
  eventUid?: unknown;
  startTime?: unknown;
  endTime?: unknown;
}

function parseStartEnd(body: CreateRoomBody): { startTime?: Date; endTime?: Date } {
  const startTime =
    typeof body.startTime === 'string' && body.startTime.length > 0
      ? new Date(body.startTime)
      : undefined;
  const endTime =
    typeof body.endTime === 'string' && body.endTime.length > 0
      ? new Date(body.endTime)
      : undefined;

  return {
    startTime: startTime && !isNaN(startTime.getTime()) ? startTime : undefined,
    endTime: endTime && !isNaN(endTime.getTime()) ? endTime : undefined,
  };
}

export async function POST(request: NextRequest) {
  try {
    const authHeader = request.headers.get('authorization');
    if (!authHeader || !authHeader.startsWith('Bearer ')) {
      return NextResponse.json({ error: 'Unauthorized' }, { status: 401 });
    }

    const token = authHeader.slice(7);
    const userinfo = await verifyBearerToken(token);
    if (!userinfo) {
      return NextResponse.json({ error: 'Unauthorized' }, { status: 401 });
    }

    const email = userinfo.email;
    if (!email) {
      return NextResponse.json({ error: 'OIDC token missing email claim' }, { status: 400 });
    }

    const body = (await request.json()) as CreateRoomBody;
    const eventTitle =
      typeof body.eventTitle === 'string' && body.eventTitle.length > 0
        ? body.eventTitle
        : 'Meeting';

    const roomName =
      typeof body.eventUid === 'string' && body.eventUid.length > 0
        ? roomNameFromEventUid(body.eventUid)
        : generateRoomName(eventTitle);

    const { startTime, endTime } = parseStartEnd(body);

    // Ensure the room exists in LiveKit so it is ready when the organizer joins.
    try {
      const svc = getRoomServiceClient();
      await svc.createRoom({ name: roomName });
    } catch (err) {
      // Room may already exist; continue.
      if (err instanceof Error && !err.message.includes('already exists')) {
        console.error('Failed to create LiveKit room:', err);
      }
    }

    const url = buildMeetingUrl(roomName, 'host');

    await createMeeting({
      roomName,
      eventTitle,
      eventUid: typeof body.eventUid === 'string' ? body.eventUid : undefined,
      organizerEmail: email,
      organizerName: userinfo.name,
      startTime,
      endTime,
      url,
    });

    return NextResponse.json({ url, roomName });
  } catch (error) {
    if (error instanceof Error) {
      console.error('Bulwark room creation error:', error);
      return NextResponse.json({ error: error.message }, { status: 500 });
    }
    return NextResponse.json({ error: 'Internal Server Error' }, { status: 500 });
  }
}
