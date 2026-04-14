-- Rooms.
CREATE TABLE rooms (
    id               UUID PRIMARY KEY,
    slug             TEXT NOT NULL UNIQUE,
    display_name     TEXT NOT NULL,
    access_level     TEXT NOT NULL DEFAULT 'trusted',
    status           TEXT NOT NULL DEFAULT 'waiting',
    max_participants INTEGER NOT NULL DEFAULT 300,
    default_quality  TEXT NOT NULL DEFAULT 'auto',
    waiting_room_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    chat_enabled     BOOLEAN NOT NULL DEFAULT TRUE,
    recording_allowed BOOLEAN NOT NULL DEFAULT TRUE,
    created_by       TEXT NOT NULL,
    livekit_room_name TEXT NOT NULL UNIQUE,
    metadata         JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at       TIMESTAMPTZ,
    ended_at         TIMESTAMPTZ,
    deleted_at       TIMESTAMPTZ
);

CREATE INDEX idx_rooms_status ON rooms(status) WHERE deleted_at IS NULL;
CREATE INDEX idx_rooms_created_by ON rooms(created_by) WHERE deleted_at IS NULL;
