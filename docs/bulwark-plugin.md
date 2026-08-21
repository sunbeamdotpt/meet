---
title: Bulwark Plugin
description: How the Bulwark Mail plugin creates LiveKit meetings from calendar events.
category: reference
order: 7
nav_order: 7
---

# Bulwark Plugin

The Bulwark plugin lives in `plugins/livekit-meet/` and adds an **"Add LiveKit Meeting"** button to calendar event editors in Bulwark Mail.

## What it does

When a calendar event is saved, the plugin calls `POST /api/bulwark/rooms` on the LiveKit Meet deployment. The endpoint:

1. Verifies the caller's OIDC Bearer token.
2. Generates a stable room name from the event UID (or a slug from the event title).
3. Creates the room in LiveKit via the server SDK.
4. Persists the meeting in Postgres.
5. Returns a host link such as `https://meet.sunbeam.pt/rooms/<roomName>?role=host`.

The event organizer is always granted host permissions.

## Authentication

The plugin must present a valid OIDC access token in the `Authorization: Bearer <token>` header. The token is verified against the configured OIDC issuer using standard OIDC discovery.

See [OIDC Setup](./oidc.md) for details.

## Webhooks

LiveKit lifecycle webhooks are forwarded to Bulwark when `BULWARK_WEBHOOK_URL` and `BULWARK_WEBHOOK_SECRET` are configured. Events include:

- `room_started`
- `room_finished`
- `participant_joined`
- `participant_left`
- `egress_started`
- `egress_ended`
- `track_published`
- `track_unpublished`

Payloads are signed with HMAC-SHA256 using `BULWARK_WEBHOOK_SECRET`.

## Packaging

Bulwark plugins ship as a ZIP bundle containing `manifest.json` and `index.js` at the root. The `build:zip` script produces this bundle:

```sh
cd plugins/livekit-meet
pnpm install --frozen-lockfile
pnpm build:zip v2026.08.1
```

The output is `livekit-meet-plugin-v2026.08.1.zip` with:

```
manifest.json
index.js
```

## Installation

1. Download the plugin ZIP from the GitHub release matching the app version.
2. In Bulwark Mail, go to **Admin → Plugins** and upload the ZIP.
3. Enable the plugin.

The CI `build-plugin` job verifies the ZIP layout on every push and pull request, and the release workflow attaches the plugin ZIP to each `v*` GitHub release.

## Versioning

The plugin version tracks the app version using the same [Calendar Versioning](https://calver.org) scheme (`YYYY.0M.PATCH`). The release workflow injects the release tag version into `manifest.json` before zipping, so the uploaded plugin always matches the release.

## Statelessness

The plugin itself is stateless; all persistent state lives in the LiveKit server (rooms, participants) and Postgres (meeting metadata). The LiveKit Meet deployment only needs access to those two data stores and the OIDC issuer.
