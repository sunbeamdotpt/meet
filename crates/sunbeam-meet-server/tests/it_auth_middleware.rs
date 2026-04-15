//! Integration test — auth middleware (`src/middleware/auth.rs`).
//!
//! Runs under `cargo nextest run --profile integration`.
//!
//! Required env vars: all `INTEGRATION_ENV_VARS` (test-runner sets them).
//!
//! Strategy:
//!   - `extract_auth` is pure; tested without I/O by inserting metadata into a
//!     tonic Request.
//!   - `identity_from_header` and `identity_from_request` are tested against a
//!     real `SharedState` wired to the running Kratos instance.
//!   - Sessions are created via the Kratos admin API (same helper used in
//!     `it_kratos_session`).
//!   - Every assertion is on structured values — no `contains`, no
//!     string-matching on `tonic::Status` messages.
//!
//! CLAUDE.md: no mocks for infra, no string-matching on state, no `#[ignore]`.

mod common;

use std::sync::Arc;

use common::{env_required, unique_suffix, TestResult};
use serde_json::json;
use sunbeam_meet_server::config::{
    AgentWorkerConfig, CalDavConfig, Config, KetoConfig, KratosConfig, LiveKitConfig, S3Config,
};
use sunbeam_meet_server::error::Error;
use sunbeam_meet_server::middleware::auth::{
    extract_auth, identity_from_header, identity_from_request,
};
use sunbeam_meet_server::state::{AppState, SharedState};
use tonic::Code;

// ── Minimal AppState builder ───────────────────────────────────────────────

/// Build a `SharedState` from env vars set by docker-compose test-runner.
///
/// Every downstream service is real; this mirrors what `main.rs` does at
/// startup. The expensive part (PgPoolOptions, NATS, Valkey) is paid once per
/// test process by Tokio's runtime — nextest isolates each binary but runs
/// tests within a binary sequentially by default.
async fn shared_state() -> SharedState {
    let config = Config {
        bind_addr: "0.0.0.0:8080".into(),
        metrics_addr: "0.0.0.0:9090".into(),
        database_url: env_required("DATABASE_URL"),
        valkey_url: env_required("VALKEY_URL"),
        nats_url: env_required("NATS_URL"),
        otlp_endpoint: None,
        livekit: LiveKitConfig {
            url: env_required("LIVEKIT_URL"),
            http_url: env_required("LIVEKIT_URL"),
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
            recordings_bucket: std::env::var("S3_BUCKET")
                .unwrap_or_else(|_| "sunbeam-meet-it".into()),
            access_key: env_required("S3_ACCESS_KEY"),
            secret_key: env_required("S3_SECRET_KEY"),
            region: std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".into()),
        },
        caldav: CalDavConfig {
            url: env_required("CALDAV_URL"),
            username: std::env::var("CALDAV_USER").unwrap_or_else(|_| "admin".into()),
            password: std::env::var("CALDAV_PASSWORD").unwrap_or_else(|_| "admin".into()),
        },
        scaleway_api_key: None,
        mistral_api_key: None,
        agent_workers: Vec::<AgentWorkerConfig>::new(),
        rate_limit: sunbeam_meet_server::config::RateLimitConfig::default(),
    };

    Arc::new(
        AppState::connect(&config)
            .await
            .expect("AppState::connect must succeed for integration tests"),
    )
}

// ── Admin/flow helpers (local copy so this file is self-contained) ────────

/// Helper for Kratos admin + self-service APIs used only by tests.
struct KratosHelper {
    http: reqwest::Client,
    admin_url: String,
    public_url: String,
}

