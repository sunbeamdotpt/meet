---
title: Testing
description: Running unit, API, and end-to-end tests for LiveKit Meet.
category: guides
order: 4
nav_order: 4
---

# Testing

## Unit and API tests

Tests use [Vitest](https://vitest.dev/). They require a Postgres database for the database and API tests.

```sh
# Start Postgres if it is not already running:
docker compose -f compose.ci.yml up postgres -d

export DATABASE_URL=postgres://livekit_meet:livekit_meet@localhost:5432/livekit_meet
pnpm test
```

## Lint and formatting

```sh
pnpm lint
pnpm format:check
```

## End-to-end tests with Playwright

E2E tests run against the full containerized stack. The fastest way to run them locally is:

```sh
# 1. Build the CI image with settings menu enabled.
docker build --build-arg NEXT_PUBLIC_SHOW_SETTINGS_MENU=true -t livekit-meet:ci .

# 2. Start LiveKit, Postgres, and the app.
docker compose -f compose.ci.yml up -d

# 3. Install Playwright browsers (one-time).
pnpm exec playwright install --with-deps chromium

# 4. Run the suite.
CI=true pnpm exec playwright test
```

The tests exercise:

- Home page signed-out and signed-in states
- `/meetings` empty state and populated meeting list
- E2EE passphrase derivation and encrypted room connection
- Waiting room admission flow
- Breakout room creation and assignment
- Reactions and raise-hand overlays
- Settings/recording UI

### Test artifacts

Feature screenshots are saved to `test-results/screenshots/` after a successful run. HTML reports and failure traces are written to `playwright-report/` and `test-results/`.

```sh
# View the HTML report.
pnpm exec playwright show-report
```

### Test-only endpoints

When `ALLOW_TEST_AUTH=true`, the app exposes `POST /api/test/seed-meeting` for Playwright to create meeting rows. This endpoint is disabled in production builds.
