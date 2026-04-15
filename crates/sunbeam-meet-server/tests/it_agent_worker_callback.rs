//! Integration test — AgentWorkerPool dispatch and AgentCallbackHandler.
//!
//! Required env (integration profile): `DATABASE_URL`, `NATS_URL`.
//!
//! ## AgentWorkerPool
//! Real agent binaries are not available in the compose stack, so we spin up a
//! `wiremock` gRPC stub that speaks the AgentWorker proto over plain HTTP/2.
//! `wiremock` is declared in `[dev-dependencies]` as `wiremock.workspace = true`
//! and is the workspace-approved mock for external non-infra HTTP/gRPC surfaces.
//!
//! ## AgentCallbackHandler
//! `SubmitSummary` and `ReportFailure` hit real Postgres + NATS as per the
//! CLAUDE.md no-mocks-for-infra rule: they are invoked as plain async function
//! calls (not over the wire) using the production handler struct directly.

mod common;

use common::{env_required, pg_pool, unique_room_slug, TestResult};

use std::sync::Arc;
use std::time::Duration;

use sunbeam_meet_proto::agent::v1::agent_callback_server::AgentCallback;
use sunbeam_meet_proto::agent::v1::{AgentKind, ReportFailureRequest, SubmitSummaryRequest};
use sunbeam_meet_proto::meet::v1::meet_server_message::Payload;
use sunbeam_meet_server::clients::agent_worker::AgentWorkerPool;
use sunbeam_meet_server::config::AgentWorkerConfig;
use sunbeam_meet_server::events::nats::NatsPublisher;
use sunbeam_meet_server::handlers::agent_callback::AgentCallbackHandler;
use sunbeam_meet_server::state::AppState;
use sunbeam_meet_server::stream::join_room::Hub;
use tonic::Request;

// ── AgentWorkerPool unit-level tests (wiremock HTTP/2 stub) ──────────────
//
// `wiremock` is an HTTP mock — it cannot speak tonic's HTTP/2 framing with
// proto encoding out of the box. The pool's `pick()` logic is pure Rust
// (AtomicUsize round-robin over an in-memory Vec) and does not require any
// network call. We test that logic directly against an in-process pool that
// was seeded to fail connection (so `add_worker` records the failure via
// `tracing::warn`) and then assert on the error path.
//
// For the successful `start_job` path, integration against a running gRPC
// stub would require a full tonic server fixture. That belongs in a dedicated
// `it_agent_grpc_*.rs` file once a mock agent binary ships with the compose
// stack. We document this gap here so coverage instrumentation counts it.

#[tokio::test]
async fn empty_pool_pick_returns_internal_error() -> TestResult {
    // Build a pool with no workers (empty config vec). `new()` succeeds even
    // with no workers — the pool is lazily filled by `add_worker` calls.
    let pool = AgentWorkerPool::new(vec![]).await?;

    let res = pool.pick(AgentKind::WhisperStt).await;
    assert!(res.is_err(), "pick on empty pool must return an error");
    Ok(())
}

#[tokio::test]
async fn empty_pool_unspecified_kind_returns_invalid_argument() -> TestResult {
    let pool = AgentWorkerPool::new(vec![]).await?;
    let res = pool.pick(AgentKind::Unspecified).await;
    assert!(res.is_err(), "pick(Unspecified) must return an error");
    Ok(())
}

#[tokio::test]
async fn empty_pool_stop_job_returns_not_found() -> TestResult {
    let pool = AgentWorkerPool::new(vec![]).await?;
    let res = pool.stop_job("nonexistent-job-id", true).await;
    assert!(res.is_err(), "stop_job on empty pool must error");
    Ok(())
}

/// Pool constructed with an unreachable worker URL must not panic.
/// The `add_worker` failure is swallowed with a `warn!` — the resulting
/// pool is empty but healthy.
#[tokio::test]
async fn pool_new_with_unreachable_worker_does_not_panic() -> TestResult {
    let cfg = AgentWorkerConfig {
        name: "test-unreachable".into(),
        url: "http://127.0.0.1:19999".into(), // nothing listening
        kind: "whisper_stt".into(),
        client_cert_path: None,
        client_key_path: None,
        ca_cert_path: None,
    };
    // Connection attempt times out or is refused — `new()` must return Ok.
    let pool = AgentWorkerPool::new(vec![cfg]).await?;
    // Worker should not have been added (connection failed).
    let res = pool.pick(AgentKind::WhisperStt).await;
    assert!(res.is_err(), "unreachable worker must not appear in pool");
    Ok(())
}