impl KratosHelper {
    fn new(admin_url: String, public_url: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            admin_url,
            public_url,
        }
    }

    /// Create a new identity with the given email + password credential.
    /// Returns the Kratos identity ID.
    async fn create_identity(
        &self,
        email: &str,
        password: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let res = self
            .http
            .post(format!("{}/admin/identities", self.admin_url))
            .json(&json!({
                "schema_id": "default",
                "traits": { "email": email },
                "credentials": {
                    "password": {
                        "config": { "password": password }
                    }
                }
            }))
            .send()
            .await?;
        assert!(
            res.status().is_success(),
            "admin create-identity returned status {}",
            res.status(),
        );
        let body: serde_json::Value = res.json().await?;
        let id = body["id"].as_str().ok_or("identity.id missing")?.to_owned();
        Ok(id)
    }

    /// Obtain a session token via the native self-service login flow.
    /// Returns `(identity_id, session_token)`.
    async fn create_identity_and_token(
        &self,
        email: &str,
    ) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync>> {
        let password = format!("Probe-{}!", unique_suffix());
        let id = self.create_identity(email, &password).await?;
        let token = self.login(email, &password).await?;
        Ok((id, token))
    }

    /// Perform a native (API-mode) login and return the session token.
    async fn login(
        &self,
        email: &str,
        password: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // Initiate API-mode login flow.
        let flow_res = self
            .http
            .get(format!("{}/self-service/login/api", self.public_url))
            .send()
            .await?;
        assert!(
            flow_res.status().is_success(),
            "login flow init returned status {}",
            flow_res.status(),
        );
        let flow_body: serde_json::Value = flow_res.json().await?;
        let flow_id = flow_body["id"]
            .as_str()
            .ok_or("login flow missing `id`")?
            .to_owned();

        // Submit credentials.
        let submit_res = self
            .http
            .post(format!(
                "{}/self-service/login?flow={flow_id}",
                self.public_url
            ))
            .json(&json!({
                "method": "password",
                "identifier": email,
                "password": password
            }))
            .send()
            .await?;
        assert!(
            submit_res.status().is_success(),
            "login flow submit returned status {}",
            submit_res.status(),
        );
        let body: serde_json::Value = submit_res.json().await?;
        let token = body["session_token"]
            .as_str()
            .ok_or("login response missing `session_token`")?
            .to_owned();
        Ok(token)
    }

    async fn delete_identity(&self, id: &str) {
        let _ = self
            .http
            .delete(format!("{}/admin/identities/{id}", self.admin_url))
            .send()
            .await;
    }
}

// ── extract_auth tests (pure, no I/O) ─────────────────────────────────────

/// `extract_auth` returns the `authorization` metadata value when present.
#[tokio::test]
async fn it_auth_middleware_extract_auth_authorization_header() -> TestResult {
    let mut req = tonic::Request::new(());
    req.metadata_mut().insert(
        "authorization",
        "Bearer test-token-xyz".parse().expect("valid ascii"),
    );

    let header = extract_auth(&req)?;
    assert_eq!(
        header, "Bearer test-token-xyz",
        "extract_auth must return the authorization header verbatim",
    );
    Ok(())
}

/// `extract_auth` falls back to the `cookie` metadata when `authorization` is absent.
#[tokio::test]
async fn it_auth_middleware_extract_auth_cookie_fallback() -> TestResult {
    let mut req = tonic::Request::new(());
    req.metadata_mut().insert(
        "cookie",
        "ory_kratos_session=abc123def".parse().expect("valid ascii"),
    );

    let header = extract_auth(&req)?;
    assert_eq!(
        header, "ory_kratos_session=abc123def",
        "extract_auth must return the cookie header when authorization is absent",
    );
    Ok(())
}

/// `extract_auth` returns `Error::Unauthenticated` when neither header is present.
#[tokio::test]
async fn it_auth_middleware_extract_auth_missing_headers_returns_unauthenticated() -> TestResult {
    let req = tonic::Request::new(());

    let err = extract_auth(&req).expect_err("missing headers must fail");
    assert!(
        matches!(err, Error::Unauthenticated),
        "expected Error::Unauthenticated, got {err:?}",
    );

    // Confirm the tonic Status code from the error.
    let status = tonic::Status::from(err);
    assert_eq!(
        status.code(),
        Code::Unauthenticated,
        "Status::code must be Unauthenticated",
    );
    Ok(())
}

/// `authorization` header takes precedence over `cookie` when both are present.
#[tokio::test]
async fn it_auth_middleware_extract_auth_authorization_takes_precedence_over_cookie() -> TestResult
{
    let mut req = tonic::Request::new(());
    req.metadata_mut()
        .insert("authorization", "Bearer wins".parse().expect("valid ascii"));
    req.metadata_mut().insert(
        "cookie",
        "ory_kratos_session=loses".parse().expect("valid ascii"),
    );

    let header = extract_auth(&req)?;
    assert_eq!(
        header, "Bearer wins",
        "authorization must be preferred over cookie",
    );
    Ok(())
}

