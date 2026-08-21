# Video Calls

A LiveKit-based video conferencing app for calendar events, forked from [livekit-examples/meet](https://github.com/livekit-examples/meet).

## Requirements

- Node.js 18+
- pnpm
- A LiveKit project (Cloud or self-hosted)
- An OIDC provider (e.g. Zitadel, Keycloak, Okta)

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
