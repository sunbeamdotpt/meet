import { auth } from '@/auth';
import { getRoomServiceClient } from '@/lib/livekit';
import { NextRequest, NextResponse } from 'next/server';
import { ParticipantPermission } from 'livekit-server-sdk';

interface RouteParams {
  params: Promise<{ roomName: string; identity: string }>;
}

export async function POST(request: NextRequest, { params }: RouteParams) {
  try {
    const session = await auth();
    if (!session?.user) {
      return new NextResponse('Unauthorized', { status: 401 });
    }

    const { roomName, identity } = await params;
    const body = await request.json();
    const { canSubscribe, canPublish, canPublishData } = body;

    const permission: Partial<ParticipantPermission> = {
      canSubscribe: typeof canSubscribe === 'boolean' ? canSubscribe : true,
      canPublish: typeof canPublish === 'boolean' ? canPublish : true,
      canPublishData: typeof canPublishData === 'boolean' ? canPublishData : true,
    };

    const svc = getRoomServiceClient();
    await svc.updateParticipant(roomName, identity, undefined, permission);
    return NextResponse.json({ success: true });
  } catch (error) {
    if (error instanceof Error) {
      return new NextResponse(error.message, { status: 500 });
    }
    return new NextResponse('Internal Server Error', { status: 500 });
  }
}
