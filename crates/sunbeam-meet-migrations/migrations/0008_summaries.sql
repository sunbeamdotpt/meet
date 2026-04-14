-- Post-meeting summaries.
CREATE TABLE summaries (
    id                  UUID PRIMARY KEY,
    room_id             UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
    transcript_ref      TEXT,
    summary_md          TEXT NOT NULL,
    action_items        JSONB NOT NULL DEFAULT '[]'::jsonb,
    meeting_duration_ms BIGINT NOT NULL DEFAULT 0,
    attendees           TEXT[] NOT NULL DEFAULT '{}',
    generated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    model               TEXT NOT NULL DEFAULT 'mistral-large-latest'
);

CREATE INDEX idx_summaries_room ON summaries(room_id);