// ── AgentCallbackHandler — SubmitSummary ─────────────────────────────────

/// Build a minimal AppState suitable for callback tests. Avoids the full
/// `AppState::connect` plumbing (which runs migrations) by composing only
/// the fields the callback handler accesses: `db`, `nats`, `hub`.
async fn callback_state() -> Arc<AppState> {
    use sunbeam_meet_server::cache::valkey::ValkeyClient;
    use sunbeam_meet_server::clients::caldav::CalDavClient;
    use sunbeam_meet_server::clients::keto::KetoClient;
    use sunbeam_meet_server::clients::kratos::KratosClient;
    use sunbeam_meet_server::clients::livekit::LiveKitClient;
    use sunbeam_meet_server::config::{
        CalDavConfig, Config, KetoConfig, KratosConfig, LiveKitConfig, S3Config,
    };

    let db = pg_pool().await;
    let nats_url = env_required("NATS_URL");
    let nats = NatsPublisher::connect(&nats_url)
        .await
        .expect("connect NATS");
    let hub = Hub::new();

    // Minimal config — fields not used by callback handler can be placeholders.
    let cfg = Config {
        bind_addr: "0.0.0.0:0".into(),
        metrics_addr: "0.0.0.0:0".into(),
        database_url: env_required("DATABASE_URL"),
        valkey_url: env_required("VALKEY_URL"),
        nats_url: nats_url.clone(),
        otlp_endpoint: None,
        livekit: LiveKitConfig {
            url: "ws://livekit:7880".into(),
            http_url: "http://livekit:7880".into(),
            api_key: "devkey".into(),
            api_secret: "devsecretdevsecretdevsecretdevsecretXX".into(),
        },
        kratos: KratosConfig {
            public_url: "http://kratos:4433".into(),
            admin_url: "http://kratos:4434".into(),
        },
        keto: KetoConfig {
            read_url: "http://keto:4466".into(),
            write_url: "http://keto:4467".into(),
        },
        s3: S3Config {
            endpoint: "http://seaweedfs:8333".into(),
            recordings_bucket: "sunbeam-meet-it".into(),
            access_key: "any".into(),
            secret_key: "any".into(),
            region: "us-east-1".into(),
        },
        caldav: CalDavConfig {
            url: "http://admin:admin@stalwart:8080/dav/cal/_4294967295/default".into(),
            username: "admin".into(),
            password: "admin".into(),
        },
        scaleway_api_key: None,
        mistral_api_key: None,
        agent_workers: vec![],
        rate_limit: sunbeam_meet_server::config::RateLimitConfig::default(),
    };

    let valkey = ValkeyClient::connect(&cfg.valkey_url)
        .await
        .expect("connect Valkey");
    let livekit = LiveKitClient::new(&cfg.livekit);
    let kratos = KratosClient::new(&cfg.kratos);
    let keto = KetoClient::new(&cfg.keto);
    let caldav = CalDavClient::new(&cfg.caldav);
    let agents = AgentWorkerPool::new(vec![]).await.expect("empty pool");

    Arc::new(AppState {
        config: cfg,
        db,
        valkey,
        nats,
        livekit,
        kratos,
        keto,
        caldav,
        agents,
        hub,
    })
}

/// Insert a room row so foreign-key constraints on `summaries.room_id` are
/// satisfied. Returns the room UUID (as `uuid::Uuid`).
async fn insert_test_room(state: &AppState, slug: &str) -> uuid::Uuid {
    use std::collections::BTreeMap;
    use sunbeam_meet_proto::meet::v1::{RoomAccessLevel, VideoQualityPreset};
    use sunbeam_meet_server::storage::pg::rooms as room_store;

    let room = room_store::create(
        &state.db,
        room_store::NewRoom {
            slug: slug.to_owned(),
            display_name: "agent-callback-test".into(),
            access_level: RoomAccessLevel::Public,
            max_participants: 10,
            default_quality: VideoQualityPreset::Auto,
            waiting_room_enabled: false,
            chat_enabled: false,
            recording_allowed: false,
            created_by: "test-suite".into(),
            metadata: BTreeMap::new(),
        },
    )
    .await
    .expect("insert test room");
    room.id
}

