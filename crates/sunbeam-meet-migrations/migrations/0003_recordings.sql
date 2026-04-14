-- Recordings.
CREATE TABLE recordings (
    id           UUID PRIMARY KEY,
    room_id      UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
    egress_id    TEXT NOT NULL DEFAULT '',
    mode         TEXT NOT NULL,
    output       TEXT NOT NULL DEFAULT 'file',
    status       TEXT NOT NULL DEFAULT 'starting',
    started_by   TEXT NOT NULL,
    started_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ended_at     TIMESTAMPTZ,
    storage_url  TEXT,
    rtmp_url     TEXT,
    duration_ms  BIGINT NOT NULL DEFAULT 0,
    size_bytes   BIGINT NOT NULL DEFAULT 0,
    error        TEXT,
    deleted_at   TIMESTAMPTZ
);

CREATE INDEX idx_recordings_room ON recordings(room_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_recordings_status ON recordings(status) WHERE deleted_at IS NULL;
