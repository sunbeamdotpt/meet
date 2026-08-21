import { AccessToken, AccessTokenOptions, VideoGrant } from 'livekit-server-sdk';
import { MeetingRole } from './types';

const API_KEY = process.env.LIVEKIT_API_KEY;
const API_SECRET = process.env.LIVEKIT_API_SECRET;

export interface TokenUserInfo extends AccessTokenOptions {
  role: MeetingRole;
}

export async function createParticipantToken(
  userInfo: TokenUserInfo,
  roomName: string,
): Promise<string> {
  const at = new AccessToken(API_KEY, API_SECRET, userInfo);
  at.ttl = '5m';

  const isHost = userInfo.role === 'host';
  const grant: VideoGrant = {
    room: roomName,
    roomJoin: true,
    canPublish: isHost,
    canPublishData: isHost,
    canSubscribe: isHost,
    roomAdmin: isHost,
  };
  at.addGrant(grant);
  return await at.toJwt();
}