// ── identity_from_header tests (real Kratos) ──────────────────────────────

/// A valid bearer token resolves to the correct identity via `identity_from_header`.
#[tokio::test]
async fn it_auth_middleware_identity_from_header_valid_bearer() -> TestResult {
    let state = shared_state().await;
    let helper = KratosHelper::new(
        env_required("KRATOS_ADMIN_URL"),
        env_required("KRATOS_PUBLIC_URL"),
    );

    let email = format!("it-mw-{}@sunbeam.test", unique_suffix());
    let (identity_id, token) = helper.create_identity_and_token(&email).await?;

    let bearer = format!("Bearer {token}");
    let identity = identity_from_header(&state, &bearer).await?;

    assert_eq!(
        identity.id, identity_id,
        "identity_from_header must resolve to the created identity id",
    );
    assert_eq!(
        identity.email.as_deref(),
        Some(email.as_str()),
        "identity_from_header must populate email from kratos traits",
    );

    helper.delete_identity(&identity_id).await;
    Ok(())
}

/// An invalid bearer token yields `Error::Unauthenticated` from `identity_from_header`.
#[tokio::test]
async fn it_auth_middleware_identity_from_header_invalid_token_unauthenticated() -> TestResult {
    let state = shared_state().await;

    let err = identity_from_header(&state, "Bearer totally-invalid-token-000")
        .await
        .expect_err("invalid token must fail");

    assert!(
        matches!(err, Error::Unauthenticated),
        "expected Error::Unauthenticated, got {err:?}",
    );

    // Verify the tonic Status code is correct — structured assertion.
    let status = tonic::Status::from(err);
    assert_eq!(status.code(), Code::Unauthenticated);
    Ok(())
}

// ── identity_from_request tests (real Kratos) ─────────────────────────────

/// A well-formed tonic Request with a valid bearer resolves correctly.
#[tokio::test]
async fn it_auth_middleware_identity_from_request_valid_bearer() -> TestResult {
    let state = shared_state().await;
    let helper = KratosHelper::new(
        env_required("KRATOS_ADMIN_URL"),
        env_required("KRATOS_PUBLIC_URL"),
    );

    let email = format!("it-mw-req-{}@sunbeam.test", unique_suffix());
    let (identity_id, token) = helper.create_identity_and_token(&email).await?;

    let mut req = tonic::Request::new(());
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {token}").parse().expect("valid ascii"),
    );

    let identity = identity_from_request(&state, &req).await?;

    assert_eq!(identity.id, identity_id);
    assert_eq!(identity.email.as_deref(), Some(email.as_str()));

    helper.delete_identity(&identity_id).await;
    Ok(())
}

/// A tonic Request with no auth metadata yields `Error::Unauthenticated`.
#[tokio::test]
async fn it_auth_middleware_identity_from_request_no_headers_unauthenticated() -> TestResult {
    let state = shared_state().await;

    let req = tonic::Request::new(());
    let err = identity_from_request(&state, &req)
        .await
        .expect_err("missing auth must fail");

    assert!(
        matches!(err, Error::Unauthenticated),
        "expected Error::Unauthenticated, got {err:?}",
    );
    let status = tonic::Status::from(err);
    assert_eq!(status.code(), Code::Unauthenticated);
    Ok(())
}

/// A tonic Request with an invalid token yields `Error::Unauthenticated`.
#[tokio::test]
async fn it_auth_middleware_identity_from_request_invalid_token_unauthenticated() -> TestResult {
    let state = shared_state().await;

    let mut req = tonic::Request::new(());
    req.metadata_mut().insert(
        "authorization",
        "Bearer bad-token-xxxxxx".parse().expect("valid ascii"),
    );

    let err = identity_from_request(&state, &req)
        .await
        .expect_err("invalid token must fail");

    assert!(
        matches!(err, Error::Unauthenticated),
        "expected Error::Unauthenticated, got {err:?}",
    );
    let status = tonic::Status::from(err);
    assert_eq!(status.code(), Code::Unauthenticated);
    Ok(())
}
