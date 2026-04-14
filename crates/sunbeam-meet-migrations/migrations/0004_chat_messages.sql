-- Chat messages.
CREATE TABLE chat_messages (
    id               UUID PRIMARY KEY,
    room_id          UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
    sender_identity  TEXT NOT NULL,
    sender_display_name TEXT NOT NULL DEFAULT '',
    body             TEXT NOT NULL,
    reply_to         UUID,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    edited_at        TIMESTAMPTZ,
    deleted_at       TIMESTAMPTZ
);

CREATE INDEX idx_chat_room_created ON chat_messages(room_id, created_at DESC) WHERE deleted_at IS NULL;
