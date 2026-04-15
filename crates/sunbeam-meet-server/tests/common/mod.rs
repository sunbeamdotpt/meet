//! Shared helpers for sunbeam-meet test suites.
//!
//! This module is included from both `unit_*.rs` and `it_*.rs` test binaries
//! via `mod common;`. Because Cargo compiles each `tests/*.rs` file as its
//! own binary, some helpers may appear unused in a given binary — that's
//! fine, `#[allow(dead_code)]` on the module suppresses the warning.
#![allow(dead_code)]

pub mod publisher;

use std::time::Duration;

/// Result alias used throughout the test suites.
pub type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;

/// Read an env var required for an integration test. Panics with a clear
/// message if missing — nextest surfaces the panic as the test failure, which
/// is exactly what we want: running `cargo nextest run --profile integration`
/// without the env var set should fail loudly rather than silently skip.
pub fn env_required(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| {
        panic!(
            "integration test requires env var `{key}` — see \
             tests/it_*.rs header comment for the full list"
        )
    })
}

/// Read an env var, or return a default. Used for endpoints that have a
/// conventional local-dev value.
pub fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Short, unique test suffix for ephemeral resource names (rooms, calendar
/// event UIDs, etc.). Keeps parallel test runs from colliding.
pub fn unique_suffix() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{pid}-{ts}-{n}")
}

/// A plain-ASCII, slug-friendly unique name.
pub fn unique_room_slug(prefix: &str) -> String {
    let suffix = unique_suffix().replace(['-', '.'], "");
    format!("{prefix}-{suffix}")
}

/// Default timeout for waiting on real-service effects (DB replication, S3
/// object visibility, LiveKit egress state, etc.).
pub const DEFAULT_WAIT: Duration = Duration::from_secs(30);

