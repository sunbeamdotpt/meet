# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Calendar Versioning](https://calver.org) (`YYYY.0M.PATCH`).

## [v2026.08.4] - 2026-08-21

### Fixed

- OIDC token endpoint authentication by setting `client.token_endpoint_auth_method` to `client_secret_post`, matching the SSO gateway client's registered method. Without this, NextAuth v5 falls back to `client_secret_basic` and Hydra rejects the token exchange with `invalid_client`.

## [v2026.08.3] - 2026-08-21

### Fixed

- OIDC sign-in with Hydra by adding `checks: ['pkce', 'state']` to the OIDC provider. NextAuth v5 only sends the `state` parameter when explicitly requested, and Hydra rejects callbacks without it.

## [v2026.08.2] - 2026-08-21

### Added

- `pnpm build:zip` script in `plugins/livekit-meet/` to produce a Bulwark-compatible plugin ZIP (`manifest.json` + `index.js` at the root).
- Plugin version injection from the release tag into `manifest.json` at zip time.
- GitHub Actions release job that builds the plugin and attaches `livekit-meet-plugin-{tag}.zip` to each `v*` release.
- Plugin-specific `CHANGELOG.md` in `plugins/livekit-meet/`.

### Changed

- CI `build-plugin` job now verifies the plugin ZIP contains `manifest.json` and `index.js` at the root.

## [v2026.08.1] - 2026-08-21

### Added

- Browser-facing `NEXT_PUBLIC_LIVEKIT_URL` environment variable so the connection-details endpoint returns a host-routable LiveKit URL while the server SDK continues to use the internal `LIVEKIT_URL`.
- Test-only `POST /api/test/seed-meeting` endpoint for Playwright to seed meeting data.
- Playwright E2E coverage for the signed-out and signed-in home page, empty and populated `/meetings` page, and the settings/recording UI.
- Feature screenshots captured during E2E runs in `test-results/screenshots/`.

### Changed

- Enabled the settings menu (and recording tab) in CI container builds via `NEXT_PUBLIC_SHOW_SETTINGS_MENU`.
- Switched container CI build to pass `NEXT_PUBLIC_SHOW_SETTINGS_MENU=true` as a Docker build arg.

### Removed

- Temporary debug sign-in test and middleware logging used while stabilizing authentication.

## [v2026.08.0] - 2026-08-21

### Added

- Bulwark Mail calendar plugin (`plugins/livekit-meet`) that adds an "Add LiveKit Meeting" button to calendar event editors.
- `POST /api/bulwark/rooms` endpoint to create LiveKit rooms from calendar events, verifying the caller's OIDC Bearer token.
- Postgres-backed meeting persistence (`meetings` table) with lazy migrations.
- Login-only `/meetings` landing page listing upcoming meetings for the authenticated user.
- Forwarding of LiveKit lifecycle webhooks (`room_started`, `room_finished`, `participant_joined`, `participant_left`, `egress_started`, `egress_ended`, `track_published`, `track_unpublished`) to Bulwark with HMAC-SHA256 signatures.
- Unit tests for OIDC verification, database layer, and the Bulwark room API.
- GitHub Actions CI workflow with Postgres service, plugin build job, container build, and Playwright E2E tests.
- Multi-arch container release workflow triggered by `v*` calver tags.
