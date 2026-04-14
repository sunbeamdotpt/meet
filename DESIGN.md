# sunbeam-meet — Design Note (spec-to-service, Phase 0)

Derived from `PLAN.md` + `proto/meet.proto` + `proto/agent.proto`.
This note is the authoritative input for Phases 1–4. Sub-agents read this,
not PLAN.md.

---

## 1. Scope for this skill run

**In scope:** build `sunbeam-meet/` — the Rust service that implements
`sunbeam.meet.v1.MeetService` and `sunbeam.agent.v1.AgentCallback`, and
calls `sunbeam.agent.v1.AgentWorker` on remote workers.

**Out of scope (separate skill runs / repos):**

- `sunbeam-agents/` — whisper-stt, mistral-summarizer workers.
- `beam-ui/` frontend app.
- `sbbb/base/meet/` kustomization (written here as stubs, applied separately
  per the one-at-a-time deploy rule).
- Django `meet/` decommission.

## 2. Divergences from the estafeta template

The skill's reference implementation is `estafeta`. This project differs:

| Area | estafeta | sunbeam-meet | Impact |
|---|---|---|---|
| RPC stack | `tonic` only | `connect-rust` + `axum` (Connect/gRPC/gRPC-Web on one port) | `build.rs` uses `connect-build` alongside `tonic-build`; server bootstrap is axum-based |
| Proto files | 1 | 2 (`meet.proto`, `agent.proto`) | proto crate exports two modules; server links both |
| Services implemented | 1 | 2 (`MeetService` + `AgentCallback`) | two handler trees under `handlers/` |
| Services called | 0 | 1 (`AgentWorker` on workers) | `clients/agent_worker.rs` with round-robin dispatch + reconnecting `StatusStream` |
| Streaming RPCs | 0 | 1 bidi (`JoinRoom`) + 1 server-stream (`StatusStream`) | fan-out hub: webhook → per-participant channel → stream tx |
| External deps | postgres | 10 (see §4) | integration suite is bigger; CI matrix needs more secrets |
| Auth | n/a | Ory Kratos (OIDC) + Ory Keto (relation tuples) | `middleware/auth.rs` + `authz.rs`; token minting uses Kratos session → LiveKit JWT |
| RPC count | ~5 | 27 on MeetService, 4 + 2 on agent services | larger handler surface; parallel fan-out across domain modules is load-bearing |

**Scope realism:** "afternoon" (the estafeta quote) doesn't apply. This is
multi-day work. Phase 2 fan-out is still the right move.

## 3. Crate layout

```
sunbeam-meet/
├── Cargo.toml                      # workspace
├── README.md
├── ARCHITECTURE.md
├── DESIGN.md                       # this file
├── PLAN.md                         # product plan (already present)
├── Dockerfile                      # multi-stage, distroless runtime
├── workflows.yaml                  # WFE CI/CD
├── deploy/                         # kustomize stubs for sbbb/base/meet/
├── proto/
│   ├── meet.proto                  # already present
│   └── agent.proto                 # already present
└── crates/
    ├── sunbeam-meet-proto/         # publish = ["sunbeam"]
    │   ├── build.rs                # tonic-build + connect-build
    │   └── src/lib.rs              # re-exports sunbeam.meet.v1 + sunbeam.agent.v1
    ├── sunbeam-meet-server/        # publish = false
    │   └── src/
    │       ├── main.rs             # figment config, axum + connect-rust bind
    │       ├── config.rs
    │       ├── telemetry.rs        # opentelemetry + tracing-subscriber
    │       ├── domain/
    │       │   ├── room.rs
    │       │   ├── participant.rs
    │       │   ├── recording.rs
    │       │   ├── caption.rs
    │       │   ├── chat.rs
    │       │   ├── reaction.rs
    │       │   ├── breakout.rs
    │       │   ├── schedule.rs
    │       │   └── summary.rs
    │       ├── handlers/
    │       │   ├── meet/           # one file per RPC group (rooms, recording, …)
    │       │   └── agent_callback.rs
    │       ├── clients/
    │       │   ├── livekit.rs      # rooms, egress, JWT mint
    │       │   ├── agent_worker.rs # round-robin dispatch + StatusStream
    │       │   ├── caldav.rs       # Stalwart
    │       │   ├── kratos.rs
    │       │   ├── keto.rs
    │       │   ├── scaleway.rs     # Whisper (for spec/quota probes only)
    │       │   └── mistral.rs      # (for spec/quota probes only)
    │       ├── storage/
    │       │   └── pg.rs           # sqlx pool + query modules per entity
    │       ├── cache/
    │       │   └── valkey.rs       # presence, session, rate limits
    │       ├── events/
    │       │   └── nats.rs         # publisher
    │       ├── webhooks/
    │       │   └── livekit.rs      # signed-JWT webhook handler
    │       ├── stream/
    │       │   └── join_room.rs    # fan-out hub for the bidi JoinRoom RPC
    │       ├── middleware/
    │       │   ├── auth.rs         # Kratos session → identity
    │       │   └── authz.rs        # Keto check helpers
    │       └── error.rs            # one error type → Connect/gRPC status
    │   └── tests/
    │       └── integration/        # hits real shared sunbeam services
    └── sunbeam-meet-migrations/    # publish = ["sunbeam"]
        └── migrations/             # sqlx migrations, numbered
```

