-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE IF NOT EXISTS meetings (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  room_name TEXT NOT NULL UNIQUE,
  event_title TEXT NOT NULL,
  event_uid TEXT UNIQUE,
  organizer_email TEXT NOT NULL,
  organizer_name TEXT,
  start_time TIMESTAMPTZ,
  end_time TIMESTAMPTZ,
  url TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_meetings_organizer ON meetings(organizer_email);
CREATE INDEX IF NOT EXISTS idx_meetings_start_time ON meetings(start_time);
