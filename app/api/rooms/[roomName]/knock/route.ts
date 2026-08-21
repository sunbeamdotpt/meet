import { auth } from '@/auth';
import { knock } from '@/lib/waitingRoom';
import { NextRequest, NextResponse } from 'next/server';

interface RouteParams {
  params: Promise<{ roomName: string }>;
}

export async function POST(request: NextRequest, { params }: RouteParams) {
  try {
    const session = await auth();
    if (!session?.user?.email) {
      return new NextResponse('Unauthorized', { status: 401 });
    }

    const { roomName } = await params;
    const body = await request.json().catch(() => ({}));
    const name = typeof body?.name === 'string' ? body.name : session.user.name ?? session.user.email;
    const identity = session.user.email;

    const guest = knock(roomName, identity, name, session.user.email);
    return NextResponse.json(guest);
  } catch (error) {
    if (error instanceof Error) {
      return new NextResponse(error.message, { status: 500 });
    }
    return new NextResponse('Internal Server Error', { status: 500 });
  }
}
