# sunbeam-meet — Architecture

Deep reference for `sunbeam-meet`. Pairs with [DESIGN.md](./DESIGN.md) (the
authoritative build contract) and [PLAN.md](./PLAN.md) (product brief). The
[README](./README.md) covers quickstart + RPC index; this doc covers how the
pieces fit together.

---

## System context

```
┌─────────────────────────────────────────────────────────────┐
│  beam-ui (React 19 + PandaCSS + Ark UI)                     │
│  @connectrpc/connect-web  →  HTTP/JSON                      │
│  livekit-client           →  WebRTC                         │
└──────────────┬────────────────────────┬─────────────────────┘
               │ Connect / gRPC-Web     │ WebRTC (media)
               ▼                        │
┌──────────────────────────────────┐    │
│  sunbeam-meet (this repo)        │    │
│  axum + connect-rust, one port   │    │
│  ├─ MeetService (27 RPCs)        │    │
│  ├─ AgentCallback (2 RPCs)       │    │
│  ├─ LiveKit webhook ingress      │    │
│  ├─ JoinRoom fan-out hub         │    │
│  └─ AgentWorker client dispatch  │    │
└──┬──┬──┬──┬──┬──┬──┬──┬──┬───────┘    │
   │  │  │  │  │  │  │  │  │            │
   ▼  ▼  ▼  ▼  ▼  ▼  ▼  ▼  ▼            │
  PG  Vk NA SW St Kr Ke LK  Scaleway    │
                         │               │
                         └───────────────┤
                                         │
┌─────────────────────────────────────────▼───────────────────┐
│  Remote agent workers (sunbeam-agents repo)                 │
│  ├─ whisper-stt        (audio → Scaleway → lk.transcription)│
│  └─ mistral-summarizer (transcript → Mistral → SubmitSummary)│
└─────────────────────────────────────────────────────────────┘
```

PG = Postgres, Vk = Valkey, NA = NATS, SW = SeaweedFS, St = Stalwart CalDAV,
Kr = Ory Kratos, Ke = Ory Keto, LK = LiveKit.

---

## Crate layout

Three crates in one workspace. Only `sunbeam-meet-proto` and
`sunbeam-meet-migrations` publish to the Sunbeam registry; the server binary
does not. Full directory tree in [DESIGN.md §3](./DESIGN.md).

| Crate | `publish` | Role |
|---|---|---|
| `sunbeam-meet-proto` | `["sunbeam"]` | Generated types for `sunbeam.meet.v1` + `sunbeam.agent.v1`. `build.rs` runs `tonic-build` and `connect-build` over `proto/*.proto` and re-exports both modules. |
| `sunbeam-meet-server` | `false` | The service binary. Everything below lives here. |
| `sunbeam-meet-migrations` | `["sunbeam"]` | Numbered `sqlx` migrations. Consumable by ops tooling and by `sunbeam-meet-server` at startup. |

### Server module tree

- `main.rs` — figment config load, telemetry init, axum bind with
  `connect-rust` mounted.
- `config.rs` — typed config, env + file overlay.
- `telemetry.rs` — `tracing-subscriber` + OpenTelemetry OTLP export.
- `domain/` — one module per entity (`room`, `participant`, `recording`,
  `caption`, `chat`, `reaction`, `breakout`, `schedule`, `summary`). Pure
  validation, state transitions, no I/O.
- `handlers/meet/` — one file per RPC group; maps request → domain →
  storage/clients. `handlers/agent_callback.rs` for the `AgentCallback` tree.
- `clients/` — outbound integrations: `livekit` (rooms, egress, JWT mint),
  `agent_worker` (round-robin dispatch + reconnecting `StatusStream`),
  `caldav`, `kratos`, `keto`, `scaleway`, `mistral` (the last two only for
  config/quota probes; actual calls happen in workers).
- `storage/pg.rs` — `sqlx::PgPool` plus query modules per entity.
- `cache/valkey.rs` — presence, session, rate limits.
- `events/nats.rs` — publisher; also consumed by the `JoinRoom` hub.
- `webhooks/livekit.rs` — signed-JWT webhook handler, republishes to NATS.
- `stream/join_room.rs` — the fan-out hub (see below).
- `middleware/auth.rs` — Kratos session → `Identity`.
- `middleware/authz.rs` — Keto check helpers.
- `error.rs` — single error type, maps to Connect/gRPC status.

---

## Canonical data flows

### 1. A participant joins a room

