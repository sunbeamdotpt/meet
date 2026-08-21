'use client';

import React from 'react';
import { useRoomContext } from '@livekit/components-react';
import { Participant, RoomEvent } from 'livekit-client';

function isWaiting(participant: Participant): boolean {
  const p = participant.permissions;
  if (!p) return false;
  return !p.canSubscribe && !p.canPublish && !p.canPublishData;
}

export function HostControls() {
  const room = useRoomContext();
  const [, forceUpdate] = React.useReducer((x) => x + 1, 0);

  React.useEffect(() => {
    const onUpdate = () => forceUpdate();
    room.on(RoomEvent.ParticipantConnected, onUpdate);
    room.on(RoomEvent.ParticipantDisconnected, onUpdate);
    room.on(RoomEvent.ParticipantPermissionsChanged, onUpdate);
    return () => {
      room.off(RoomEvent.ParticipantConnected, onUpdate);
      room.off(RoomEvent.ParticipantDisconnected, onUpdate);
      room.off(RoomEvent.ParticipantPermissionsChanged, onUpdate);
    };
  }, [room]);

  const waitingParticipants = Array.from(room.remoteParticipants.values()).filter(isWaiting);

  const admit = async (identity: string) => {
    try {
      const response = await fetch(
        `/api/rooms/${encodeURIComponent(room.name)}/participants/${encodeURIComponent(identity)}/permissions`,
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            canSubscribe: true,
            canPublish: true,
            canPublishData: true,
            roomAdmin: false,
          }),
        },
      );
      if (!response.ok) {
        const message = await response.text();
        throw new Error(message);
      }
    } catch (error) {
      console.error('Failed to admit participant:', error);
      alert(error instanceof Error ? error.message : 'Failed to admit participant');
    }
  };

  if (waitingParticipants.length === 0) {
    return null;
  }

  return (
    <div
      style={{
        position: 'absolute',
        top: '1rem',
        right: '1rem',
        zIndex: 10,
        backgroundColor: 'var(--lk-bg)',
        border: '1px solid var(--lk-border-color)',
        borderRadius: '0.5rem',
        padding: '1rem',
        minWidth: '240px',
        boxShadow: '0 4px 12px rgba(0,0,0,0.3)',
      }}
    >
      <h4 style={{ marginTop: 0, marginBottom: '0.75rem' }}>Waiting to join</h4>
      <ul style={{ listStyle: 'none', padding: 0, margin: 0 }}>
        {waitingParticipants.map((p) => (
          <li
            key={p.identity}
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              gap: '0.5rem',
              marginBottom: '0.5rem',
            }}
          >
            <span>{p.name || p.identity}</span>
            <button className="lk-button" onClick={() => admit(p.identity)}>
              Admit
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
