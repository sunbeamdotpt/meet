import { auth } from '@/auth';
import { getRoomServiceClient } from '@/lib/livekit';
import { NextResponse } from 'next/server';

interface RouteParams {
  params: Promise<{ roomName: string }>;
}

export async function GET(_request: Request, { params }: RouteParams) {
  try {
    const session = await auth();
    if (!session?.user) {
      return new NextResponse('Unauthorized', { status: 401 });
    }

    const { roomName } = await params;
    const svc = getRoomServiceClient();
    const rooms = await svc.listRooms([roomName]);
    if (rooms.length === 0) {
      return new NextResponse('Room not found', { status: 404 });
    }
    return NextResponse.json(rooms[0]);
  } catch (error) {
    if (error instanceof Error) {
      return new NextResponse(error.message, { status: 500 });
    }
    return new NextResponse('Internal Server Error', { status: 500 });
  }
}

export async function DELETE(_request: Request, { params }: RouteParams) {
  try {
    const session = await auth();
    if (!session?.user) {
      return new NextResponse('Unauthorized', { status: 401 });
    }

    const { roomName } = await params;
    const svc = getRoomServiceClient();
    await svc.deleteRoom(roomName);
    return new NextResponse(null, { status: 204 });
  } catch (error) {
    if (error instanceof Error) {
      return new NextResponse(error.message, { status: 500 });
    }
    return new NextResponse('Internal Server Error', { status: 500 });
  }
}
