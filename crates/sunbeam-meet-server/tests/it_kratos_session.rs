//! Integration test — Kratos identity client (`KratosClient::whoami`).
//!
//! Runs under `cargo nextest run --profile integration`.
//!
//! Required env vars: `KRATOS_PUBLIC_URL`, `KRATOS_ADMIN_URL`.
//!
//! Strategy:
//!   1. Create a fresh identity via the Kratos admin API (with a password
//!      credential embedded in the create call).
//!   2. Obtain a session token via the native self-service login flow:
//!      - `GET  {public_url}/self-service/login/api`   → flow_id
//!      - `POST {public_url}/self-service/login?flow={flow_id}`  → session_token
//!   3. Drive `KratosClient::whoami` with Bearer-prefixed and lowercase-bearer
//!      forms of the token.
//!   4. Assert on structured `Identity` fields — never substring-match on state.
//!   5. Assert unauthenticated paths return `Error::Unauthenticated`.
//!
//! CLAUDE.md: no mocks for infra, parse structured fields, no string-matching,
//! no `#[ignore]`.

mod common;

use common::{env_required, unique_suffix, TestResult};
use sunbeam_meet_server::clients::kratos::KratosClient;
use sunbeam_meet_server::config::KratosConfig;
use sunbeam_meet_server::error::Error;

// ── Admin/flow helpers ─────────────────────────────────────────────────────

