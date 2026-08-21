'use client';

import React from 'react';

interface WaitingGuest {
  identity: string;
  name: string;
  email: string;
  knockedAt: number;
}

export function HostControls() {
  const [guests, setGuests] = React.useState<WaitingGuest[]>([]);
  const [roomName, setRoomName] = React.useState<string | undefined>(undefined);

  React.useEffect(() => {
    setRoomName(window.location.pathname.split('/').pop());
  }, []);

  React.useEffect(() => {
    if (!roomName) return;

    let cancelled = false;
    const fetchWaiting = async () => {
      try {
        const res = await fetch(`/api/rooms/${encodeURIComponent(roomName)}/waiting`);
        if (!res.ok) return;
        const data = await res.json();
        if (!cancelled) {
          setGuests(data.guests ?? []);
        }
      } catch (error) {
        console.error('Failed to fetch waiting guests:', error);
      }
    };

    fetchWaiting();
    const interval = window.setInterval(fetchWaiting, 1500);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [roomName]);

  const admit = async (identity: string) => {
    try {
      const response = await fetch(
        `/api/rooms/${encodeURIComponent(roomName ?? '')}/waiting/${encodeURIComponent(identity)}/admit`,
        { method: 'POST' },
      );
      if (!response.ok) {
        const message = await response.text();
        throw new Error(message);
      }
      setGuests((prev) => prev.filter((g) => g.identity !== identity));
    } catch (error) {
      console.error('Failed to admit guest:', error);
      alert(error instanceof Error ? error.message : 'Failed to admit guest');
    }
  };

  const admitAll = async () => {
    await Promise.all(guests.map((g) => admit(g.identity)));
  };

  if (guests.length === 0) {
    return null;
  }

  return (
    <div
      data-testid="host-controls"
      style={{
        position: 'absolute',
        top: '1rem',
        right: '1rem',
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
        <h4 style={{ margin: 0 }}>Waiting to join</h4>
        <button className="lk-button" onClick={admitAll}>
          Admit all
        </button>
      </div>
      <ul style={{ listStyle: 'none', padding: 0, margin: 0 }}>
        {guests.map((g) => (
          <li
            key={g.identity}
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              gap: '0.5rem',
              marginBottom: '0.5rem',
            }}
          >
            <span>{g.name || g.identity}</span>
            <button className="lk-button" onClick={() => admit(g.identity)}>
              Admit
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
