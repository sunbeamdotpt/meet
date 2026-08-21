// Server-side waiting-room state.
// NOTE: This is an in-memory store suitable for the current single-instance
// deployment. For horizontal scaling, replace this with Redis or another
// shared store.

export interface WaitingGuest {
  identity: string;
  name: string;
  email: string;
  knockedAt: number;
  admitted: boolean;
}

const GLOBAL_KEY = '__livekit_meet_waiting_rooms__';

function getWaitingRooms(): Map<string, Map<string, WaitingGuest>> {
  if (!(globalThis as any)[GLOBAL_KEY]) {
    (globalThis as any)[GLOBAL_KEY] = new Map<string, Map<string, WaitingGuest>>();
  }
  return (globalThis as any)[GLOBAL_KEY];
}

function getRoom(roomName: string): Map<string, WaitingGuest> {
  const waitingRooms = getWaitingRooms();
  let room = waitingRooms.get(roomName);
  if (!room) {
    room = new Map();
    waitingRooms.set(roomName, room);
  }
  return room;
}

export function knock(
  roomName: string,
  identity: string,
  name: string,
  email: string,
): WaitingGuest {
  const room = getRoom(roomName);
  const guest: WaitingGuest = {
    identity,
    name,
    email,
    knockedAt: Date.now(),
    admitted: false,
  };
  room.set(identity, guest);
  return guest;
}

export function getWaitingGuests(roomName: string): WaitingGuest[] {
  const room = getWaitingRooms().get(roomName);
  return room
    ? Array.from(room.values())
        .filter((g) => !g.admitted)
        .sort((a, b) => a.knockedAt - b.knockedAt)
    : [];
}

export function admitGuest(roomName: string, identity: string): WaitingGuest | undefined {
  const room = getWaitingRooms().get(roomName);
  if (!room) return undefined;
  const guest = room.get(identity);
  if (!guest) return undefined;
  guest.admitted = true;
  return guest;
}

export function isAdmitted(roomName: string, identity: string): boolean {
  const room = getWaitingRooms().get(roomName);
  if (!room) return false;
  const guest = room.get(identity);
  return guest?.admitted ?? false;
}

export function clearAdmittedGuest(roomName: string, identity: string): void {
  const room = getWaitingRooms().get(roomName);
  if (!room) return;
  room.delete(identity);
}