```
client                  sunbeam-meet                LiveKit        Keto    Postgres
  │                         │                         │             │         │
  │ GenerateToken ─────────▶│ authz.can(join) ────────┼────────────▶│         │
  │                         │◀────────────────────────┼── ok ───────│         │
  │                         │ mint JWT(grants, hidden:false)        │         │
  │◀──── token, lk url ─────│                         │             │         │
  │                                                                            │
  │ Connect: JoinRoom bidi stream opens                                        │
  │ ──── JoinRequest(room_id, token) ───────────▶                              │
  │                         │ register client in hub                            │
  │                         │ participants_history INSERT ──────────────────▶│
  │                         │ Valkey SET presence:{room}:{identity}            │
  │                         │ publish NATS meet.room.{room_id}                 │
  │                         │ subscribe hub → NATS (first join triggers)       │
  │                         │                                                   │
  │                         │                                                   │
  │ WebRTC PUBLISH/SUBSCRIBE ─────────────────────▶│                          │
  │                         │                                                   │
  │                         │◀── webhook participant_joined ─────│             │
  │                         │ verify signed JWT                                 │
  │                         │ translate → MeetServerMessage::ParticipantJoined  │
  │                         │ republish to NATS meet.room.{room_id}            │
  │◀──── ParticipantJoined broadcast (via hub) ───                              │
```

Key points:

- Token mint never talks to LiveKit; it's pure JWT signing.
- `hidden: true` is set for bot participants so they don't appear client-side.
- Webhooks are idempotent via `webhook_events.livekit_event_id UNIQUE`.

### 2. Recording a meeting

```
admin                  sunbeam-meet               LiveKit Egress     SeaweedFS
  │                         │                         │                 │
  │ StartRecording ────────▶│ authz.can(record)       │                 │
  │                         │ recordings INSERT (STARTING)               │
  │                         │ Egress.StartRoomCompositeEgress ────────▶│
  │                         │                         │ ──── write ────▶│
  │◀──── Recording(STARTING) │                         │                 │
  │                         │◀── webhook egress_started ─               │
  │                         │ recordings UPDATE → ACTIVE                 │
  │                         │ publish RecordingStateChanged via NATS     │
  │                         │ hub → all participants                     │
  │                         │                                            │
  │ StopRecording ─────────▶│ Egress.StopEgress ─────▶│                 │
  │                         │◀── webhook egress_ended ─                  │
  │                         │ recordings UPDATE → SAVED (or FAILED)      │
  │                         │ broadcast RecordingStateChanged            │
```

`GetRecording` fetches from Postgres and returns a pre-signed SeaweedFS URL.

### 3. Captioning + summary

```
admin/autostart   sunbeam-meet   whisper-stt worker   LiveKit   mistral-summarizer
     │                 │                │                │             │
     │ StartCaptioning▶│ pick worker (round robin)       │             │
     │                 │ AgentWorker.StartJob(token, cfg)▶             │
     │                 │                │ join LiveKit (hidden:true) ─▶│
     │                 │                │ receive audio frames ◀───────│
     │                 │                │ Scaleway Whisper chunk→text  │
     │                 │                │ publish lk.transcription ───▶│
     │                 │◀── webhook track_published (transcription) ──│
     │                 │ NATS → hub → Caption messages on JoinRoom    │
     │                 │                                               │
     │ (room ends: room_finished webhook)                              │
     │                 │ AgentWorker.StartJob(summarize) ──────────────▶
     │                 │                                          accumulate
     │                 │                                          Mistral call
     │                 │◀── AgentCallback.SubmitSummary ───────────────│
     │                 │ summaries INSERT                              │
```

On failure the worker calls `AgentCallback.ReportFailure`; the service maps
it onto `MeetServerMessage::CaptioningStatusChanged { state: FAILED, .. }`
and publishes via the hub.

---

## Dependency graph

| Who | Calls | For |
|---|---|---|
| `handlers/meet/rooms` | Postgres, LiveKit (room create) | CRUD + LiveKit room lifecycle |
| `handlers/meet/auth` | Keto, (in-process JWT signer) | `GenerateToken` |
| `handlers/meet/participants` | LiveKit, Postgres, Stalwart | mute / kick / invite |
| `handlers/meet/recording` | LiveKit Egress, Postgres, SeaweedFS (pre-sign) | recording lifecycle |
| `handlers/meet/captioning` | `clients/agent_worker` | dispatch whisper-stt |
| `handlers/meet/chat` | Postgres, hub | persist + broadcast |
| `handlers/meet/reactions` | Postgres, hub | append + broadcast |
| `handlers/meet/breakout` | LiveKit, Postgres | child rooms + moves |
| `handlers/meet/scheduling` | Stalwart CalDAV, NATS | VEVENT CRUD + notify |
| `handlers/meet/summaries` | Postgres | read-only |
| `handlers/agent_callback` | Postgres, hub | persist summary / fan out failure |
| `stream/join_room` (hub) | NATS, Valkey | cross-instance distribution + presence |
| `webhooks/livekit` | NATS, Postgres | verify, persist (idempotency), republish |
| `middleware/auth` | Kratos | session → `Identity` |
| `middleware/authz` | Keto | relation tuple checks |
| `clients/agent_worker` | remote `AgentWorker` gRPC | round-robin dispatch + `StatusStream` |

