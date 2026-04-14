-- LiveKit webhook events — idempotency ledger.
CREATE TABLE webhook_events (
    id                 UUID PRIMARY KEY,
    livekit_event_id   TEXT NOT NULL UNIQUE,
    type               TEXT NOT NULL,
    payload            JSONB NOT NULL,
    received_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    processed_at       TIMESTAMPTZ
);

CREATE INDEX idx_webhook_events_type ON webhook_events(type);
