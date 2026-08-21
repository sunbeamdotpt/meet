---
title: Running Locally
description: How to run LiveKit Meet for local development and testing.
category: guides
order: 3
nav_order: 3
---

# Running Locally

## Dev server

With dependencies installed and `.env` populated:

```sh
pnpm dev
```

The dev server starts on [http://localhost:3000](http://localhost:3000).

## Using Docker Compose for dependencies

The `compose.ci.yml` file defines a LiveKit server and Postgres database that are sufficient for local development and E2E tests:

```sh
docker compose -f compose.ci.yml up livekit postgres -d
```

A minimal `.env` for this setup:

```env
LIVEKIT_API_KEY=key
LIVEKIT_API_SECRET=secret
LIVEKIT_URL=ws://localhost:7880
NEXT_PUBLIC_LIVEKIT_URL=ws://localhost:7880
E2EE_SECRET=local-e2ee-secret-change-me
AUTH_SECRET=local-auth-secret-change-me
DATABASE_URL=postgres://livekit_meet:livekit_meet@localhost:5432/livekit_meet
MEET_BASE_URL=http://localhost:3000
ALLOW_TEST_AUTH=true
AUTH_TRUST_HOST=true
AUTH_URL=http://localhost:3000
OIDC_ISSUER=
OIDC_CLIENT_ID=
OIDC_CLIENT_SECRET=
```

`OIDC_*` values are only required if you want to test real OIDC sign-in. For Playwright runs, `ALLOW_TEST_AUTH=true` is enough.

## Full stack via Docker Compose

To run the built app container locally instead of the dev server:

```sh
docker compose -f compose.ci.yml up -d
```

This builds the production image with `NEXT_PUBLIC_SHOW_SETTINGS_MENU=true`, starts LiveKit and Postgres, and exposes the app on [http://localhost:3000](http://localhost:3000).

## Notes

- The `meetings` table is created automatically on first access.
- `AUTH_TRUST_HOST=true` and `AUTH_URL=http://localhost:3000` are required when running NextAuth behind Docker or on localhost.
- `NEXT_PUBLIC_*` variables are inlined at build time; changing them requires a rebuild when using the production image.
