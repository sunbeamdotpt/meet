//! Integration test — `AppState::connect` smoke test.
//!
//! Required env: `DATABASE_URL`, `VALKEY_URL`, `NATS_URL`, `LIVEKIT_URL`,
//! `LIVEKIT_API_KEY`, `LIVEKIT_API_SECRET`, `KETO_READ_URL`,
//! `KETO_WRITE_URL`, `KRATOS_PUBLIC_URL`, `KRATOS_ADMIN_URL`, `CALDAV_URL`.
//!
//! Calls `AppState::connect` against real compose services, then pings each
//! subsystem to confirm the pool/client is live. Asserts on structured
//! responses — never on display strings.

mod common;

use common::{env_or, env_required, TestResult};
use sunbeam_meet_server::config::{
    CalDavConfig, Config, KetoConfig, KratosConfig, LiveKitConfig, S3Config,
};
use sunbeam_meet_server::state::AppState;

fn integration_config() -> Config {
    Config {
        bind_addr: "0.0.0.0:0".into(),
        metrics_addr: "0.0.0.0:0".into(),
        database_url: env_required("DATABASE_URL"),
        valkey_url: env_required("VALKEY_URL"),
        nats_url: env_required("NATS_URL"),
        otlp_endpoint: None,
        livekit: LiveKitConfig {
            url: env_required("LIVEKIT_URL"),
            http_url: env_required("LIVEKIT_URL")
                .replacen("wss://", "https://", 1)
                .replacen("ws://", "http://", 1),
            api_key: env_required("LIVEKIT_API_KEY"),
            api_secret: env_required("LIVEKIT_API_SECRET"),
        },
        kratos: KratosConfig {
            public_url: env_required("KRATOS_PUBLIC_URL"),
            admin_url: env_required("KRATOS_ADMIN_URL"),
        },
        keto: KetoConfig {
            read_url: env_required("KETO_READ_URL"),
            write_url: env_required("KETO_WRITE_URL"),
        },
        s3: S3Config {
            endpoint: env_required("S3_ENDPOINT"),
            recordings_bucket: env_or("S3_BUCKET", "sunbeam-meet-it"),
            access_key: env_required("S3_ACCESS_KEY"),
            secret_key: env_required("S3_SECRET_KEY"),
            region: env_or("S3_REGION", "us-east-1"),
        },
        caldav: CalDavConfig {
            url: env_required("CALDAV_URL"),
            username: env_or("CALDAV_USER", "admin"),
            password: env_or("CALDAV_PASSWORD", "admin"),
        },
        scaleway_api_key: None,
        mistral_api_key: None,
        agent_workers: vec![],
        rate_limit: sunbeam_meet_server::config::RateLimitConfig::default(),
    }
}

// ── smoke tests ───────────────────────────────────────────────────────────

/// `AppState::connect` must succeed and migrations must run without error.
#[tokio::test]
async fn app_state_connect_succeeds() -> TestResult {
    let cfg = integration_config();
    let state = AppState::connect(&cfg).await?;
    // If we got here without error the connection pool was established and
    // migrations ran. Assert the config round-trips correctly.
    assert_eq!(state.config.nats_url, cfg.nats_url, "config round-trip");
    Ok(())
}

/// Postgres pool must respond to a trivial query.
#[tokio::test]
async fn postgres_pool_is_live() -> TestResult {
    let cfg = integration_config();
    let state = AppState::connect(&cfg).await?;

    let one: i32 = sqlx::query_scalar("SELECT 1").fetch_one(&state.db).await?;
    assert_eq!(one, 1, "SELECT 1 must return 1");
    Ok(())
}

/// Postgres pool must see the migrated schema (tables exist).
#[tokio::test]
async fn postgres_schema_is_migrated() -> TestResult {
    let cfg = integration_config();
    let state = AppState::connect(&cfg).await?;

    // `information_schema.tables` is always present in Postgres. We check that
    // our migrations created the `rooms` table.
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM information_schema.tables
             WHERE table_schema = 'public' AND table_name = 'rooms'
         )",
    )
    .fetch_one(&state.db)
    .await?;
    assert!(exists, "rooms table must exist after migration");
    Ok(())
}

/// Valkey client must respond to `PING`.
#[tokio::test]
async fn valkey_client_is_live() -> TestResult {
    let cfg = integration_config();
    let state = AppState::connect(&cfg).await?;

    // The Valkey client's `set_presence` is the simplest real-world call.
    // We use a throwaway key so the test is idempotent.
    state
        .valkey
        .set_presence("state-smoke-test", "probe", 0)
        .await?;
    state
        .valkey
        .clear_presence("state-smoke-test", "probe")
        .await?;
    Ok(())
}

/// NATS client must be able to publish a message without error.
#[tokio::test]
async fn nats_client_is_live() -> TestResult {
    let cfg = integration_config();
    let state = AppState::connect(&cfg).await?;

    // Publish to a throwaway subject. No subscriber — that's fine; NATS is
    // fire-and-forget. We assert only that the publish call succeeds.
    state
        .nats
        .publish("meet.probe.state-connect", bytes::Bytes::from("ping"))
        .await?;
    Ok(())
}

/// NATS publish + subscribe round-trip through the connected state.
#[tokio::test]
async fn nats_publish_subscribe_round_trip() -> TestResult {
    use futures::StreamExt as _;
    use prost::Message as _;
    use std::time::Duration;
    use sunbeam_meet_proto::meet::v1::{meet_server_message::Payload, MeetServerMessage, Pong};
    use tokio::time::timeout;

    let cfg = integration_config();
    let state = AppState::connect(&cfg).await?;

    let subject = format!("meet.probe.roundtrip.{}", uuid::Uuid::now_v7());
    let mut sub = state.nats.subscribe(&subject).await?;

    let msg = MeetServerMessage {
        payload: Some(Payload::Pong(Pong { timestamp: 42 })),
    };
    state
        .nats
        .publish(subject.clone(), msg.encode_to_vec().into())
        .await?;

    let raw = timeout(Duration::from_secs(5), sub.next())
        .await
        .expect("NATS must deliver within 5 s")
        .expect("subscriber must yield a message");

    let decoded = MeetServerMessage::decode(raw.payload.as_ref())?;
    let Some(Payload::Pong(p)) = decoded.payload else {
        panic!("expected Pong payload, got {:?}", decoded.payload);
    };
    assert_eq!(p.timestamp, 42, "Pong timestamp");
    Ok(())
}

/// Hub is initialised empty — `room_exists` on an unseen room returns false.
#[tokio::test]
async fn hub_starts_empty() -> TestResult {
    let cfg = integration_config();
    let state = AppState::connect(&cfg).await?;
    assert!(
        !state.hub.room_exists("nonexistent-room-xyz").await,
        "freshly connected hub must not have a room registered",
    );
    Ok(())
}

/// Agent pool starts empty (no agent_workers in config).
#[tokio::test]
async fn agent_pool_starts_empty_when_no_workers_configured() -> TestResult {
    use sunbeam_meet_proto::agent::v1::AgentKind;

    let cfg = integration_config();
    let state = AppState::connect(&cfg).await?;

    let res = state.agents.pick(AgentKind::WhisperStt).await;
    assert!(
        res.is_err(),
        "empty agent pool must return an error on pick"
    );
    Ok(())
}
