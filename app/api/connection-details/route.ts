import { randomString } from '@/lib/client-utils';
import { getLiveKitURL } from '@/lib/getLiveKitURL';
import { ConnectionDetails, MeetingRole } from '@/lib/types';
import { auth } from '@/auth';
import { createParticipantToken } from '@/lib/token';
import { deriveE2EEPassphrase } from '@/lib/e2ee';
import { isAdmitted } from '@/lib/waitingRoom';
import { NextRequest, NextResponse } from 'next/server';

const LIVEKIT_URL = process.env.LIVEKIT_URL;

const COOKIE_KEY = 'random-participant-postfix';

export async function GET(request: NextRequest) {
  try {
    const session = await auth();
    if (!session?.user) {
      return new NextResponse('Unauthorized', { status: 401 });
    }

    const roomName = request.nextUrl.searchParams.get('roomName');
    const region = request.nextUrl.searchParams.get('region');
    const roleParam = request.nextUrl.searchParams.get('role');
    const role: MeetingRole = roleParam === 'host' ? 'host' : 'guest';

    if (!LIVEKIT_URL) {
      throw new Error('LIVEKIT_URL is not defined');
    }
    const livekitServerUrl = region ? getLiveKitURL(LIVEKIT_URL, region) : LIVEKIT_URL;
    if (livekitServerUrl === undefined) {
      throw new Error('Invalid region');
    }

    if (typeof roomName !== 'string') {
      return new NextResponse('Missing required query parameter: roomName', { status: 400 });
    }

    const participantName = session.user.name ?? session.user.email ?? 'Guest';
    const stableIdentity = session.user.email ?? session.user.id ?? randomString(8);

    let randomParticipantPostfix = request.cookies.get(COOKIE_KEY)?.value;
    if (!randomParticipantPostfix) {
      randomParticipantPostfix = randomString(4);
    }

    const admitted = role === 'guest' ? isAdmitted(roomName, session.user.email ?? '') : true;
    const isHost = role === 'host';
    const permissions = admitted && !isHost
      ? { canPublish: true, canPublishData: true, canSubscribe: true, roomAdmin: false }
      : undefined;

    const participantToken = await createParticipantToken(
      {
        identity: `${stableIdentity}__${randomParticipantPostfix}`,
        name: participantName,
        metadata: JSON.stringify({ email: session.user.email, role }),
        role,
        permissions,
      },
      roomName,
    );

    const data: ConnectionDetails = {
      serverUrl: livekitServerUrl,
      roomName: roomName,
      participantToken: participantToken,
      participantName: participantName,
      e2eePassphrase: deriveE2EEPassphrase(roomName),
    };
    return new NextResponse(JSON.stringify(data), {
      headers: {
        'Content-Type': 'application/json',
        'Set-Cookie': `${COOKIE_KEY}=${randomParticipantPostfix}; Path=/; HttpOnly; SameSite=Strict; Secure; Expires=${getCookieExpirationTime()}`,
      },
    });
  } catch (error) {
    if (error instanceof Error) {
      return new NextResponse(error.message, { status: 500 });
    }
    return new NextResponse('Internal Server Error', { status: 500 });
  }
}

function getCookieExpirationTime(): string {
  const now = new Date();
  const time = now.getTime();
  const expireTime = time + 60 * 120 * 1000;
  now.setTime(expireTime);
  return now.toUTCString();
}
