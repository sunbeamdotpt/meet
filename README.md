# sunbeam-meet

Rust video conferencing service. Implements `sunbeam.meet.v1.MeetService` over
Connect + gRPC + gRPC-Web on a single port (via [`connect-rust`] + `axum`),
with LiveKit as the media backend. Replaces the legacy Django `meet/` app.

- `MeetService` — 27 RPCs covering rooms, realtime join stream, auth,
  participants, recording, captioning, chat, reactions, breakouts, scheduling
  and summaries.
- `AgentCallback` — receives summaries / failure reports from remote agent
  workers (`whisper-stt`, `mistral-summarizer` in the `sunbeam-agents/` repo).
- `AgentWorker` client — this service dispatches jobs to those workers over
  gRPC; no LiveKit Agents framework involved.

For the architecture deep-dive see [ARCHITECTURE.md](./ARCHITECTURE.md). For
the product brief see [PLAN.md](./PLAN.md). For the authoritative build
contract see [DESIGN.md](./DESIGN.md).

---

## Quickstart

```bash
git clone https://src.sunbeam.pt/sunbeam/sunbeam-meet.git
cd sunbeam-meet
cargo build --workspace
```

The service links against shared Sunbeam dev instances — there are no local
substitutes (per the studio "no mocks for infrastructure-facing code" rule).
Copy `.env.example` (TODO: not yet checked in) and fill in the values below,
then:

```bash
cargo run -p sunbeam-meet-server
```

### Environment variables

All of these are required for integration tests and for running the server
against real services. See [DESIGN.md §4](./DESIGN.md).

| Variable | Purpose |
|---|---|
| `DATABASE_URL` | Postgres DSN (rooms, recordings, chat, schedules, summaries) |
| `VALKEY_URL` | Valkey — presence, session cache, rate limits |
| `NATS_URL` | NATS — cross-service events + `JoinRoom` fan-out subjects |
| `S3_ENDPOINT` | SeaweedFS S3 endpoint (Egress destination) |
| `S3_ACCESS_KEY` / `S3_SECRET_KEY` | SeaweedFS credentials |
| `CALDAV_URL` | Stalwart CalDAV base URL |
| `CALDAV_USER` / `CALDAV_PASSWORD` | Stalwart service creds |
| `KRATOS_PUBLIC_URL` | Ory Kratos public API (session verify) |
| `KRATOS_ADMIN_URL` | Ory Kratos admin API (identity lookup) |
| `KETO_READ_URL` | Ory Keto read API (relation tuple checks) |
| `KETO_WRITE_URL` | Ory Keto write API (tuple mutations) |
| `LIVEKIT_URL` | LiveKit WS/REST endpoint |
| `LIVEKIT_API_KEY` | LiveKit API key (also used to verify webhook JWTs) |
| `LIVEKIT_API_SECRET` | LiveKit API secret |
| `SCALEWAY_API_KEY` | Scaleway Whisper API key (passed through to workers) |
| `MISTRAL_API_KEY` | Mistral API key (passed through to workers) |
| `AGENT_WORKER_ENDPOINTS` | Comma-separated gRPC endpoints for agent workers |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OTLP traces endpoint |
| `BIND_ADDR` | Listen address (default `0.0.0.0:8080`) |

---

## Running tests

Unit suite is mock-free but dependency-free; integration suite hits real
shared dev services and needs the env vars above.

```bash
# Unit — fast, no services.
cargo nextest run --profile default

# Integration — requires all shared dev services reachable.
cargo nextest run --profile integration
```

Coverage floor is 85% line, enforced in CI via `cargo llvm-cov`. See
[DESIGN.md §8](./DESIGN.md) for the full test strategy.

### Workspace cargo aliases

Defined in `.cargo/config.toml`:

| Alias | What it runs |
|---|---|
| `cargo xlint` | `clippy --workspace --all-targets -- -D warnings` |
| `cargo xtest-unit` | `nextest run --workspace --exclude-lib --no-fail-fast` |
| `cargo xtest-integration` | `nextest run --workspace --run-ignored only --no-fail-fast` |
| `cargo xcov` | `llvm-cov nextest --workspace --fail-under-lines 85` |

---

## RPC surface

Proto files:
[`proto/meet.proto`](./proto/meet.proto),
[`proto/agent.proto`](./proto/agent.proto).

`sunbeam.meet.v1.MeetService` exposes 27 RPCs, grouped below. Browser clients
reach them over Connect (HTTP/JSON via `@connectrpc/connect-web`); internal
Rust services reach them over gRPC; same handlers, same port.

### Rooms

- `MeetService.CreateRoom` — create a room + backing LiveKit room.
- `MeetService.GetRoom` — fetch a room with current participant snapshot.
- `MeetService.ListRooms` — paginated list, filterable by status.
- `MeetService.UpdateRoom` — partial update (access level, quality, flags).
- `MeetService.EndRoom` — manually end a room; broadcasts `RoomEnded`.

### Realtime session

