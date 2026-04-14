# Sunbeam Meet — Product Plan

A Rust gRPC meeting service backed by LiveKit, with a beam-ui React frontend.
Replaces the existing Django `meet/` app and Element Call deployment.

---

## Goals

Build a self-hosted video conferencing service with feature parity to Google
Meet / Microsoft Teams, including:

- 1080p60 @ 8 Mbps baseline video, 4K HDR aspirational
- Live captioning (Scaleway Whisper API)
- Live recording (LiveKit Egress → SeaweedFS)
- AI meeting summaries (Mistral)
- Persistent chat, reactions, raise-hand
- Screen sharing with detail/motion content hints
- Breakout rooms
- Calendar scheduling via Stalwart CalDAV
- Picture-in-Picture video popouts
- Up to 300 participants per room (multi-node LiveKit)

---

## Decisions (locked)

| Question | Answer |
|----------|--------|
| Backend language | **All Rust.** No Python, no Node. |
| RPC protocol | **Connect** via [`connect-rust`](https://github.com/anthropics/connect-rust). Speaks Connect + gRPC + gRPC-Web on the same port. |
| Frontend | **beam-ui** (React 19 + PandaCSS + Ark UI), `@connectrpc/connect-web` client. |
| Existing Django `meet/` | **Replace and decommission.** |
| SIP/PSTN | **Deferred.** Not v1. |
| AI agents | **Scaleway Whisper API** (STT) + **Mistral API** (summarizer). No on-device inference, no GPU. |
| Encryption | **TLS everywhere**, no client-side E2EE. Lets recording + captioning work. |
| Calendar | **Stalwart CalDAV** integration. |
| Room scale | **300 participants**, multi-node LiveKit cluster. |
| Agent dispatch | **`sunbeam-meet` dispatches directly** via gRPC to remote agent workers. Skip LiveKit's Python/Node-only agent worker protocol. |
| Bot visibility | **Hidden participants** via LiveKit's `hidden: true` token grant. |

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  beam-ui frontend (React 19 + PandaCSS + Ark UI)            │
│  @connectrpc/connect-web → HTTP/JSON                        │
│  livekit-client → WebRTC (audio/video tracks)               │
└─────────────────┬───────────────────────────────────────────┘
                  │ Connect protocol (HTTPS)
┌─────────────────▼───────────────────────────────────────────┐
│  sunbeam-meet (Rust, connect-rust + Axum)                   │
│  ├─ sunbeam.meet.v1.MeetService                             │
│  ├─ LiveKit server API client (rooms, tokens, egress)       │
│  ├─ LiveKit webhook ingestion (signed JWT)                  │
│  ├─ AgentWorker dispatch (gRPC → remote workers)            │
│  ├─ AgentCallback receiver (gRPC ← workers)                 │
│  ├─ Stalwart CalDAV client (scheduling)                     │
│  ├─ NATS publisher (cross-service events)                   │
│  ├─ Postgres (sqlx) — rooms, recordings, chat, schedules    │
│  └─ Valkey — session cache, presence                        │
└─────────────────┬───────────────────────────────────────────┘
                  │ gRPC / WebSocket / HTTP
┌─────────────────▼───────────────────────────────────────────┐
│  LiveKit cluster (multi-node, hostNetwork, Valkey-backed)   │
│  ├─ VP9 L3T3_KEY for HD, AV1 for 4K HDR                     │
│  ├─ Egress → SeaweedFS (S3-compatible)                      │
│  └─ Webhooks → sunbeam-meet                                 │
└─────────────────────────────────────────────────────────────┘
                  ▲
                  │ WebRTC (hidden participants)
┌─────────────────┴───────────────────────────────────────────┐
│  Remote agent workers (Rust, livekit + tonic)               │
│  ├─ whisper-stt — audio frames → Scaleway Whisper API       │
│  │   → publishes to lk.transcription text stream            │
│  └─ mistral-summarizer — accumulates lk.transcription       │
│      → on room end → Mistral API → SubmitSummary callback   │
└─────────────────────────────────────────────────────────────┘
```

### Why Connect (not raw tonic + gRPC-Web proxy)

`connect-rust` (Anthropic, production-tested, 12,800-test conformance suite)
serves Connect (HTTP/JSON), gRPC (HTTP/2 binary), and gRPC-Web on the **same
handlers, same port**. This means:

- **Browser clients** call RPCs over plain `fetch()` via
  `@connectrpc/connect-web`. No WASM. No Envoy proxy. Deno's weak gRPC support
  doesn't matter.
- **Internal Rust services** (Sol, WFE) call native gRPC.
- **One service binary, one ingress, three protocols.**

### Why direct agent dispatch (not LiveKit Agents framework)

LiveKit's agent worker registration protocol is undocumented and
Python/Node-only. Reverse-engineering it from `livekit-protocol` would mean
chasing upstream changes forever. Instead:

1. `sunbeam-meet` mints a `hidden: true` LiveKit token for the bot.
2. `sunbeam-meet` calls the worker's `AgentWorker.StartJob` gRPC.
3. The worker joins the LiveKit room directly via the Rust `livekit` crate.
4. LiveKit webhooks tell `sunbeam-meet` if the bot disconnects.

Public stable APIs only. Full control. Round-robin dispatch in the service.

### Why `hidden: true` for bots

LiveKit's `hidden` token grant makes the bot invisible to other participants:
- No `participant_joined` event broadcast to clients
- Doesn't appear in client-side `listParticipants`
- **Can still publish data** (text streams for captions)
- Server-side APIs and webhooks still see it for liveness tracking

No phantom users in the meeting.

---

## Deliverables

### 1. `sunbeam-meet/` (this repo)

```
sunbeam-meet/
├─ proto/
│  ├─ meet.proto                # sunbeam.meet.v1.MeetService
│  └─ agent.proto               # sunbeam.agent.v1.AgentWorker, AgentCallback
├─ Cargo.toml                   # workspace
├─ crates/
│  ├─ sunbeam-meet-proto/       # generated tonic + connect-rust types
│  ├─ sunbeam-meet-server/      # main service binary
│  └─ sunbeam-meet-migrations/  # sqlx migrations
├─ Dockerfile
├─ workflows.yaml               # WFE CI/CD
├─ deploy/                      # kustomize → sbbb/base/meet/
└─ README.md
```

### 2. `sunbeam-agents/` (separate repo)

```
sunbeam-agents/
├─ Cargo.toml                   # workspace
├─ crates/
│  ├─ agent-common/             # shared: livekit room join, AgentWorker server,
│  │                            #         AgentCallback client, status reporting
│  ├─ whisper-stt/              # audio chunking → Scaleway Whisper API
│  │                            # → publishes to lk.transcription
│  └─ mistral-summarizer/       # accumulates transcript → Mistral API
│                               # → SubmitSummary callback to sunbeam-meet
├─ Dockerfile.whisper
├─ Dockerfile.mistral
├─ workflows.yaml
└─ deploy/                      # systemd units / containers for remote hosts
```

### 3. Frontend in `beam-ui/`

New app under `beam-ui/app/meet/` (or sibling app), built with existing
beam-ui components plus:

- LiveKit React SDK for media tracks
- `@connectrpc/connect-web` for RPC
- Document Picture-in-Picture API for video popouts
- Existing PandaCSS tokens for styling

### 4. Infrastructure in `sbbb/base/meet/`

- Deployment for `sunbeam-meet` (2+ replicas)
- Service (ClusterIP, single port for Connect/gRPC/gRPC-Web)
- Ingress at `meet.sunbeam.pt`
- ServiceMonitor + PrometheusRules (recording failures, room capacity, agent
  health, p99 RPC latency, webhook delivery lag)
- ConfigMap with quality presets, egress templates

### 5. LiveKit values updates in `sbbb/base/media/`

- Multi-node config tuned for 300-participant rooms
- Custom `VideoPreset` definitions wired through agent worker token grants

---

## RPC surface (proto/meet.proto)

`sunbeam.meet.v1.MeetService` — 27 RPCs:

- **Rooms**: `CreateRoom`, `GetRoom`, `ListRooms`, `UpdateRoom`, `EndRoom`
- **Realtime session**: `JoinRoom` (bidirectional stream, primary client channel)
- **Auth**: `GenerateToken`
- **Participants**: `InviteParticipant`, `KickParticipant`,
  `UpdateParticipantRole`, `MuteParticipant`
- **Recording**: `StartRecording`, `StopRecording`, `ListRecordings`,
  `GetRecording`, `DeleteRecording`
- **Captioning**: `StartCaptioning`, `StopCaptioning`
- **Chat**: `SendChatMessage`, `GetChatHistory`, `DeleteChatMessage`
- **Reactions**: `SendReaction`
- **Breakout**: `CreateBreakoutRooms`, `MergeBreakoutRooms`,
  `MoveParticipantToBreakout`
- **Scheduling**: `ScheduleMeeting`, `GetScheduledMeeting`,
  `ListScheduledMeetings`, `UpdateScheduledMeeting`, `CancelScheduledMeeting`
- **Summaries**: `GetMeetingSummary`, `ListMeetingSummaries`

`JoinRoom` stream messages (proto/meet.proto):

- **Client → Server**: `JoinRequest`, `LeaveRequest`, `MuteToggle`,
  `LayoutChange`, `RaiseHandToggle`, `ClientReaction`, `QualityChange`,
  `PinParticipant`, `Ping`
- **Server → Client**: `RoomState`, `ParticipantJoined`, `ParticipantLeft`,
  `ParticipantUpdated`, `ActiveSpeakerChanged`, `Caption`,
  `CaptioningStatusChanged`, `RecordingStateChanged`, `ChatMessageBroadcast`,
  `ReactionBroadcast`, `BreakoutAnnouncement`, `RoomEnded`, `Error`,
  `WaitingRoomEntry`, `Pong`

## Agent protocol (proto/agent.proto)

`sunbeam.agent.v1.AgentWorker` (workers implement, sunbeam-meet calls):

- `StartJob` — assign a room+token+config to the worker
- `StopJob` — stop a running job (graceful or immediate)
- `GetStatus` — snapshot of worker capacity and active jobs
- `StatusStream` — long-lived stream of status changes, errors, heartbeats

`sunbeam.agent.v1.AgentCallback` (sunbeam-meet implements, workers call):

- `SubmitSummary` — store completed meeting summary + transcript
- `ReportFailure` — surface job errors to sunbeam-meet for client notification

---

## Quality presets

| Preset | Resolution | FPS | Bitrate | Codec | SVC Mode | Notes |
|--------|-----------|-----|---------|-------|----------|-------|
| `AUTO` | adaptive | adaptive | adaptive | VP9 | L3T3_KEY | server picks layers |
| `LOW` | 640×360 | 30 | 400 Kbps | VP9 | L1T3 | mobile / poor network |
| `MEDIUM` | 1280×720 | 30 | 2.5 Mbps | VP9 | L2T3 | default fallback |
| `HIGH` | 1920×1080 | 60 | 8 Mbps | VP9 | L3T3_KEY | **baseline target** |
| `ULTRA` | 3840×2160 | 60 | 20 Mbps | AV1 | L3T3_KEY | **4K HDR aspirational** |

Simulcast fallback for `HIGH`: 720p30 @ 2.5M, 360p30 @ 400K.
Backup codec (VP8) auto-publishes for clients that can't decode VP9/AV1.

### 4K HDR caveats

- AV1 encode is CPU-heavy; needs HW encode (RTX 40+, Intel Arc, RDNA3+) or
  it'll crush client CPUs
- Browser AV1 encode support: Chrome good, Firefox partial, Safari absent
- VP9 Profile 2 (10-bit) is the realistic HDR fallback today
- LiveKit's `enabled_codecs` already includes VP9, AV1, H.264, VP8 (in that
  preference order) — see `sbbb/base/media/livekit-values.yaml`

---

## Frontend views (beam-ui)

1. **Lobby** — Camera/mic preview, device selection, room info, waiting room
   queue
2. **Meeting** — Gallery / speaker / sidebar / spotlight layouts, controls
   bar, participant panel, chat panel, captions overlay, reactions
3. **Screen share** — Shared content fills main area, speaker strip on side
4. **Popout window** — Document PiP (Chrome/Edge) for full custom UI in
   floating window, fallback to `HTMLVideoElement.requestPictureInPicture()`
   for Safari/Firefox single-video
5. **Breakout** — Room selector, timer, reassignment UI
6. **Recording controls** — Start/stop, mode picker, status badge
7. **Settings** — Devices, quality preset, caption language, background
   blur/replace, noise cancellation
8. **Post-meeting** — Recording playback, transcript viewer, AI summary +
   action items
9. **Schedule** — Create/edit, recurrence, invitee list, Stalwart CalDAV sync

### Key components to add to beam-ui

- `VideoTile` — participant video with name overlay, connection quality
  indicator, speaking indicator, popout button
- `ControlBar` — mic, camera, screen share, record, captions, chat,
  reactions, raise hand, leave
- `ParticipantList` — sorted by role/speaking, mute/kick controls for admins
- `CaptionOverlay` — floating captions with speaker attribution
- `ChatPanel` — messages, replies, file refs, reactions
- `DeviceSelector` — camera/mic/speaker picker with preview
- `QualitySelector` — preset picker with bandwidth indicator
- `MeetingScheduler` — date/time, recurrence, invitee list
- `PopoutWindow` — Document PiP wrapper with PandaCSS adopted stylesheets
- `BreakoutRoomManager` — visual assignment UI, drag-drop participants

---

## Backend dependencies (real, not mocked)

| Dependency | Purpose | How we hit it in tests |
|------------|---------|------------------------|
| Postgres | Rooms, recordings, chat, schedules, summaries | Shared dev instance |
| Valkey | Session cache, presence, LiveKit coordination | Shared dev instance |
| NATS | Cross-service events (calendar, notifications) | Shared dev instance |
| SeaweedFS | Recording storage (Egress S3 destination) | Shared dev instance |
| Stalwart | CalDAV scheduling | Shared dev instance |
| Ory Kratos | OIDC identity (who is this user) | Shared dev instance |
| Ory Keto | Authorization (can this user join/admin) | Shared dev instance |
| LiveKit | Media routing, Egress, webhooks | Shared dev instance (in-cluster) |
| Scaleway Whisper API | STT for captions | Real API, throttled in CI |
| Mistral API | Summary generation | Real API, throttled in CI |

Per `CLAUDE.md`: **no mocks for infrastructure-facing code.** Integration
tests hit the real services.

---

## Phased delivery

1. **Proto + design note** ✅ (this doc + `proto/meet.proto`,
   `proto/agent.proto`)
2. **Scaffold `sunbeam-meet` workspace** — `sunbeam-meet-proto`,
   `sunbeam-meet-server`, `sunbeam-meet-migrations`
3. **Core service** — rooms CRUD, token minting, webhook ingestion
4. **`JoinRoom` stream** — fan-out from webhooks to connected clients
5. **Recording** — `StartRecording` → LiveKit Egress API + SeaweedFS
6. **Scaffold `sunbeam-agents` workspace** — `agent-common`, `whisper-stt`,
   `mistral-summarizer`
7. **Captioning end-to-end** — `StartCaptioning` → dispatch whisper-stt →
   captions appear on `JoinRoom` stream
8. **Summaries end-to-end** — auto-dispatch mistral-summarizer at room start
   → `SubmitSummary` callback on room end
9. **Frontend foundation in beam-ui** — lobby + meeting view with LiveKit
   React SDK + Connect client
10. **Frontend feature parity** — chat, captions overlay, recording controls,
    popout, breakout rooms, screen sharing, scheduling
11. **Stalwart CalDAV scheduling** — `ScheduleMeeting` RPC + email invites
12. **Infra deploy** — `sbbb/base/meet/` kustomization, one apply at a time
    (per project deployment rules)
13. **Cutover** — DNS swap from old `meet/` Django app, decommission
14. **Observability hardening** — Prometheus alerts, OpenTelemetry traces,
    SLO dashboards

---

## Open / deferred

- **SIP/PSTN dial-in** — deferred per decision
- **End-to-end encryption** — skipped; TLS-only
- **Translation captions** — possible future agent (mistral translation
  pipeline on `lk.transcription`)
- **Webinar mode** — HLS fan-out for >300 viewers, deferred
- **Mobile native apps** — out of scope for v1; web works on mobile browsers

---

## Reference

- `proto/meet.proto` — full RPC surface
- `proto/agent.proto` — agent worker + callback protocol
- `sbbb/base/media/livekit-values.yaml` — current LiveKit deployment
- `meet/` (Django) — reference for existing room/recording state machine
  (will be decommissioned)
- `cli-worktree/sunbeam-proto/proto/code.proto` — reference proto style
- `estafeta/` — reference Rust gRPC service layout