## 4. External dependencies (real, shared dev instances — no mocks)

Per `CLAUDE.md` testing rule. All of these are *hit* by integration tests.

| # | Service | Used for | Connection |
|---|---|---|---|
| 1 | Postgres | rooms, recordings, chat, schedules, summaries, breakout assignments | `DATABASE_URL` |
| 2 | Valkey | presence, session cache, LiveKit coordination (shared with LK) | `VALKEY_URL` |
| 3 | NATS | cross-service events (calendar, notifications) | `NATS_URL` |
| 4 | SeaweedFS | Egress destination (S3 API) | `S3_ENDPOINT` + creds |
| 5 | Stalwart | CalDAV scheduling | `CALDAV_URL` + service creds |
| 6 | Ory Kratos | OIDC identity | `KRATOS_PUBLIC_URL`, `KRATOS_ADMIN_URL` |
| 7 | Ory Keto | authorization (relation tuples) | `KETO_READ_URL`, `KETO_WRITE_URL` |
| 8 | LiveKit | rooms, egress, JWTs, webhooks | `LIVEKIT_URL`, `LIVEKIT_API_KEY`, `LIVEKIT_API_SECRET` |
| 9 | Scaleway Whisper | STT (for workers; service only validates config) | `SCALEWAY_API_KEY` |
| 10 | Mistral | summaries (for workers; service only validates config) | `MISTRAL_API_KEY` |

## 5. Data model (first cut — implementer refines)

Postgres entities (one table each unless noted). Timestamps are `timestamptz`,
IDs are `uuid v7` (sortable). Soft deletes via `deleted_at` where retention
rules apply; hard deletes for ephemeral state.

- `rooms` — id, name, slug, created_by, status (lobby|active|ended|archived), settings (jsonb: quality preset, waiting room, chat flags), started_at, ended_at, livekit_room_name
- `participants_history` — id, room_id, identity, role, joined_at, left_at, hidden (for bot audit)
- `recordings` — id, room_id, egress_id, mode, status, started_at, ended_at, storage_url, duration_ms, size_bytes, error
- `chat_messages` — id, room_id, sender_identity, body, reply_to, created_at, deleted_at
- `reactions` — append-only log; id, room_id, sender_identity, emoji, created_at (kept 30d)
- `breakout_rooms` — id, parent_room_id, name, livekit_room_name, ended_at
- `breakout_assignments` — breakout_room_id, participant_identity, assigned_at
- `schedules` — id, owner_identity, title, description, starts_at, ends_at, rrule, caldav_uid, caldav_etag, room_id (nullable until materialized)
- `schedule_invitees` — schedule_id, identity, email, response (pending|accepted|declined|tentative)
- `summaries` — id, room_id, transcript_ref (seaweed URL), summary_md, action_items (jsonb), generated_at, model
- `webhook_events` — id, livekit_event_id (unique), type, payload (jsonb), received_at, processed_at (idempotency)

Valkey keys (ephemeral):
- `presence:{room_id}` → hash of identity → last_seen
- `session:{token_id}` → JSON (identity, room_id, role, expires_at)
- `rate:{identity}:{rpc}` → sliding window

## 6. Auth model

- **Identity:** client sends Kratos session cookie or `Authorization: Bearer <jwt>` (Kratos-issued). Middleware resolves to an `Identity { id, email, traits }`.
- **Authorization:** Keto relation tuples. Namespaces: `room` (object = room id, relations = `owner`, `moderator`, `participant`, `invited`), `recording`, `schedule`. Handlers call `authz::can(identity, relation, object)`.
- **LiveKit token minting:** `GenerateToken` — verify Keto says the identity can join the room, then mint a LiveKit JWT with `name`, `identity`, grants (`roomJoin`, `canPublish`, `canSubscribe`, `canPublishData`), and `hidden: false` for humans / `true` for bots.
- **Agent callbacks:** workers authenticate to `AgentCallback` with mTLS *or* a shared HMAC header derived from a per-worker secret (decide during Phase 1; default to mTLS since we already have it for internal gRPC).
- **Webhooks:** LiveKit webhook JWT verified against the `LIVEKIT_API_KEY`/`LIVEKIT_API_SECRET` pair; reject on bad signature or stale `iat`.

## 7. JoinRoom fan-out architecture

Single hub per room, owned by the service instance that holds the bidi stream.
Cross-instance distribution via **NATS subject** `meet.room.{room_id}` —
instances subscribe on first join, unsubscribe on last leave. Webhooks are
consumed on *any* instance and republished to NATS for hub delivery.

