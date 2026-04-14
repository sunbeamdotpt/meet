-- Captioning state per room (singleton row per room).
CREATE TABLE captioning_state (
    room_id       UUID PRIMARY KEY REFERENCES rooms(id) ON DELETE CASCADE,
    state         TEXT NOT NULL DEFAULT 'stopped',
    language      TEXT NOT NULL DEFAULT '',
    job_id        TEXT NOT NULL DEFAULT '',
    worker_name   TEXT NOT NULL DEFAULT '',
    error_message TEXT NOT NULL DEFAULT '',
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