- `MeetService.JoinRoom` — bidirectional stream. Primary client channel.
  Client sends control messages (`JoinRequest`, `MuteToggle`, `LayoutChange`,
  `RaiseHandToggle`, `ClientReaction`, `QualityChange`, `PinParticipant`,
  `LeaveRequest`, `Ping`); server pushes room state (`RoomState`,
  `ParticipantJoined/Left/Updated`, `ActiveSpeakerChanged`, `Caption`,
  `CaptioningStatusChanged`, `RecordingStateChanged`, `ChatMessageBroadcast`,
  `ReactionBroadcast`, `BreakoutAnnouncement`, `WaitingRoomEntry`,
  `RoomEnded`, `Error`, `Pong`). See
  [ARCHITECTURE.md](./ARCHITECTURE.md#joinroom-fan-out) for the fan-out hub.

### Auth

- `MeetService.GenerateToken` — verify Keto authorization, then mint a
  LiveKit JWT with the appropriate grants.

### Participants

- `MeetService.InviteParticipant` — issue invite via email / link / Matrix.
- `MeetService.KickParticipant` — admin-only disconnect.
- `MeetService.UpdateParticipantRole` — promote/demote viewer/member/admin.
- `MeetService.MuteParticipant` — server-forced audio/video mute.

### Recording

- `MeetService.StartRecording` — start LiveKit Egress to SeaweedFS.
- `MeetService.StopRecording` — stop active Egress.
- `MeetService.ListRecordings` — paginated list per room.
- `MeetService.GetRecording` — fetch one recording, includes pre-signed URL.
- `MeetService.DeleteRecording` — delete DB row and SeaweedFS object.

### Captioning

- `MeetService.StartCaptioning` — dispatch `whisper-stt` worker to the room.
- `MeetService.StopCaptioning` — graceful worker shutdown.

### Chat

- `MeetService.SendChatMessage` — persist + broadcast on `JoinRoom` stream.
- `MeetService.GetChatHistory` — paginated backfill.
- `MeetService.DeleteChatMessage` — soft delete (author or moderator).

### Reactions

- `MeetService.SendReaction` — append to reaction log; broadcast via hub.

### Breakout rooms

- `MeetService.CreateBreakoutRooms` — spawn N breakouts with assignments.
- `MeetService.MergeBreakoutRooms` — collapse breakouts back into the parent.
- `MeetService.MoveParticipantToBreakout` — one-off reassignment.

### Scheduling (Stalwart CalDAV)

- `MeetService.ScheduleMeeting` — create CalDAV VEVENT + invitee emails.
- `MeetService.GetScheduledMeeting` — fetch a single schedule.
- `MeetService.ListScheduledMeetings` — range-filtered pagination.
- `MeetService.UpdateScheduledMeeting` — partial update, re-syncs CalDAV.
- `MeetService.CancelScheduledMeeting` — CANCEL iTIP, optional notify.

### Summaries

- `MeetService.GetMeetingSummary` — fetch summary + full transcript.
- `MeetService.ListMeetingSummaries` — paginated list, transcript omitted.

---

## Agent protocol

Captioning and summarization run in **separate processes** (the
[`sunbeam-agents/`](https://src.sunbeam.pt/sunbeam/sunbeam-agents) repo) and
communicate with this service over gRPC.

- `sunbeam.agent.v1.AgentWorker` — implemented by workers, called by this
  service. `StartJob`, `StopJob`, `GetStatus`, `StatusStream`.
- `sunbeam.agent.v1.AgentCallback` — implemented by this service, called by
  workers. `SubmitSummary`, `ReportFailure`.

Dispatch is round-robin across `AGENT_WORKER_ENDPOINTS`, with a reconnecting
`StatusStream` subscription per worker for health tracking. Workers
authenticate via mTLS on internal gRPC.

---

## Deployment

Production kustomization lives at `sbbb/base/meet/`; local stubs under
[`deploy/`](./deploy/) mirror it. Per the studio deploy rule, apply one
kustomization at a time and verify before moving on.

- 2+ replicas behind a ClusterIP, single ingress at `meet.sunbeam.pt`.
- LiveKit runs `hostNetwork` in `sbbb/base/media/` and is reached over the
  in-cluster service.
- Egress writes to SeaweedFS via its S3 API.
- Agent workers run on dedicated hosts (see `sunbeam-agents/deploy/`).

---

## Related repos

| Repo | Role |
|---|---|
| [`sunbeam-agents`](https://src.sunbeam.pt/sunbeam/sunbeam-agents) | `whisper-stt` + `mistral-summarizer` workers (implement `AgentWorker`, call `AgentCallback`) |
| [`beam-ui`](https://src.sunbeam.pt/sunbeam/beam-ui) | React 19 frontend; `@connectrpc/connect-web` client + LiveKit React SDK |
| [`sbbb`](https://src.sunbeam.pt/sunbeam/sbbb) | Cluster kustomizations (`base/meet/`, `base/media/`) |

---

## License

AGPL-3.0-or-later.

[`connect-rust`]: https://github.com/anthropics/connect-rust