- Client → server: control messages (mute, layout, raise hand, ping, …) are handled locally and optionally persisted (reactions, chat enter the respective tables via the same handlers).
- Server → client: webhook translator produces `MeetServerMessage` variants and broadcasts to subscribers. Captions flow from LiveKit text stream → NATS → hub → clients.
- Backpressure: per-client `mpsc` bounded (default 256); overflow drops the client stream with `Error { code: BACKPRESSURE }` rather than blocking the hub.

## 8. Test strategy

**Unit (mock-free, fast):**
- domain validation (slug rules, role transitions, rrule parsing)
- webhook JWT verification
- Connect error ↔ gRPC status mapping
- token grant builders
- caption merge/ordering logic
- fan-out hub subscribe/unsubscribe/backpressure (tokio, no external deps)

**Integration (real services, `cargo nextest run --profile integration`):**
- rooms CRUD round-trip vs Postgres
- token mint verified by real LiveKit join attempt
- Egress start/stop against real LiveKit + SeaweedFS write
- CalDAV create/update/delete against Stalwart
- Keto permission checks
- NATS event publish + subscribe across two service instances
- webhook ingest → JoinRoom fan-out with two subscribed clients

**Coverage floor:** 85% line via `cargo llvm-cov`, matching estafeta.
Streaming RPCs get explicit subscribe/drop/reconnect cases.

**Anti-patterns banned:** string-matching on state, mocked Postgres/LiveKit/Keto, `#[ignore]` on flaky integration tests.

## 9. CI / `workflows.yaml`

Tier structure from the skill template:
1. `lint` — fmt + clippy (`-D warnings`) + `cargo deny check`
2. `test-unit` — nextest, no services
3. `test-integration` — nextest against shared sunbeam dev services (needs secrets injected; the workflow lists them)
4. `coverage` — `cargo llvm-cov --fail-under-lines 85`
5. `publish-proto` — tier-1: `sunbeam-meet-proto` to sunbeam registry on tag
6. `publish-migrations` — tier-2: `sunbeam-meet-migrations` to sunbeam registry on tag
7. `image` — tier-3: build + push `sunbeam-meet-server` container
8. `release` — GH-style release on Gitea with changelog

Gitea remote, not GitHub (per `CLAUDE.md`). Repo created via Gitea MCP.

## 10. Observability

- `tracing` with OpenTelemetry OTLP export; every RPC gets a span with `rpc.service`, `rpc.method`, `room.id`, `identity.id` attributes.
- Prometheus `/metrics` endpoint: RPC counters + histograms, fan-out gauge (rooms × subscribers), webhook lag, Egress success/fail counters, agent worker availability.
- Stubs for `sbbb/base/meet/` PrometheusRules: recording failure rate, room-at-capacity, p99 `JoinRoom` send latency, webhook delivery lag, agent worker unavailable.

## 11. Phase 2 sub-agent assignments

Spawned in parallel once you confirm this note.

1. **Implementer** — workspace + proto crate + server skeleton + all 27 MeetService handlers + AgentCallback + AgentWorker client + webhook + JoinRoom hub. Reads §3, §5, §6, §7.
2. **Test-author** — unit suite from §8 first tier; integration suite from §8 second tier; coverage config. No mocks. Reads §4, §8.
3. **Doc-writer** — README quickstart (local dev with shared services), ARCHITECTURE.md (data flow + dep graph + fan-out diagram + deployment topology), rustdoc on every public item in `sunbeam-meet-proto` and `sunbeam-meet-server`. Reads §3, §7, §10.
4. **Release-engineer** — Dockerfile (multi-stage, distroless), `workflows.yaml` (§9), `Cargo.toml` metadata (license=AGPL-3.0 to match studio default — confirm), `deploy/` kustomize stubs for `sbbb/base/meet/`. Reads §9, §10.

Coordinator (me) enforces Phase 3 gates before any commit, and drives to `v0.1.0`.

## 12. Open questions — answer before Phase 1

1. **License.** Studio default is AGPL-3.0 (per `sol`, `wfe`). Confirm for `sunbeam-meet` or pick another.
2. **Agent-callback auth.** mTLS (preferred, infra already exists) vs per-worker HMAC header. Default: mTLS.
3. **Coverage threshold.** 85% inherited from estafeta. Lower for streaming-heavy code? Default: keep 85.
4. **Connect-rust version.** Pinned or latest? Default: latest release, `Cargo.lock` committed.
5. **Cargo workspace members for `publish = ["sunbeam"]`.** Confirm proto + migrations publish; server does not.
6. **Shared sunbeam dev-service endpoints.** Integration CI needs a secrets bundle from `wfe-core` conventions — confirm the secret name/format we inject.
7. **Connect-web path prefix.** Default `/sunbeam.meet.v1.MeetService/...`. Keep or mount under `/api/v1`?

---

**Next step:** reply with approvals / overrides for §12, and I'll kick off Phase 1 scaffold + Phase 2 parallel fan-out.
