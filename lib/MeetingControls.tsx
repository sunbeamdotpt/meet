'use client';

import React from 'react';
import { useDataChannel, useRoomContext } from '@livekit/components-react';

const REACTIONS = ['👍', '❤️', '😂', '🎉', '👏', '🔥'];

interface ReactionMessage {
  emoji: string;
  from: string;
  name: string;
}

interface RaiseHandMessage {
  identity: string;
  name: string;
  raised: boolean;
}

export interface MeetingControlsProps {
  userName: string;
  onReaction?: (emoji: string) => void;
  onRaiseHand?: (raised: boolean, identity: string, name: string) => void;
}

export function MeetingControls(props: MeetingControlsProps) {
  const room = useRoomContext();
  const { send: sendReaction } = useDataChannel('reaction');
  const { send: sendRaiseHand } = useDataChannel('raise-hand');
  const [handRaised, setHandRaised] = React.useState(false);

  const localName =
    props.userName || room.localParticipant.name || room.localParticipant.identity || 'You';

  const sendReactionMessage = (emoji: string) => {
    props.onReaction?.(emoji);
    try {
      const payload: ReactionMessage = {
        emoji,
        from: room.localParticipant.identity,
        name: localName,
      };
      const encoder = new TextEncoder();
      sendReaction(encoder.encode(JSON.stringify(payload)), { reliable: true });
    } catch (error) {
      console.warn('Failed to send reaction:', error);
    }
  };

  const toggleRaiseHand = () => {
    const next = !handRaised;
    setHandRaised(next);
    props.onRaiseHand?.(next, room.localParticipant.identity, localName);
    try {
      const payload: RaiseHandMessage = {
        identity: room.localParticipant.identity,
        name: localName,
        raised: next,
      };
      const encoder = new TextEncoder();
      sendRaiseHand(encoder.encode(JSON.stringify(payload)), { reliable: true });
    } catch (error) {
      console.warn('Failed to send raise-hand:', error);
    }
  };

  return (
    <div
      data-testid="meeting-controls"
      style={{
        position: 'absolute',
        bottom: '5.5rem',
        left: '50%',
        transform: 'translateX(-50%)',
        zIndex: 10,
        display: 'flex',
        gap: '0.5rem',
        padding: '0.5rem',
        borderRadius: '0.5rem',
        backgroundColor: 'rgba(0,0,0,0.6)',
      }}
    >
      {REACTIONS.map((emoji) => (
        <button
          key={emoji}
          data-testid={`reaction-${emoji}`}
          className="lk-button"
          onClick={() => sendReactionMessage(emoji)}
          style={{ fontSize: '1.25rem', padding: '0.25rem 0.5rem' }}
        >
          {emoji}
        </button>
      ))}
      <button
        data-testid="raise-hand-button"
        className="lk-button"
        onClick={toggleRaiseHand}
        style={{
          padding: '0.25rem 0.75rem',
          backgroundColor: handRaised ? 'var(--lk-primary)' : undefined,
        }}
      >
        {handRaised ? 'Lower hand' : 'Raise hand'} ✋
      </button>
    </div>
  );
}

export interface ReactionOverlayProps {
  reactions: Array<{ id: string; emoji: string; left: number; createdAt: number }>;
}

export function ReactionOverlay({ reactions }: ReactionOverlayProps) {
  return (
    <div
      data-testid="reaction-overlay"
      style={{
        position: 'absolute',
        inset: 0,
        pointerEvents: 'none',
        overflow: 'hidden',
        zIndex: 5,
      }}
    >
      {reactions.map((r) => (
        <span
          key={r.id}
          style={{
            position: 'absolute',
            left: `${r.left}%`,
            bottom: '15%',
            fontSize: '2rem',
            animation: 'reaction-float 2s ease-out forwards',
          }}
        >
          {r.emoji}
        </span>
      ))}
    </div>
  );
}

export interface RaisedHandsOverlayProps {
  raisedHands: Array<{ identity: string; name: string; raisedAt: number }>;
}

export function RaisedHandsOverlay({ raisedHands }: RaisedHandsOverlayProps) {
  if (raisedHands.length === 0) return null;

  return (
    <div
      data-testid="raised-hands-panel"
      style={{
        position: 'absolute',
        top: '1rem',
        left: '1rem',
        zIndex: 10,
        backgroundColor: 'var(--lk-bg)',
        border: '1px solid var(--lk-border-color)',
        borderRadius: '0.5rem',
        padding: '0.75rem',
        minWidth: '180px',
        boxShadow: '0 4px 12px rgba(0,0,0,0.3)',
      }}
    >
      <h4 style={{ margin: '0 0 0.5rem 0' }}>Raised hands</h4>
      <ul style={{ listStyle: 'none', padding: 0, margin: 0 }}>
        {raisedHands.map((h) => (
          <li key={h.identity} style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
            <span>✋</span>
            <span>{h.name}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