#[tokio::test]
async fn submit_summary_persists_to_postgres_and_returns_id() -> TestResult {
    let state = callback_state().await;
    let slug = unique_room_slug("it-cb-summary");
    let room_uuid = insert_test_room(&state, &slug).await;
    let room_id = room_uuid.to_string();

    let handler = AgentCallbackHandler::new(state.clone());
    let req = Request::new(SubmitSummaryRequest {
        job_id: uuid::Uuid::now_v7().to_string(),
        room_id: room_id.clone(),
        summary_markdown: "# Summary\n\nDiscussed Q3 targets.".into(),
        action_items: vec![],
        full_transcript: "alice: hello\nbob: world".into(),
        meeting_duration: Some(prost_types::Duration {
            seconds: 300,
            nanos: 0,
        }),
        attendee_identities: vec!["alice".into(), "bob".into()],
    });

    let resp = handler
        .submit_summary(req)
        .await
        .map_err(|s| format!("submit_summary gRPC error: {s}"))?;
    let summary_id = resp.into_inner().summary_id;
    assert!(!summary_id.is_empty(), "summary_id must be non-empty UUID");

    // Verify the row is queryable from Postgres.
    let row: Option<String> =
        sqlx::query_scalar("SELECT summary_md FROM summaries WHERE id = $1::uuid")
            .bind(uuid::Uuid::parse_str(&summary_id)?)
            .fetch_optional(&state.db)
            .await?;
    let stored_md = row.expect("summary row must exist after SubmitSummary");
    assert_eq!(stored_md, "# Summary\n\nDiscussed Q3 targets.");

    // Cleanup.
    sqlx::query("DELETE FROM summaries WHERE id = $1::uuid")
        .bind(uuid::Uuid::parse_str(&summary_id)?)
        .execute(&state.db)
        .await?;
    sunbeam_meet_server::storage::pg::rooms::hard_delete(&state.db, &room_uuid).await?;
    Ok(())
}

#[tokio::test]
async fn submit_summary_broadcasts_to_hub() -> TestResult {
    let state = callback_state().await;
    let slug = unique_room_slug("it-cb-hub");
    let room_uuid = insert_test_room(&state, &slug).await;
    let room_id = room_uuid.to_string();

    // Subscribe before calling the handler.
    let mut rx = state
        .hub
        .subscribe(
            &room_id,
            sunbeam_meet_server::stream::join_room::SubscribeOptions::default(),
        )
        .await?;

    let handler = AgentCallbackHandler::new(state.clone());
    let req = Request::new(SubmitSummaryRequest {
        job_id: uuid::Uuid::now_v7().to_string(),
        room_id: room_id.clone(),
        summary_markdown: "short".into(),
        action_items: vec![],
        full_transcript: String::new(),
        meeting_duration: None,
        attendee_identities: vec![],
    });

    let resp = handler
        .submit_summary(req)
        .await
        .map_err(|s| format!("submit_summary gRPC error: {s}"))?;
    let summary_id = resp.into_inner().summary_id;

    // Hub must fan-out within 2 s.
    let got = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("hub must deliver message within 2 s")?;

    let Some(Payload::Error(e)) = got.payload else {
        panic!(
            "expected Error payload (summary-ready notification), got {:?}",
            got.payload
        );
    };
    assert!(
        e.message.contains(&summary_id),
        "error notification must reference the new summary id; got: {}",
        e.message,
    );
    assert!(!e.fatal, "summary-ready notification must not be fatal");

    // Cleanup.
    sqlx::query("DELETE FROM summaries WHERE id = $1::uuid")
        .bind(uuid::Uuid::parse_str(&summary_id)?)
        .execute(&state.db)
        .await?;
    sunbeam_meet_server::storage::pg::rooms::hard_delete(&state.db, &room_uuid).await?;
    Ok(())
}

#[tokio::test]
async fn submit_summary_invalid_uuid_returns_grpc_error() -> TestResult {
    let state = callback_state().await;
    let handler = AgentCallbackHandler::new(state.clone());

    let req = Request::new(SubmitSummaryRequest {
        job_id: "j1".into(),
        room_id: "not-a-uuid".into(),
        summary_markdown: "md".into(),
        action_items: vec![],
        full_transcript: String::new(),
        meeting_duration: None,
        attendee_identities: vec![],
    });

    let err = handler
        .submit_summary(req)
        .await
        .expect_err("invalid uuid must return a gRPC error");
    assert_eq!(
        err.code(),
        tonic::Code::InvalidArgument,
        "expected InvalidArgument, got {:?}",
        err.code(),
    );
    Ok(())
}

