import { WebhookReceiver } from 'livekit-server-sdk';
import { NextRequest, NextResponse } from 'next/server';
import { forwardToBulwark, BulwarkWebhookPayload } from '@/lib/bulwark-webhook';

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

    const forwardedEvents = [
      'room_started',
      'room_finished',
      'participant_joined',
      'participant_left',
      'egress_started',
      'egress_ended',
      'track_published',
      'track_unpublished',
    ];

    if (forwardedEvents.includes(event.event)) {
      const payload: BulwarkWebhookPayload = {
        event: event.event,
        roomName: event.room?.name,
        roomSid: event.room?.sid,
        participantIdentity: event.participant?.identity,
        egressId: event.egressInfo?.egressId,
      };

      if (event.event === 'room_started' || event.event === 'egress_started') {
        payload.startedAt = new Date().toISOString();
      }
      if (event.event === 'room_finished' || event.event === 'egress_ended') {
        payload.endedAt = new Date().toISOString();
      }

      try {
        await forwardToBulwark(payload);
      } catch (err) {
        // Fail open: LiveKit must receive a 200. Log the error for observability.
        if (err instanceof Error) {
          console.error('Failed to forward LiveKit webhook to Bulwark:', err.message);
        }
      }
    }

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
