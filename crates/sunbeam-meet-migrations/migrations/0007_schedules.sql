-- Scheduled meetings (CalDAV-backed).
CREATE TABLE schedules (
    id               UUID PRIMARY KEY,
    owner_identity   TEXT NOT NULL,
    title            TEXT NOT NULL,
    description      TEXT NOT NULL DEFAULT '',
    access_level     TEXT NOT NULL DEFAULT 'trusted',
    default_quality  TEXT NOT NULL DEFAULT 'auto',
    starts_at        TIMESTAMPTZ NOT NULL,
    ends_at          TIMESTAMPTZ NOT NULL,
    recurrence       TEXT NOT NULL DEFAULT 'none',
    recurrence_rule  TEXT NOT NULL DEFAULT '',
    caldav_uid       TEXT NOT NULL UNIQUE,
    caldav_etag      TEXT NOT NULL DEFAULT '',
    room_id          UUID REFERENCES rooms(id) ON DELETE SET NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at       TIMESTAMPTZ
);

CREATE INDEX idx_schedules_owner ON schedules(owner_identity) WHERE deleted_at IS NULL;
CREATE INDEX idx_schedules_starts ON schedules(starts_at) WHERE deleted_at IS NULL;

CREATE TABLE schedule_invitees (
    schedule_id UUID NOT NULL REFERENCES schedules(id) ON DELETE CASCADE,
    identity    TEXT NOT NULL DEFAULT '',
    email       TEXT NOT NULL,
    response    TEXT NOT NULL DEFAULT 'pending',
    PRIMARY KEY (schedule_id, email)
);