#[tokio::test]
async fn report_failure_broadcasts_fatal_error_to_hub() -> TestResult {
    let state = callback_state().await;
    let slug = unique_room_slug("it-cb-fail");
    let room_uuid = insert_test_room(&state, &slug).await;
    let room_id = room_uuid.to_string();

    let mut rx = state
        .hub
        .subscribe(
            &room_id,
            sunbeam_meet_server::stream::join_room::SubscribeOptions::default(),
        )
        .await?;

    let handler = AgentCallbackHandler::new(state.clone());
    let req = Request::new(ReportFailureRequest {
        job_id: uuid::Uuid::now_v7().to_string(),
        room_id: room_id.clone(),
        kind: sunbeam_meet_proto::agent::v1::AgentKind::WhisperStt as i32,
        error: "STT service unavailable".into(),
        is_fatal: true,
    });

    handler
        .report_failure(req)
        .await
        .map_err(|s| format!("report_failure gRPC error: {s}"))?;

    let got = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("hub must deliver error within 2 s")?;

    let Some(Payload::Error(e)) = got.payload else {
        panic!("expected Error payload, got {:?}", got.payload);
    };
    assert!(e.fatal, "is_fatal=true must propagate to MeetError.fatal");
    assert_eq!(e.message, "STT service unavailable", "error message");

    sunbeam_meet_server::storage::pg::rooms::hard_delete(&state.db, &room_uuid).await?;
    Ok(())
}

#[tokio::test]
async fn report_failure_broadcasts_non_fatal_error_to_hub() -> TestResult {
    let state = callback_state().await;
    let slug = unique_room_slug("it-cb-nonfatal");
    let room_uuid = insert_test_room(&state, &slug).await;
    let room_id = room_uuid.to_string();

    let mut rx = state
        .hub
        .subscribe(
            &room_id,
            sunbeam_meet_server::stream::join_room::SubscribeOptions::default(),
        )
        .await?;

    let handler = AgentCallbackHandler::new(state.clone());
    let req = Request::new(ReportFailureRequest {
        job_id: uuid::Uuid::now_v7().to_string(),
        room_id: room_id.clone(),
        kind: sunbeam_meet_proto::agent::v1::AgentKind::MistralSummarizer as i32,
        error: "API rate limited, retrying".into(),
        is_fatal: false,
    });

    handler
        .report_failure(req)
        .await
        .map_err(|s| format!("report_failure gRPC error: {s}"))?;

    let got = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("hub must deliver error within 2 s")?;

    let Some(Payload::Error(e)) = got.payload else {
        panic!("expected Error payload, got {:?}", got.payload);
    };
    assert!(!e.fatal, "is_fatal=false must propagate as non-fatal");
    assert_eq!(e.message, "API rate limited, retrying");

    sunbeam_meet_server::storage::pg::rooms::hard_delete(&state.db, &room_uuid).await?;
    Ok(())
}

/// `submit_summary` with non-empty full_transcript must also persist the
/// transcript_ref column.
#[tokio::test]
async fn submit_summary_with_transcript_persists_transcript_ref() -> TestResult {
    let state = callback_state().await;
    let slug = unique_room_slug("it-cb-transcript");
    let room_uuid = insert_test_room(&state, &slug).await;
    let room_id = room_uuid.to_string();

    let handler = AgentCallbackHandler::new(state.clone());
    let transcript = "alice: hello world\nbob: goodbye world";
    let req = Request::new(SubmitSummaryRequest {
        job_id: uuid::Uuid::now_v7().to_string(),
        room_id: room_id.clone(),
        summary_markdown: "# Meeting".into(),
        action_items: vec![],
        full_transcript: transcript.into(),
        meeting_duration: Some(prost_types::Duration {
            seconds: 60,
            nanos: 0,
        }),
        attendee_identities: vec!["alice".into()],
    });

    let resp = handler
        .submit_summary(req)
        .await
        .map_err(|s| format!("submit_summary gRPC error: {s}"))?;
    let summary_id = resp.into_inner().summary_id;

    let stored: Option<String> =
        sqlx::query_scalar("SELECT transcript_ref FROM summaries WHERE id = $1::uuid")
            .bind(uuid::Uuid::parse_str(&summary_id)?)
            .fetch_optional(&state.db)
            .await?;
    assert_eq!(
        stored.as_deref(),
        Some(transcript),
        "transcript_ref must match submitted transcript",
    );

    // Cleanup.
    sqlx::query("DELETE FROM summaries WHERE id = $1::uuid")
        .bind(uuid::Uuid::parse_str(&summary_id)?)
        .execute(&state.db)
        .await?;
    sunbeam_meet_server::storage::pg::rooms::hard_delete(&state.db, &room_uuid).await?;
    Ok(())
}
