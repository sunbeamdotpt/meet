# Changelog

All notable changes to the Bulwark LiveKit Meet plugin will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Calendar Versioning](https://calver.org) (`YYYY.0M.PATCH`).

## [v2026.08.3] - 2026-08-21

### Added

- Plugin settings schema with configurable `buttonLabel`, `autoSetLocation`, and `apiPath` values.
- Author attribution changed to **Sunbeam Studios**.

### Fixed

- Read `eventData.start`/`eventData.end` from the Bulwark `calendar-event-actions` slot contract instead of the non-existent `startTime`/`endTime` keys.
- Unwrap the host's `{ ok, status, data }` response envelope when reading the created meeting URL.

## [v2026.08.2] - 2026-08-21

### Added

- Automated ZIP packaging via `pnpm build:zip`.
- Release version is injected into `manifest.json` at zip time.
- Plugin released as `livekit-meet-plugin-{tag}.zip` attached to GitHub releases.

## [v2026.08.1] - 2026-08-21

### Added

- Initial Bulwark Mail plugin that adds an **"Add LiveKit Meeting"** button to calendar event editors.
- Calls `POST /api/bulwark/rooms` on the livekit-meet app with event title, UID, and start/end times.
- Sets the event virtual location to the organizer meeting URL returned by the server.

## [v2026.08.0] - 2026-08-21

### Added

- Plugin scaffold with `calendar-event-actions` slot and `ui:calendar-action` permission.