---

## JoinRoom fan-out

Each service instance owns an in-memory **hub** per room it holds a bidi
stream for. The hub is single-writer (the `axum` request task) with a
broadcast channel out to each subscribed client.

```
           ┌──────────── instance A ─────────────┐
client 1 ◀─┤ mpsc(256) ◀── hub A ◀── NATS sub ───┤◀──┐
client 2 ◀─┤ mpsc(256) ◀──                         │  │ meet.room.{room_id}
           └─────────────────────────────────────┘  │   (JetStream, retention 10s)
                                                    │
           ┌──────────── instance B ─────────────┐  │
client 3 ◀─┤ mpsc(256) ◀── hub B ◀── NATS sub ───┤◀─┘
client 4 ◀─┤ mpsc(256) ◀──                         │
           └─────────────────────────────────────┘  ▲
                                                    │ republish
                                      ┌─────────────┴──────────────┐
                                      │ webhooks/livekit.rs        │
                                      │ (any instance can receive) │
                                      └────────────────────────────┘
```

Behaviour:

- **Subscribe:** first `JoinRequest` for a room triggers NATS subscribe on
  `meet.room.{room_id}`. Subsequent joins on the same instance reuse it.
- **Unsubscribe:** last leave → NATS unsubscribe, hub drop.
- **Backpressure:** per-client `mpsc` is bounded at 256. Overflow drops the
  client with `MeetServerMessage::Error { code: BACKPRESSURE, fatal: true }`
  rather than blocking the hub.
- **Reconnect:** clients reconnect with a fresh `JoinRoom` stream; hub state
  is rebuilt from `participants_history` + Valkey presence.
- **Captions:** LiveKit text-stream messages arrive on the webhook path,
  translated to `MeetServerMessage::Caption`, republished through NATS.

See [DESIGN.md §7](./DESIGN.md) for the source contract.

---

## Auth model

Identity is resolved from Kratos; authorization is checked against Keto
relation tuples; LiveKit enforces its own grants server-side.

1. **Identity (`middleware/auth.rs`).** The request carries either a Kratos
   session cookie or `Authorization: Bearer <jwt>` (Kratos-issued). The
   middleware resolves to `Identity { id, email, traits }` and attaches it
   to the request extensions. `AgentCallback` requests skip this step and
   authenticate via mTLS.
2. **Authorization (`middleware/authz.rs`).** Handlers call
   `authz::can(identity, relation, object)`, which maps to a Keto
   `check_permission` on the namespaces below.
3. **LiveKit grant (`clients/livekit.rs`).** `GenerateToken` mints a JWT
   with grants derived from the participant's role.

### Keto namespaces

| Namespace | Object | Relations |
|---|---|---|
| `room` | `room:<uuid>` | `owner`, `moderator`, `participant`, `invited` |
| `recording` | `recording:<uuid>` | `owner`, `viewer` |
| `schedule` | `schedule:<uuid>` | `organizer`, `invitee` |

Role → LiveKit grant mapping:

| `ParticipantRole` | `roomJoin` | `canPublish` | `canSubscribe` | `canPublishData` | `roomAdmin` | `hidden` |
|---|---|---|---|---|---|---|
| `VIEWER` | ✓ | ✗ | ✓ | ✓ | ✗ | ✗ |
| `MEMBER` | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ |
| `ADMIN` | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ |
| `OWNER` | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ |
| bot (internal) | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ |

Webhook JWTs from LiveKit are verified against
`LIVEKIT_API_KEY` / `LIVEKIT_API_SECRET`; reject on bad signature or stale
`iat`.

### Rate limiting

A per-IP GCRA rate limiter (`tower_governor` with `SmartIpKeyExtractor`)
wraps the public surface (gRPC + `/webhooks/livekit`). Defaults are 100
rps sustained with a 200-token burst, tuned under `RateLimitConfig` in
`config.rs`. Rejections return HTTP 429 with
`{"code":"rate_limited","message":…}` and bump
`rate_limit_rejected_total{route=…}`. `/metrics` (and any future
`/healthz`) are mounted outside the limiter so probes and scrapers can
never be throttled.

---

## Observability

From [DESIGN.md §10](./DESIGN.md).

### Tracing

- `tracing` + `opentelemetry-otlp`. Every RPC gets a span with
  `rpc.service`, `rpc.method`, `room.id`, `identity.id` attributes.
