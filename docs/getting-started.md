---
title: Getting Started
description: Quick start guide for running the Sunbeam LiveKit Meet app locally or in Docker.
category: guides
order: 1
nav_order: 1
---

# Getting Started

Sunbeam LiveKit Meet is a Next.js video-conferencing frontend backed by a self-hosted [LiveKit](https://livekit.io/) server. It provides calendar-integrated meetings, waiting rooms, breakout rooms, reactions, raise-hand, and opt-in egress recording.

## Prerequisites

- [Node.js](https://nodejs.org/) 22+ and [pnpm](https://pnpm.io/) 10.18.2
- A running LiveKit server (local Docker or LiveKit Cloud)
- PostgreSQL 16+ for meeting metadata
- An OIDC provider for authentication (e.g. [sso-gateway](../sso-gateway/README.md))

## Quick start

1. Clone the repository and install dependencies:

   ```sh
   pnpm install --frozen-lockfile
   ```

2. Copy the example environment file and fill in the values:

   ```sh
   cp .env.example .env
   ```

   See [Configuration](./configuration.md) for a description of every variable.

3. Start a local LiveKit server and Postgres (optional but recommended):

   ```sh
   docker compose -f compose.ci.yml up livekit postgres -d
   ```

4. Run the database migrations automatically by starting the app:

   ```sh
   pnpm dev
   ```

5. Open [http://localhost:3000](http://localhost:3000).

The `meetings` table is created lazily on first access, so no manual migration step is required for local development.

## Project layout

```
app/
  api/              # API routes (connection details, Bulwark webhooks, recordings)
  meetings/         # Upcoming meetings landing page
  rooms/[roomName]/ # Video conference room
lib/                # React components and server helpers
plugins/livekit-meet/ # Bulwark Mail plugin
migrations/         # SQL schema files
tests/              # Unit, API, and E2E tests
```

## Next steps

- [Configuration](./configuration.md)
- [Running Locally](./running-locally.md)
- [Testing](./testing.md)
- [Deployment](./deployment.md)
