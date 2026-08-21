import { RoomServiceClient, EgressClient } from 'livekit-server-sdk';

const API_KEY = process.env.LIVEKIT_API_KEY;
const API_SECRET = process.env.LIVEKIT_API_SECRET;
const LIVEKIT_URL = process.env.LIVEKIT_URL;

export function getRoomServiceClient(): RoomServiceClient {
  if (!LIVEKIT_URL) {
    throw new Error('LIVEKIT_URL is not defined');
  }
  if (!API_KEY || !API_SECRET) {
    throw new Error('LIVEKIT_API_KEY and LIVEKIT_API_SECRET are required');
  }
  // RoomServiceClient expects an HTTP URL, e.g. https://my-project.livekit.cloud
  const url = LIVEKIT_URL.replace(/^wss:/, 'https:').replace(/^ws:/, 'http:');
  return new RoomServiceClient(url, API_KEY, API_SECRET);
}

export function getEgressClient(): EgressClient {
  if (!LIVEKIT_URL) {
    throw new Error('LIVEKIT_URL is not defined');
  }
  if (!API_KEY || !API_SECRET) {
    throw new Error('LIVEKIT_API_KEY and LIVEKIT_API_SECRET are required');
  }
  const url = LIVEKIT_URL.replace(/^wss:/, 'https:').replace(/^ws:/, 'http:');
  return new EgressClient(url, API_KEY, API_SECRET);
}