- Webhook ingestion spans attach `livekit.event_id` so events are
  correlatable end-to-end with the triggering RPC.
- `JoinRoom` stream spans are long-lived; per-message events are logged with
  the span's trace id so the hub path stays traceable.

### Metrics

Prometheus `/metrics`:

- RPC request counter + duration histogram (labels: `service`, `method`,
  `code`).
- `meet_joinroom_subscribers` gauge (labels: `room_id`).
- `meet_joinroom_backpressure_drops_total` counter.
- `meet_webhook_lag_seconds` histogram (webhook `created_at` → processing).
- `meet_egress_outcome_total` counter (labels: `outcome=success|fail`).
- `meet_agent_worker_available` gauge per endpoint.

### Alerts (stubbed in `sbbb/base/meet/` PrometheusRules)

- Recording failure rate above threshold.
- Room at capacity (approaching `max_participants`).
- p99 `JoinRoom` send latency.
- Webhook delivery lag.
- Agent worker unavailable.

---

## Deployment topology

- **Replicas:** 2+, behind a ClusterIP service, single ingress at
  `meet.sunbeam.pt`. Connect / gRPC / gRPC-Web all share the one port.
- **LiveKit:** multi-node in `sbbb/base/media/`, `hostNetwork` for WebRTC
  performance, Valkey-backed coordination. sunbeam-meet reaches it over the
  in-cluster service name for REST/webhooks.
- **SeaweedFS:** Egress S3 destination; recordings + transcripts land here.
- **Agent workers:** separate hosts (see `sunbeam-agents/`), registered via
  `AGENT_WORKER_ENDPOINTS`. Round-robin dispatch + reconnecting
  `StatusStream` subscription tracks availability.
- **NATS:** JetStream stream for `meet.room.>` subjects with a short (~10s)
  retention window; enough to survive brief instance restarts without
  replaying stale history.
- **Postgres:** shared dev/prod instance; migrations applied by
  `sunbeam-meet-migrations` consumers at deploy time.

Per studio rule: apply one kustomization at a time, verify with the
`sunbeam` CLI health probe, tail logs for 30s, check Alertmanager, wait for
confirmation.

---

## Failure modes & mitigations

| Failure | Detection | Mitigation |
|---|---|---|
| Slow client on `JoinRoom` | `mpsc(256)` full | Drop client with `Error { BACKPRESSURE, fatal }`; hub keeps serving others. Metric `meet_joinroom_backpressure_drops_total`. |
| LiveKit webhook redelivery | `webhook_events.livekit_event_id UNIQUE` | Idempotent insert; subsequent events no-op. |
| Webhook bad signature / stale `iat` | JWT verify | 401 immediately, metric increment, no side effects. |
| NATS disconnect | client error | Reconnect with backoff; hub keeps in-memory state, resyncs from Postgres/Valkey on reconnect. |
| Instance crash mid-stream | TCP reset | Clients reconnect; new hub rebuilds from `participants_history` + Valkey presence; in-flight webhooks are re-delivered by LiveKit. |
| Agent worker unavailable | `StatusStream` disconnect / `GetStatus` fail | Mark endpoint unhealthy, skip in round-robin, surface `CaptioningStatusChanged::FAILED` or `ReportFailure` to clients. Metric `meet_agent_worker_available`. |
| Egress failure | webhook `egress_ended` with error | `recordings` row goes `FAILED`, `RecordingStateChanged` broadcast, metric + alert. |
| Mistral / Scaleway quota exhausted | worker `ReportFailure` | surface error to clients; no retry loops in the service; workers own retry policy. |
| Stalwart CalDAV unreachable | client error | `ScheduleMeeting` fails; no local fallback (events must be authoritative on CalDAV). |
| Postgres down | sqlx error | Connect error mapped to `Unavailable`; readiness probe fails; k8s stops routing. |

---

## Test strategy (summary)

Full plan in [DESIGN.md §8](./DESIGN.md). In one paragraph:

Unit tests are mock-free and cover pure logic — domain validation, webhook
JWT verification, Connect↔gRPC error mapping, token grant builders, caption
ordering, hub subscribe/unsubscribe/backpressure with `tokio` channels.
Integration tests hit the real shared Sunbeam dev services (Postgres,
LiveKit, Keto, Kratos, NATS, SeaweedFS, Stalwart) — rooms CRUD, real
LiveKit token-and-join, Egress → SeaweedFS, CalDAV, Keto checks, NATS
publish across two instances, webhook → `JoinRoom` fan-out. Coverage floor
is 85% line (`cargo llvm-cov`); streaming RPCs get explicit
subscribe/drop/reconnect cases. No string-matching on state, no
`#[ignore]` on flaky integration tests.
