import { auth } from '@/auth';
import { getRoomServiceClient } from '@/lib/livekit';
import { NextRequest, NextResponse } from 'next/server';

export async function GET() {
  try {
    const session = await auth();
    if (!session?.user) {
      return new NextResponse('Unauthorized', { status: 401 });
    }

    const svc = getRoomServiceClient();
    const rooms = await svc.listRooms();
    return NextResponse.json(rooms);
  } catch (error) {
    if (error instanceof Error) {
      return new NextResponse(error.message, { status: 500 });
    }
    return new NextResponse('Internal Server Error', { status: 500 });
  }
}

export async function POST(request: NextRequest) {
  try {
    const session = await auth();
    if (!session?.user) {
      return new NextResponse('Unauthorized', { status: 401 });
    }

    const body = await request.json();
    const { roomName, metadata, emptyTimeout, maxParticipants } = body;

    if (typeof roomName !== 'string') {
      return new NextResponse('Missing required field: roomName', { status: 400 });
    }

    const svc = getRoomServiceClient();
    const room = await svc.createRoom({
      name: roomName,
      metadata: typeof metadata === 'string' ? metadata : undefined,
      emptyTimeout: typeof emptyTimeout === 'number' ? emptyTimeout : undefined,
      maxParticipants: typeof maxParticipants === 'number' ? maxParticipants : undefined,
    });

    return NextResponse.json(room);
  } catch (error) {
    if (error instanceof Error) {
      return new NextResponse(error.message, { status: 500 });
    }
    return new NextResponse('Internal Server Error', { status: 500 });
  }
}
