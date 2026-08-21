'use client';

import React from 'react';
import { useDataChannel, useRoomContext } from '@livekit/components-react';
import { RoomEvent, Participant } from 'livekit-client';

interface BreakoutRoomInput {
  label: string;
  participants: string[];
}

interface BreakoutRoomState {
  id: string;
  name: string;
  label: string;
}

interface BreakoutState {
  active: boolean;
  mainRoom: string;
  rooms: BreakoutRoomState[];
  assignments: Record<string, string>;
  createdAt: number;
}

export function BreakoutControls() {
  const room = useRoomContext();
  const { send: sendBreakout } = useDataChannel('breakout');

  const [participants, setParticipants] = React.useState<Participant[]>([]);
  const [loading, setLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [activeState, setActiveState] = React.useState<BreakoutState | null>(null);
  const [isOpen, setIsOpen] = React.useState(false);
  const [roomLabel, setRoomLabel] = React.useState('');
  const [rooms, setRooms] = React.useState<BreakoutRoomInput[]>([]);

  const fetchParticipants = React.useCallback(async () => {
    try {
      const response = await fetch(`/api/rooms/${encodeURIComponent(room.name)}/participants`);
      if (!response.ok) throw new Error(await response.text());
      const data = (await response.json()) as Array<{ identity: string; name: string }>;
      setParticipants(
        data.map((p) => {
          const existing = room.remoteParticipants.get(p.identity);
          return existing ?? ({ identity: p.identity, name: p.name } as unknown as Participant);
        }),
      );
    } catch (e) {
      console.error('Failed to load participants:', e);
    }
  }, [room]);

  const fetchState = React.useCallback(async () => {
    try {
      const response = await fetch(`/api/rooms/${encodeURIComponent(room.name)}/breakouts`);
      if (!response.ok) throw new Error(await response.text());
      const data = (await response.json()) as { state: BreakoutState | null };
      setActiveState(data.state?.active ? data.state : null);
    } catch (e) {
      console.error('Failed to load breakout state:', e);
    }
  }, [room.name]);

  React.useEffect(() => {
    if (!room.name) return;
    const onUpdate = () => {
      fetchParticipants();
      fetchState();
    };
    onUpdate();
    room.on(RoomEvent.ParticipantConnected, onUpdate);
    room.on(RoomEvent.ParticipantDisconnected, onUpdate);
    return () => {
      room.off(RoomEvent.ParticipantConnected, onUpdate);
      room.off(RoomEvent.ParticipantDisconnected, onUpdate);
    };
  }, [room, room.name, fetchParticipants, fetchState]);

  const addRoom = () => {
    const label = roomLabel.trim() || `Room ${rooms.length + 1}`;
    setRooms((prev) => [...prev, { label, participants: [] }]);
    setRoomLabel('');
  };

  const removeRoom = (index: number) => {
    setRooms((prev) => prev.filter((_, i) => i !== index));
  };

  const assignParticipant = (identity: string, roomIndex: number) => {
    setRooms((prev) =>
      prev.map((r, i) => {
        if (i !== roomIndex) {
          return { ...r, participants: r.participants.filter((id) => id !== identity) };
        }
        if (r.participants.includes(identity)) return r;
        return { ...r, participants: [...r.participants, identity] };
      }),
    );
  };

  const sendAssignment = (identity: string, roomName: string, label: string) => {
    const payload = JSON.stringify({ type: 'breakout-assignment', roomName, label });
    sendBreakout(new TextEncoder().encode(payload), {
      reliable: true,
      destinationIdentities: [identity],
    });
  };

  const openBreakouts = async () => {
    if (rooms.length === 0) return;
    setLoading(true);
    setError(null);
    try {
      const body = {
        role: 'host',
        rooms: rooms.map((r) => ({ label: r.label, participants: r.participants })),
      };
      const response = await fetch(`/api/rooms/${encodeURIComponent(room.name)}/breakouts`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
      if (!response.ok) throw new Error(await response.text());
      const data = (await response.json()) as { state: BreakoutState };
      setActiveState(data.state);
      setRooms([]);

      for (const [identity, roomName] of Object.entries(data.state.assignments)) {
        const label = data.state.rooms.find((r) => r.name === roomName)?.label ?? 'Breakout';
        sendAssignment(identity, roomName, label);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to open breakouts');
    } finally {
      setLoading(false);
    }
  };

  const closeBreakouts = async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await fetch(
        `/api/rooms/${encodeURIComponent(room.name)}/breakouts?role=host`,
        { method: 'DELETE' },
      );
      if (!response.ok) throw new Error(await response.text());
      setActiveState(null);

      const payload = JSON.stringify({ type: 'breakout-close' });
      sendBreakout(new TextEncoder().encode(payload), { reliable: true });
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to close breakouts');
    } finally {
      setLoading(false);
    }
  };

  const participantIdentities = participants.map((p) => p.identity);

  return (
    <div
      data-testid="breakout-controls"
      style={{
        position: 'absolute',
        top: '1rem',
        left: '1rem',
        zIndex: 10,
        backgroundColor: 'var(--lk-bg)',
        border: '1px solid var(--lk-border-color)',
        borderRadius: '0.5rem',
        padding: '1rem',
        minWidth: '260px',
        maxWidth: '360px',
        boxShadow: '0 4px 12px rgba(0,0,0,0.3)',
      }}
    >
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          marginBottom: '0.75rem',
        }}
      >
        <h4 style={{ margin: 0 }}>Breakout rooms</h4>
        <button
          data-testid="breakout-toggle"
          className="lk-button"
          onClick={() => setIsOpen((v) => !v)}
        >
          {isOpen ? 'Close' : activeState ? 'Manage' : 'Create'}
        </button>
      </div>

      {activeState && (
        <div style={{ marginBottom: '0.75rem' }}>
          <strong>{activeState.rooms.length}</strong> active breakout room
          {activeState.rooms.length === 1 ? '' : 's'}
          <button
            data-testid="breakout-close-all"
            className="lk-button"
            onClick={closeBreakouts}
            disabled={loading}
            style={{ marginLeft: '0.75rem' }}
          >
            Close all
          </button>
        </div>
      )}

      {isOpen && !activeState && (
        <div>
          <div style={{ display: 'flex', gap: '0.5rem', marginBottom: '0.75rem' }}>
            <input
              data-testid="breakout-room-label"
              type="text"
              value={roomLabel}
              onChange={(e) => setRoomLabel(e.target.value)}
              placeholder="Room label"
              className="lk-form-control"
              style={{ flex: 1 }}
              onKeyDown={(e) => e.key === 'Enter' && addRoom()}
            />
            <button data-testid="breakout-add-room" className="lk-button" onClick={addRoom}>
              Add room
            </button>
          </div>

          {rooms.map((r, idx) => (
            <div
              key={idx}
              style={{
                marginBottom: '0.75rem',
                padding: '0.5rem',
                border: '1px solid var(--lk-border-color)',
                borderRadius: '0.25rem',
              }}
            >
              <div
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'center',
                  marginBottom: '0.5rem',
                }}
              >
                <strong>{r.label}</strong>
                <button className="lk-button" onClick={() => removeRoom(idx)}>
                  Remove
                </button>
              </div>
              <select
                multiple
                value={r.participants}
                onChange={(e) => {
                  const selected = Array.from(e.target.selectedOptions).map((o) => o.value);
                  setRooms((prev) =>
                    prev.map((roomInput, i) => {
                      if (i !== idx) {
                        return {
                          ...roomInput,
                          participants: roomInput.participants.filter(
                            (id) => !selected.includes(id),
                          ),
                        };
                      }
                      return { ...roomInput, participants: selected };
                    }),
                  );
                }}
                className="lk-form-control"
                style={{ width: '100%', minHeight: '80px' }}
              >
                {participantIdentities.map((identity) => (
                  <option key={identity} value={identity}>
                    {participants.find((p) => p.identity === identity)?.name || identity}
                  </option>
                ))}
              </select>
            </div>
          ))}

          <button
            data-testid="breakout-open"
            className="lk-button"
            onClick={openBreakouts}
            disabled={loading || rooms.length === 0}
            style={{ width: '100%' }}
          >
            {loading ? 'Opening...' : 'Open breakouts'}
          </button>
        </div>
      )}

      {error && <p style={{ color: 'var(--lk-danger2)', marginTop: '0.5rem' }}>{error}</p>}
    </div>
  );
}