/// Thin wrapper for Kratos admin + public API calls used only by tests.
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

    /// Create a new Kratos identity with the given email and a password
    /// credential (required so the self-service login flow can authenticate).
    /// Returns the Kratos identity ID.
    async fn create_identity(
        &self,
        email: &str,
        password: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let res = self
            .http
            .post(format!("{}/admin/identities", self.admin_url))
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
            .await?;
        assert!(
            res.status().is_success(),
            "admin create-identity returned status {}",
            res.status(),
        );
        let body: serde_json::Value = res.json().await?;
        let id = body["id"]
            .as_str()
            .ok_or("identity.id missing from admin create response")?
            .to_owned();
        Ok(id)
    }

    /// Obtain a session token for the given credentials via the native
    /// (API-mode, browser-less) self-service login flow.
    ///
    /// Returns the `ory_st_*` session token string.
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
            .ok_or("login flow response missing `id` field")?
            .to_owned();

        // Submit credentials.
        let submit_res = self
            .http
            .post(format!(
                "{}/self-service/login?flow={flow_id}",
                self.public_url
            ))
            .json(&serde_json::json!({
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
        let submit_body: serde_json::Value = submit_res.json().await?;
        let token = submit_body["session_token"]
            .as_str()
            .ok_or("login response missing `session_token` field")?
            .to_owned();
        Ok(token)
    }

    /// Delete an identity by id (best-effort cleanup).
    async fn delete_identity(&self, id: &str) {
        let _ = self
            .http
            .delete(format!("{}/admin/identities/{id}", self.admin_url))
            .send()
            .await;
    }
}

// ── Fixtures ───────────────────────────────────────────────────────────────

struct SessionFixture {
    helper: KratosHelper,
    /// Kratos identity ID of the created identity.
    pub identity_id: String,
    /// Raw session token (`ory_st_*`), no prefix.
    pub token: String,
    /// The email used to create the identity.
    pub email: String,
}

impl SessionFixture {
    async fn create() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let admin_url = env_required("KRATOS_ADMIN_URL");
        let public_url = env_required("KRATOS_PUBLIC_URL");
        let helper = KratosHelper::new(admin_url, public_url);

        let email = format!("it-kratos-{}@sunbeam.test", unique_suffix());
        // Password must satisfy minimum Kratos requirements (8+ chars).
        let password = format!("Probe-{}!", unique_suffix());

        let identity_id = helper.create_identity(&email, &password).await?;
        let token = helper.login(&email, &password).await?;

        Ok(Self {
            helper,
            identity_id,
            token,
            email,
        })
    }

    async fn cleanup(self) {
        self.helper.delete_identity(&self.identity_id).await;
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

/// `whoami` resolves a valid bearer token to the correct `Identity`.
#[tokio::test]
async fn it_kratos_whoami_bearer_returns_identity() -> TestResult {
    let public_url = env_required("KRATOS_PUBLIC_URL");
    let fix = SessionFixture::create().await?;

    let client = KratosClient::new(&KratosConfig {
        public_url,
        admin_url: env_required("KRATOS_ADMIN_URL"),
    });

    // `whoami` accepts `Bearer <token>` form.
    let bearer = format!("Bearer {}", fix.token);
    let identity = client.whoami(&bearer).await?;

    // Assert on structured fields — never substring-match.
    assert_eq!(
        identity.id, fix.identity_id,
        "returned identity id must match the one we created",
    );
    assert_eq!(
        identity.email.as_deref(),
        Some(fix.email.as_str()),
        "returned email must match the trait we set",
    );
    // The integration test schema only has `email`; no `name`/`username` trait.
    assert!(
        identity.display_name.is_none(),
        "display_name must be None when neither `name` nor `username` trait is present",
    );

    fix.cleanup().await;
    Ok(())
}

/// `whoami` detects the `bearer ` prefix in lowercase and sends as Authorization header.
#[tokio::test]
async fn it_kratos_whoami_lowercase_bearer_prefix_accepted() -> TestResult {
    let public_url = env_required("KRATOS_PUBLIC_URL");
    let fix = SessionFixture::create().await?;

    let client = KratosClient::new(&KratosConfig {
        public_url,
        admin_url: env_required("KRATOS_ADMIN_URL"),
    });

    // Lowercase `bearer ` prefix — exercises the second branch in `whoami`.
    let bearer = format!("bearer {}", fix.token);
    let identity = client.whoami(&bearer).await?;

    assert_eq!(identity.id, fix.identity_id);
    assert_eq!(identity.email.as_deref(), Some(fix.email.as_str()));

    fix.cleanup().await;
    Ok(())
}

/// `whoami` with a missing / empty auth string returns `Error::Unauthenticated`.
#[tokio::test]
async fn it_kratos_whoami_missing_token_returns_unauthenticated() -> TestResult {
    let public_url = env_required("KRATOS_PUBLIC_URL");

    let client = KratosClient::new(&KratosConfig {
        public_url,
        admin_url: env_required("KRATOS_ADMIN_URL"),
    });

    // An empty string falls into the cookie path; Kratos rejects a missing/blank cookie.
    let err = client.whoami("").await.expect_err("empty auth must fail");
    assert!(
        matches!(err, Error::Unauthenticated),
        "expected Error::Unauthenticated for empty auth, got {err:?}",
    );

    Ok(())
}

/// `whoami` with a well-formed but invalid token returns `Error::Unauthenticated`.
#[tokio::test]
async fn it_kratos_whoami_invalid_token_returns_unauthenticated() -> TestResult {
    let public_url = env_required("KRATOS_PUBLIC_URL");

    let client = KratosClient::new(&KratosConfig {
        public_url,
        admin_url: env_required("KRATOS_ADMIN_URL"),
    });

    let err = client
        .whoami("Bearer not-a-real-token-xxxxxxxxxxxxxxxxxxx")
        .await
        .expect_err("invalid token must fail");

    assert!(
        matches!(err, Error::Unauthenticated),
        "expected Error::Unauthenticated for invalid token, got {err:?}",
    );

    Ok(())
}

/// Two distinct sessions resolve to independent `Identity` values.
///
/// Guards against session state cross-contamination between calls.
#[tokio::test]
async fn it_kratos_whoami_distinct_sessions_resolve_independently() -> TestResult {
    let public_url = env_required("KRATOS_PUBLIC_URL");
    let admin_url = env_required("KRATOS_ADMIN_URL");

    let client = KratosClient::new(&KratosConfig {
        public_url: public_url.clone(),
        admin_url: admin_url.clone(),
    });

    let fix_a = SessionFixture::create().await?;
    let fix_b = SessionFixture::create().await?;

    let id_a = client.whoami(&format!("Bearer {}", fix_a.token)).await?;
    let id_b = client.whoami(&format!("Bearer {}", fix_b.token)).await?;

    // Each session resolves to its own identity.
    assert_ne!(
        id_a.id, id_b.id,
        "two different sessions must not resolve to the same identity",
    );
    assert_eq!(id_a.id, fix_a.identity_id);
    assert_eq!(id_b.id, fix_b.identity_id);
    assert_eq!(id_a.email.as_deref(), Some(fix_a.email.as_str()));
    assert_eq!(id_b.email.as_deref(), Some(fix_b.email.as_str()));

    fix_a.cleanup().await;
    fix_b.cleanup().await;
    Ok(())
}
