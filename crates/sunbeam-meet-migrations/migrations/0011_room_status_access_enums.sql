-- Promote room status and access level from free-form TEXT columns to
-- Postgres ENUM types. Keeps the stored strings identical so existing rows
-- migrate with a trivial USING cast.
--
-- Critical-review #4/#5 follow-up: forces the service layer to model the
-- domain with typed enums and catches bad writes at the DB boundary.

CREATE TYPE room_status AS ENUM ('waiting', 'active', 'ended');
CREATE TYPE room_access_level AS ENUM ('public', 'trusted', 'restricted');

-- rooms.status: drop the TEXT default, cast, re-apply a typed default.
ALTER TABLE rooms ALTER COLUMN status DROP DEFAULT;
ALTER TABLE rooms
    ALTER COLUMN status TYPE room_status
    USING status::room_status;
ALTER TABLE rooms ALTER COLUMN status SET DEFAULT 'waiting'::room_status;

-- rooms.access_level.
ALTER TABLE rooms ALTER COLUMN access_level DROP DEFAULT;
ALTER TABLE rooms
    ALTER COLUMN access_level TYPE room_access_level
    USING access_level::room_access_level;
ALTER TABLE rooms ALTER COLUMN access_level SET DEFAULT 'trusted'::room_access_level;

-- schedules.access_level shares the same enum type.
ALTER TABLE schedules ALTER COLUMN access_level DROP DEFAULT;
ALTER TABLE schedules
    ALTER COLUMN access_level TYPE room_access_level
    USING access_level::room_access_level;
ALTER TABLE schedules ALTER COLUMN access_level SET DEFAULT 'trusted'::room_access_level;
