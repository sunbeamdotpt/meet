# Video Calls

A LiveKit-based video conferencing app for calendar events, forked from [livekit-examples/meet](https://github.com/livekit-examples/meet).

## Requirements

- Node.js 18+
- pnpm
- A LiveKit project (Cloud or self-hosted)
- An OIDC provider (e.g. Zitadel, Keycloak, Okta)
- S3-compatible storage if you use the recording feature

## Setup

1. Install dependencies:
   ```bash
   pnpm install
   ```

2. Copy `.env.example` to `.env.local` and fill in the values:
   ```bash
   cp .env.example .env.local
   ```

3. Start the development server:
   ```bash
   pnpm dev
   ```

## Usage

Users authenticate via OIDC. Calendar events link directly to `/rooms/{roomName}`; authenticated users can join the room from that URL.

### Roles and waiting room

Append `?role=host` to the room URL for organizers. Guests join with `?role=guest` (or omit the parameter). Guests receive restricted tokens and see a waiting screen until a host admits them from the in-meeting host panel.

### Recording

Hosts can start and stop room-composite recordings from the settings menu. Recordings are written to the configured S3 bucket via LiveKit Egress.

### Room management API

The following API routes are available for server-side integrations (e.g. Bulwark creating rooms for calendar events):

- `GET /api/rooms` — list active rooms
- `POST /api/rooms` — create a room
- `GET /api/rooms/{roomName}` — get room details
- `DELETE /api/rooms/{roomName}` — delete a room
- `GET /api/rooms/{roomName}/participants` — list participants
- `DELETE /api/rooms/{roomName}/participants/{identity}` — remove a participant
- `POST /api/rooms/{roomName}/participants/{identity}/permissions` — update/admit a participant
- `POST /api/rooms/{roomName}/record/start` — start recording
- `POST /api/rooms/{roomName}/record/stop` — stop recording

### Webhooks

Configure your LiveKit project to send webhooks to `/api/webhooks/livekit`. The endpoint verifies signatures and logs events.

## Environment Variables

| Variable | Description |
|---|---|
| `LIVEKIT_API_KEY` | LiveKit API key |
| `LIVEKIT_API_SECRET` | LiveKit API secret |
| `LIVEKIT_URL` | LiveKit server URL, e.g. `wss://my-project.livekit.cloud` |
| `OIDC_ISSUER` | OIDC issuer URL |
| `OIDC_CLIENT_ID` | OIDC client ID |
| `OIDC_CLIENT_SECRET` | OIDC client secret |
| `AUTH_SECRET` | Random secret for NextAuth session cookies |
| `S3_KEY_ID` | S3 access key ID for egress recordings |
| `S3_KEY_SECRET` | S3 secret access key for egress recordings |
| `S3_ENDPOINT` | S3 endpoint URL (omit for AWS) |
| `S3_BUCKET` | S3 bucket for egress recordings |
| `S3_REGION` | S3 region for egress recordings |
