import { auth } from '@/auth';
import { isAdmitted } from '@/lib/waitingRoom';
import { NextResponse } from 'next/server';

interface RouteParams {
  params: Promise<{ roomName: string }>;
}

export async function GET(_request: Request, { params }: RouteParams) {
  try {
    const session = await auth();
    if (!session?.user?.email) {
      return new NextResponse('Unauthorized', { status: 401 });
    }

    const { roomName } = await params;
    const admitted = isAdmitted(roomName, session.user.email);
    return NextResponse.json({ admitted });
  } catch (error) {
    if (error instanceof Error) {
      return new NextResponse(error.message, { status: 500 });
    }
    return new NextResponse('Internal Server Error', { status: 500 });
  }
}
