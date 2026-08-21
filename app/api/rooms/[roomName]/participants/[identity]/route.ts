import { auth } from '@/auth';
import { getRoomServiceClient } from '@/lib/livekit';
import { NextResponse } from 'next/server';

interface RouteParams {
  params: Promise<{ roomName: string; identity: string }>;
}

export async function DELETE(_request: Request, { params }: RouteParams) {
  try {
    const session = await auth();
    if (!session?.user) {
      return new NextResponse('Unauthorized', { status: 401 });
    }

    const { roomName, identity } = await params;
    const svc = getRoomServiceClient();
    await svc.removeParticipant(roomName, identity);
    return new NextResponse(null, { status: 204 });
  } catch (error) {
    if (error instanceof Error) {
      return new NextResponse(error.message, { status: 500 });
    }
    return new NextResponse('Internal Server Error', { status: 500 });
  }
}
