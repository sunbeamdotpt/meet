import { AccessToken, AccessTokenOptions, VideoGrant } from 'livekit-server-sdk';
import { MeetingRole } from './types';

const API_KEY = process.env.LIVEKIT_API_KEY;
const API_SECRET = process.env.LIVEKIT_API_SECRET;

export interface TokenPermissions {
  canPublish?: boolean;
  canPublishData?: boolean;
  canSubscribe?: boolean;
  roomAdmin?: boolean;
}

export interface TokenUserInfo extends AccessTokenOptions {
  role: MeetingRole;
  permissions?: TokenPermissions;
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
    canPublish: userInfo.permissions?.canPublish ?? isHost,
    canPublishData: userInfo.permissions?.canPublishData ?? isHost,
    canSubscribe: userInfo.permissions?.canSubscribe ?? isHost,
    roomAdmin: userInfo.permissions?.roomAdmin ?? isHost,
  };
  at.addGrant(grant);
  return await at.toJwt();
}
