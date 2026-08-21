# Video Calls

A LiveKit-based video conferencing app for calendar events, forked from [livekit-examples/meet](https://github.com/livekit-examples/meet).

## Quick start

```bash
pnpm install --frozen-lockfile
cp .env.example .env
# Edit .env with your LiveKit, OIDC, and Postgres credentials.
pnpm dev
```

Open [http://localhost:3000](http://localhost:3000).

## Documentation

Detailed documentation is available in `docs/` and served by the Sunbeam docs portal:

- [Getting Started](./docs/getting-started.md)
- [Configuration](./docs/configuration.md)
- [Running Locally](./docs/running-locally.md)
- [Testing](./docs/testing.md)
- [Deployment and Releases](./docs/deployment.md)
- [OIDC Setup](./docs/oidc.md)
- [Bulwark Plugin](./docs/bulwark-plugin.md)
- [Features](./docs/features.md)
- [License and Attribution](./docs/license.md)

## High-level overview

Users authenticate via OIDC. Calendar events link directly to `/rooms/{roomName}`; authenticated users join from that URL. Organizers use `?role=host`, guests use `?role=guest` and wait in a waiting room until admitted.

The Bulwark Mail plugin in `plugins/livekit-meet/` adds an **"Add LiveKit Meeting"** button to calendar events and creates rooms via `POST /api/bulwark/rooms`.

## License

Copyright (C) 2026 Sunbeam, Lda.

This project is licensed under the [GNU Affero General Public License v3.0 or later](LICENSE).

This project is a fork of [livekit-examples/meet](https://github.com/livekit-examples/meet), which was originally licensed under the [Apache License 2.0](NOTICE). The original Apache 2.0 license text and attribution are preserved in the [NOTICE](NOTICE) file.
