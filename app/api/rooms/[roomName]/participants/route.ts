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
    const participants = await svc.listParticipants(roomName);
    return NextResponse.json(participants);
  } catch (error) {
    if (error instanceof Error) {
      return new NextResponse(error.message, { status: 500 });
    }
    return new NextResponse('Internal Server Error', { status: 500 });
  }
}
