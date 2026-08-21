import { auth } from '@/auth';
import { admitGuest } from '@/lib/waitingRoom';
import { NextResponse } from 'next/server';

interface RouteParams {
  params: Promise<{ roomName: string; identity: string }>;
}

export async function POST(_request: Request, { params }: RouteParams) {
  try {
    const session = await auth();
    if (!session?.user) {
      return new NextResponse('Unauthorized', { status: 401 });
    }

    const { roomName, identity } = await params;
    const guest = admitGuest(roomName, decodeURIComponent(identity));
    if (!guest) {
      return new NextResponse('Guest not found', { status: 404 });
    }
    return NextResponse.json(guest);
  } catch (error) {
    if (error instanceof Error) {
      return new NextResponse(error.message, { status: 500 });
    }
    return new NextResponse('Internal Server Error', { status: 500 });
  }
}
