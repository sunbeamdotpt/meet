-- Participants history (per-session join/leave log).
CREATE TABLE participants_history (
    id         UUID PRIMARY KEY,
    room_id    UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
    identity   TEXT NOT NULL,
    role       TEXT NOT NULL,
    joined_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    left_at    TIMESTAMPTZ,
    hidden     BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX idx_participants_room ON participants_history(room_id);
CREATE INDEX idx_participants_identity ON participants_history(identity);
