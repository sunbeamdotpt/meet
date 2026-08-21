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

## Installation

The plugin is built as a separate artifact in CI. For local development:

```sh
cd plugins/livekit-meet
pnpm install --frozen-lockfile
pnpm build
```

The built artifact is uploaded by the `build-plugin` CI job.

## Statelessness

The plugin itself is stateless; all persistent state lives in the LiveKit server (rooms, participants) and Postgres (meeting metadata). The LiveKit Meet deployment only needs access to those two data stores and the OIDC issuer.
