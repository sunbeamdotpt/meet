import { auth } from '@/auth';
import { getEgressClient } from '@/lib/livekit';
import { NextRequest, NextResponse } from 'next/server';

interface RouteParams {
  params: Promise<{ roomName: string }>;
}

export async function POST(request: NextRequest, { params }: RouteParams) {
  try {
    const session = await auth();
    if (!session?.user) {
      return new NextResponse('Unauthorized', { status: 401 });
    }

    const { roomName } = await params;
    const body = await request.json();
    const { egressId } = body;

    if (typeof egressId !== 'string') {
      return new NextResponse('Missing required field: egressId', { status: 400 });
    }

    const egressClient = getEgressClient();
    const info = await egressClient.stopEgress(egressId);

    return NextResponse.json({ egressId: info.egressId, status: info.status });
  } catch (error) {
    if (error instanceof Error) {
      return new NextResponse(error.message, { status: 500 });
    }
    return new NextResponse('Internal Server Error', { status: 500 });
  }
}
