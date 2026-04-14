-- Reactions log.
CREATE TABLE reactions (
    id               UUID PRIMARY KEY,
    room_id          UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
    sender_identity  TEXT NOT NULL,
    emoji            TEXT NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_reactions_room_created ON reactions(room_id, created_at DESC);
