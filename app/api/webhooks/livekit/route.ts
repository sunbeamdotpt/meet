import { WebhookReceiver } from 'livekit-server-sdk';
import { NextRequest, NextResponse } from 'next/server';

function getReceiver(): WebhookReceiver {
  const API_KEY = process.env.LIVEKIT_API_KEY;
  const API_SECRET = process.env.LIVEKIT_API_SECRET;
  if (!API_KEY || !API_SECRET) {
    throw new Error('LIVEKIT_API_KEY and LIVEKIT_API_SECRET are required');
  }
  return new WebhookReceiver(API_KEY, API_SECRET);
}

export async function POST(request: NextRequest) {
  try {
    const receiver = getReceiver();
    const body = await request.text();
    const authorization = request.headers.get('Authorization') ?? '';
    const event = await receiver.receive(body, authorization);

    // TODO: forward events to Bulwark backend or store them for "meeting in progress" indicators.
    console.log('livekit webhook event', {
      event: event.event,
      roomName: event.room?.name,
      roomSid: event.room?.sid,
      participantIdentity: event.participant?.identity,
    });

    return NextResponse.json({ received: true });
  } catch (error) {
    if (error instanceof Error) {
      console.error('livekit webhook error', error);
      return new NextResponse(error.message, { status: 400 });
    }
    return new NextResponse('Internal Server Error', { status: 500 });
  }
}
