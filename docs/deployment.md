---
title: Deployment and Releases
description: Container builds, CI/CD, and calver releases for LiveKit Meet.
category: reference
order: 5
nav_order: 5
---

# Deployment and Releases

## Container image

The `Dockerfile` builds a standalone Next.js image:

```sh
docker build --build-arg NEXT_PUBLIC_SHOW_SETTINGS_MENU=true -t livekit-meet:ci .
```

`NEXT_PUBLIC_SHOW_SETTINGS_MENU` is the only build-time argument. Set it to `true` to include the in-call settings and recording tab.

## GitHub Actions CI

`.github/workflows/ci.yml` runs on every push and pull request to `main`:

1. **lint-and-unit** — lints, checks formatting, and runs Vitest unit/API tests.
2. **build-plugin** — builds the Bulwark Mail plugin.
3. **build-container** — builds the Docker image and uploads it as an artifact.
4. **playwright** — downloads the image, starts the full stack, runs Playwright E2E tests, and uploads the report.

## Release workflow

`.github/workflows/release.yml` is triggered by tags matching `v*`. It builds and pushes a multi-arch container image to GHCR with these tags:

- The exact release tag (e.g. `v2026.08.1`)
- The floating month tag (`v2026.08`)
- The floating year tag (`v2026`)
- `latest`

## Calendar versioning

This project uses [Calendar Versioning](https://calver.org): `YYYY.0M.PATCH`. The patch resets to `0` at the start of each month. Tags are prefixed with `v`, for example `v2026.08.1`.

To cut a release:

1. Update `CHANGELOG.md` with a new section.
2. Bump `version` in `package.json` (e.g. `2026.8.1`).
3. Commit the release changes.
4. Create and push an annotated tag:

   ```sh
   git tag -a v2026.08.1 -m "Release v2026.08.1"
   git push origin v2026.08.1
   ```

The tag push triggers the release workflow.

## Deploying to meet.sunbeam.pt

Set the runtime environment for the deployed container:

```env
LIVEKIT_URL=wss://livekit.internal
LIVEKIT_API_KEY=...
LIVEKIT_API_SECRET=...
NEXT_PUBLIC_LIVEKIT_URL=wss://livekit.sunbeam.pt
E2EE_SECRET=...
AUTH_SECRET=...
AUTH_URL=https://meet.sunbeam.pt
AUTH_TRUST_HOST=true
DATABASE_URL=postgres://...
MEET_BASE_URL=https://meet.sunbeam.pt
OIDC_ISSUER=https://sso.sunbeam.pt
OIDC_CLIENT_ID=...
OIDC_CLIENT_SECRET=...
NEXT_PUBLIC_SHOW_SETTINGS_MENU=true
BULWARK_WEBHOOK_URL=https://bulwark.sunbeam.pt/webhooks/livekit
BULWARK_WEBHOOK_SECRET=...
```

Make sure `NEXT_PUBLIC_LIVEKIT_URL` is resolvable by browsers, while `LIVEKIT_URL` is resolvable by the app container for server SDK calls.
