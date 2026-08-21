import { auth } from '@/auth';
import { getRoomServiceClient } from '@/lib/livekit';
import { randomString } from '@/lib/client-utils';
import { NextRequest, NextResponse } from 'next/server';

interface RouteParams {
  params: Promise<{ roomName: string }>;
}

interface BreakoutRoom {
  id: string;
  name: string;
  label: string;
}

interface BreakoutState {
  active: boolean;
  mainRoom: string;
  rooms: BreakoutRoom[];
  assignments: Record<string, string>;
  createdAt: number;
}

const BREAKOUT_METADATA_KEY = 'breakouts';

async function getMainRoom(roomName: string) {
  const svc = getRoomServiceClient();
  const rooms = await svc.listRooms([roomName]);
  return rooms[0];
}

function parseBreakoutMetadata(metadata?: string): BreakoutState | undefined {
  if (!metadata) return undefined;
  try {
    const parsed = JSON.parse(metadata);
    if (parsed && parsed[BREAKOUT_METADATA_KEY]) {
      return parsed[BREAKOUT_METADATA_KEY] as BreakoutState;
    }
  } catch {
    // ignore malformed metadata
  }
  return undefined;
}

function buildBreakoutRoomName(mainRoom: string, id: string): string {
  const base = `${mainRoom}-breakout-${id}`;
  // LiveKit room names have a 64 character limit in most deployments.
  return base.slice(0, 64);
}

export async function GET(_request: NextRequest, { params }: RouteParams) {
  try {
    const session = await auth();
    if (!session?.user) {
      return new NextResponse('Unauthorized', { status: 401 });
    }

    const { roomName } = await params;
    const room = await getMainRoom(roomName);
    const state = parseBreakoutMetadata(room?.metadata);
    return NextResponse.json({ state: state ?? null });
  } catch (error) {
    if (error instanceof Error) {
      return new NextResponse(error.message, { status: 500 });
    }
    return new NextResponse('Internal Server Error', { status: 500 });
  }
}

export async function POST(request: NextRequest, { params }: RouteParams) {
  try {
    const session = await auth();
    if (!session?.user) {
      return new NextResponse('Unauthorized', { status: 401 });
    }

    const { roomName } = await params;
    const body = await request.json();
    const role = body?.role;
    const roomsInput = body?.rooms;

    if (role !== 'host') {
      return new NextResponse('Forbidden', { status: 403 });
    }
    if (!Array.isArray(roomsInput) || roomsInput.length === 0) {
      return new NextResponse('Missing required field: rooms', { status: 400 });
    }

    const svc = getRoomServiceClient();
    const breakoutRooms: BreakoutRoom[] = [];
    const assignments: Record<string, string> = {};

    for (const input of roomsInput) {
      const label = typeof input.label === 'string' ? input.label : 'Breakout';
      const participants = Array.isArray(input.participants) ? input.participants : [];
      const id = randomString(6);
      const name = buildBreakoutRoomName(roomName, id);

      await svc.createRoom({
        name,
        metadata: JSON.stringify({ breakoutMainRoom: roomName, breakoutLabel: label }),
      });

      breakoutRooms.push({ id, name, label });
      for (const identity of participants) {
        if (typeof identity === 'string' && identity) {
          assignments[identity] = name;
        }
      }
    }

    const state: BreakoutState = {
      active: true,
      mainRoom: roomName,
      rooms: breakoutRooms,
      assignments,
      createdAt: Date.now(),
    };

    await svc.updateRoomMetadata(roomName, JSON.stringify({ [BREAKOUT_METADATA_KEY]: state }));

    return NextResponse.json({ state });
  } catch (error) {
    if (error instanceof Error) {
      return new NextResponse(error.message, { status: 500 });
    }
    return new NextResponse('Internal Server Error', { status: 500 });
  }
}

export async function DELETE(request: NextRequest, { params }: RouteParams) {
  try {
    const session = await auth();
    if (!session?.user) {
      return new NextResponse('Unauthorized', { status: 401 });
    }

    const { roomName } = await params;
    const role = request.nextUrl.searchParams.get('role');
    if (role !== 'host') {
      return new NextResponse('Forbidden', { status: 403 });
    }

    const svc = getRoomServiceClient();
    const room = await getMainRoom(roomName);
    const state = parseBreakoutMetadata(room?.metadata);

    if (state?.rooms) {
      await Promise.all(
        state.rooms.map((r) =>
          svc.deleteRoom(r.name).catch((err) => {
            console.warn(`Failed to delete breakout room ${r.name}:`, err);
          }),
        ),
      );
    }

    await svc.updateRoomMetadata(roomName, JSON.stringify({ [BREAKOUT_METADATA_KEY]: { active: false } }));

    return NextResponse.json({ success: true });
  } catch (error) {
    if (error instanceof Error) {
      return new NextResponse(error.message, { status: 500 });
    }
    return new NextResponse('Internal Server Error', { status: 500 });
  }
}