/// Poll `f` until it returns `Ok(Some(T))` or `timeout` elapses. Returns the
/// first `Some`, or an error if it never materialises. Fails with structured
/// context — never by string-matching on state.
pub async fn wait_for<F, Fut, T>(timeout: Duration, mut f: F) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<Option<T>, String>>,
{
    let start = std::time::Instant::now();
    let mut attempts: u32 = 0;
    loop {
        attempts += 1;
        match f().await {
            Ok(Some(v)) => return Ok(v),
            Ok(None) => {}
            Err(e) => {
                return Err(format!(
                    "wait_for: probe errored after {attempts} attempts: {e}"
                ))
            }
        }
        if start.elapsed() >= timeout {
            return Err(format!(
                "wait_for: predicate did not become true within {timeout:?} ({attempts} attempts)"
            ));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

// ── External service connection helpers ────────────────────────────────
// These return real clients wired to the shared sunbeam dev services per
// CLAUDE.md. No mocks. The integration profile assumes the env vars below
// are set; the default profile never calls these.

/// All env vars required for the `integration` nextest profile. Test headers
/// reference this list so the doc-writer can surface it in the README.
pub const INTEGRATION_ENV_VARS: &[&str] = &[
    "DATABASE_URL",
    "VALKEY_URL",
    "NATS_URL",
    "S3_ENDPOINT",
    "S3_ACCESS_KEY",
    "S3_SECRET_KEY",
    "CALDAV_URL",
    "KRATOS_PUBLIC_URL",
    "KRATOS_ADMIN_URL",
    "KETO_READ_URL",
    "KETO_WRITE_URL",
    "LIVEKIT_URL",
    "LIVEKIT_API_KEY",
    "LIVEKIT_API_SECRET",
];

/// Returns true if all integration env vars are set. Tests can branch on this
/// for early-fail diagnostics, but the profile filter is the primary gate.
pub fn integration_env_ready() -> bool {
    INTEGRATION_ENV_VARS
        .iter()
        .all(|k| std::env::var(k).is_ok())
}

/// Connect to the integration Postgres and apply migrations exactly once per
/// test process. Subsequent calls return fresh pools but skip migration
/// (sqlx tracks state in `_sqlx_migrations`, so it would be a no-op anyway —
/// the OnceCell just avoids the round-trip).
pub async fn pg_pool() -> sqlx::PgPool {
    use std::sync::OnceLock;
    use tokio::sync::Mutex;

    static MIGRATED: OnceLock<Mutex<bool>> = OnceLock::new();

    let url = env_required("DATABASE_URL");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("connect Postgres");

    let mu = MIGRATED.get_or_init(|| Mutex::new(false));
    let mut done = mu.lock().await;
    if !*done {
        sunbeam_meet_migrations::MIGRATOR
            .run(&pool)
            .await
            .expect("apply sunbeam-meet migrations");
        *done = true;
    }
    pool
}

// ── Handler-level integration helpers ──────────────────────────────────────
// These helpers connect to every downstream service and return the
// `SharedState` that handler free-functions accept. They also provide
// short-lived Kratos identities and session tokens for tests that drive
// the auth-gated RPCs.

use std::sync::Arc;
use sunbeam_meet_server::state::{AppState, SharedState};

/// Build a `SharedState` wired to all integration services. Reads config from
/// the same env vars used by the test-runner container.
///
/// Repeated calls within the same nextest process each return a fresh
/// `Arc<AppState>` sharing the same underlying connections — creating a new
/// pool per test is cheap relative to the network round-trips that follow.
pub async fn shared_state() -> SharedState {
    use sunbeam_meet_server::config::{
        AgentWorkerConfig, CalDavConfig, Config, KetoConfig, KratosConfig, LiveKitConfig, S3Config,
    };

    let livekit_url = env_required("LIVEKIT_URL");
    // LiveKit HTTP URL for admin API: swap ws→http.
    let livekit_http_url = livekit_url
        .replacen("wss://", "https://", 1)
        .replacen("ws://", "http://", 1);

    // CalDAV URL in the compose env embeds basic-auth as userinfo
    // (e.g. `http://admin:admin@stalwart:8080/dav/cal/…`).  CalDavConfig
    // stores credentials split from the base URL.
    let caldav_raw = env_required("CALDAV_URL");
    let caldav_parsed = reqwest::Url::parse(&caldav_raw).expect("CALDAV_URL must be a valid URL");
    let caldav_username = caldav_parsed.username().to_owned();
    let caldav_password = caldav_parsed.password().unwrap_or("").to_owned();
    let mut caldav_clean = caldav_parsed.clone();
    let _ = caldav_clean.set_username("");
    let _ = caldav_clean.set_password(None);
    let caldav_url = caldav_clean.to_string().trim_end_matches('/').to_owned();

    let cfg = Config {
        bind_addr: "0.0.0.0:8080".into(),
        metrics_addr: "0.0.0.0:9090".into(),
        database_url: env_required("DATABASE_URL"),
        valkey_url: env_required("VALKEY_URL"),
        nats_url: env_required("NATS_URL"),
        otlp_endpoint: None,
        livekit: LiveKitConfig {
            url: livekit_url,
            http_url: livekit_http_url,
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
            endpoint: env_or("S3_ENDPOINT", "http://seaweedfs:8333"),
            recordings_bucket: env_or("S3_BUCKET", "sunbeam-meet-it"),
            access_key: env_or("S3_ACCESS_KEY", "any"),
            secret_key: env_or("S3_SECRET_KEY", "any"),
            region: env_or("S3_REGION", "us-east-1"),
        },
        caldav: CalDavConfig {
            url: caldav_url,
            username: caldav_username,
            password: caldav_password,
        },
        scaleway_api_key: None,
        mistral_api_key: None,
        agent_workers: Vec::<AgentWorkerConfig>::new(),
        rate_limit: sunbeam_meet_server::config::RateLimitConfig::default(),
    };

    Arc::new(
        AppState::connect(&cfg)
            .await
            .expect("AppState::connect for integration tests"),
    )
}

/// A real Kratos identity + bearer token valid for the lifetime of one test.
///
/// Uses the Kratos **admin** API to create an identity and immediately issue
/// a session token — no browser flow or selfservice UI needed. The token is
/// returned as a `Bearer <token>` string ready to drop into tonic metadata.
///
/// Lifecycle: identities accumulate in the Kratos SQLite DB for the
/// docker-compose lifetime. `docker compose down -v` wipes the volume.
/// Within a run no explicit deletion is needed — the identity count is tiny
/// and the per-test suffix guarantees uniqueness.
pub struct KratosSession {
    /// Kratos identity UUID.
    pub identity_id: String,
    /// `Bearer <token>` string — drop into `authorization` tonic metadata.
    pub bearer: String,
}

/// Create one ephemeral Kratos identity + session.
///
/// Strategy:
///   1. Create the identity via the Kratos admin API (`POST /admin/identities`)
///      with a password credential.
///   2. Obtain a session token via the Kratos **selfservice login API** flow:
///      a. `GET  {public}/self-service/login/api`         → flow id
///      b. `POST {public}/self-service/login?flow={id}`   → session_token
///
/// The identity schema (`kratos-identity.schema.json`) requires only `email`
/// as a trait. We derive a unique address from `unique_suffix()` so parallel
/// tests never collide on the unique-email constraint.
pub async fn kratos_session() -> KratosSession {
    let admin_url = env_required("KRATOS_ADMIN_URL");
    let public_url = env_required("KRATOS_PUBLIC_URL");
    let email = format!("it-{}@sunbeam.test", unique_suffix());
    // Password must be ≥8 chars with at least one symbol for Kratos defaults.
    let password = format!("Sunbeam!{}", unique_suffix());
    let http = reqwest::Client::new();

    // 1. Create identity with password credentials via admin API.
    let create_resp = http
        .post(format!("{admin_url}/admin/identities"))
        .json(&serde_json::json!({
            "schema_id": "default",
            "traits": { "email": email },
            "credentials": {
                "password": {
                    "config": { "password": password }
                }
            }
        }))
        .send()
        .await
        .expect("kratos POST /admin/identities");

    let status = create_resp.status();
    let body_text = create_resp.text().await.unwrap_or_default();
    assert!(
        status.is_success(),
        "kratos create identity failed {status}: {body_text}"
    );

    let create_body: serde_json::Value =
        serde_json::from_str(&body_text).expect("parse kratos identity JSON");
    let identity_id = create_body["id"]
        .as_str()
        .expect("kratos identity response must have 'id' field")
        .to_owned();

    // 2a. Initialise a selfservice login flow (API flow — no browser redirect).
    let flow_resp = http
        .get(format!("{public_url}/self-service/login/api"))
        .send()
        .await
        .expect("kratos GET /self-service/login/api");

    let flow_status = flow_resp.status();
    let flow_text = flow_resp.text().await.unwrap_or_default();
    assert!(
        flow_status.is_success(),
        "kratos init login flow failed {flow_status}: {flow_text}"
    );

    let flow_body: serde_json::Value =
        serde_json::from_str(&flow_text).expect("parse kratos login flow JSON");
    let flow_id = flow_body["id"]
        .as_str()
        .expect("kratos login flow must have 'id'")
        .to_owned();

    // 2b. Submit the password method to complete the flow.
    let login_resp = http
        .post(format!("{public_url}/self-service/login?flow={flow_id}"))
        .json(&serde_json::json!({
            "method": "password",
            "identifier": email,
            "password": password,
        }))
        .send()
        .await
        .expect("kratos POST /self-service/login");

    let login_status = login_resp.status();
    let login_text = login_resp.text().await.unwrap_or_default();
    assert!(
        login_status.is_success(),
        "kratos complete login flow failed {login_status}: {login_text}"
    );

    let login_body: serde_json::Value =
        serde_json::from_str(&login_text).expect("parse kratos login response JSON");
    // The selfservice login API flow returns `session_token` at the top level.
    let token = login_body["session_token"]
        .as_str()
        .expect("kratos login response must carry 'session_token'")
        .to_owned();

    KratosSession {
        identity_id,
        bearer: format!("Bearer {token}"),
    }
}

/// Build a tonic `Request<T>` carrying an `authorization` metadata header.
///
/// Handlers call `identity(state, &req)` which reads the `authorization`
/// header and resolves it via Kratos `whoami`. Pass the `bearer` string from
/// [`KratosSession`] here.
pub fn authed_request<T>(payload: T, bearer: &str) -> tonic::Request<T> {
    let mut req = tonic::Request::new(payload);
    req.metadata_mut().insert(
        "authorization",
        bearer
            .parse()
            .expect("bearer header value must be valid ASCII"),
    );
    req
}
