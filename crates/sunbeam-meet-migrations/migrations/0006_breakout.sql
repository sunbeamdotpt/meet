-- Breakout rooms.
CREATE TABLE breakout_rooms (
    id               UUID PRIMARY KEY,
    parent_room_id   UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
    name             TEXT NOT NULL,
    livekit_room_name TEXT NOT NULL UNIQUE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ended_at         TIMESTAMPTZ
);

CREATE INDEX idx_breakout_parent ON breakout_rooms(parent_room_id);

CREATE TABLE breakout_assignments (
    breakout_room_id UUID NOT NULL REFERENCES breakout_rooms(id) ON DELETE CASCADE,
    participant_identity TEXT NOT NULL,
    assigned_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (breakout_room_id, participant_identity)
);
