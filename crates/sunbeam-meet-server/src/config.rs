//! Runtime configuration for sunbeam-meet, loaded via figment.

use figment::providers::{Env, Format, Toml};
use figment::Figment;
use serde::{Deserialize, Serialize};

/// Top-level configuration for the service.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// HTTP/gRPC bind address, e.g. `0.0.0.0:8080`.
    #[serde(default = "default_bind")]
    pub bind_addr: String,
    /// Prometheus metrics bind address (served as part of the main router
    /// by default; kept as a field for future split).
    #[serde(default = "default_metrics")]
    pub metrics_addr: String,
    /// PostgreSQL connection string.
    pub database_url: String,
    /// Valkey (Redis-protocol) connection string.
    pub valkey_url: String,
    /// NATS connection string.
    pub nats_url: String,
    /// OTLP collector endpoint (gRPC or HTTP).
    #[serde(default)]
    pub otlp_endpoint: Option<String>,
    /// LiveKit config.
    pub livekit: LiveKitConfig,
    /// Ory Kratos config.
    pub kratos: KratosConfig,
    /// Ory Keto config.
    pub keto: KetoConfig,
    /// Object storage (SeaweedFS S3) config.
    pub s3: S3Config,
    /// Stalwart CalDAV config.
    pub caldav: CalDavConfig,
    /// External AI API keys (validated but not used by server directly).
    #[serde(default)]
    pub scaleway_api_key: Option<String>,
    /// External AI API keys (validated but not used by server directly).
    #[serde(default)]
    pub mistral_api_key: Option<String>,
    /// List of configured agent workers for round-robin dispatch.
    #[serde(default)]
    pub agent_workers: Vec<AgentWorkerConfig>,
    /// Per-IP rate limiting on the public HTTP/gRPC surface.
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
}

/// Per-IP rate limiting configuration.
///
/// Applied by [`crate::middleware::rate_limit`] to the public HTTP/gRPC
/// surface. `/healthz` and `/metrics` are deliberately excluded so probes and
/// scrapers can never be throttled.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RateLimitConfig {
    /// Sustained requests per second allowed per client IP.
    #[serde(default = "default_rl_rps")]
    pub requests_per_second: u32,
    /// Burst capacity (bucket size) above the sustained rate.
    #[serde(default = "default_rl_burst")]
    pub burst: u32,
    /// Master switch; when `false` the layer is a no-op.
    #[serde(default = "default_rl_enabled")]
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_second: default_rl_rps(),
            burst: default_rl_burst(),
            enabled: default_rl_enabled(),
        }
    }
}

fn default_rl_rps() -> u32 {
    100
}

fn default_rl_burst() -> u32 {
    200
}

fn default_rl_enabled() -> bool {
    true
}

/// LiveKit server + webhook credentials.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LiveKitConfig {
    /// LiveKit server base URL, e.g. `wss://meet.sunbeam.pt`.
    pub url: String,
    /// HTTP URL for LiveKit server API.
    pub http_url: String,
    /// API key.
    pub api_key: String,
    /// API secret.
    pub api_secret: String,
}

/// Ory Kratos URLs.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KratosConfig {
    /// Public Kratos URL (session whoami).
    pub public_url: String,
    /// Admin Kratos URL (identity lookups).
    pub admin_url: String,
}

/// Ory Keto URLs.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KetoConfig {
    /// Keto read API URL.
    pub read_url: String,
    /// Keto write API URL.
    pub write_url: String,
}

/// S3 (SeaweedFS) configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct S3Config {
    /// Endpoint URL.
    pub endpoint: String,
    /// Bucket for recordings.
    pub recordings_bucket: String,
    /// Access key.
    pub access_key: String,
    /// Secret key.
    pub secret_key: String,
    /// Region (often `us-east-1` for SeaweedFS).
    #[serde(default = "default_region")]
    pub region: String,
}

/// Stalwart CalDAV config.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CalDavConfig {
    /// CalDAV base URL.
    pub url: String,
    /// Service account username.
    pub username: String,
    /// Service account password.
    pub password: String,
}

/// Remote agent worker endpoint.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentWorkerConfig {
    /// Unique name for this worker.
    pub name: String,
    /// gRPC URL including scheme (`https://...` for mTLS).
    pub url: String,
    /// Worker kind (`whisper_stt` or `mistral_summarizer`).
    pub kind: String,
    /// Path to client cert file for mTLS, if required.
    #[serde(default)]
    pub client_cert_path: Option<String>,
    /// Path to client key file for mTLS, if required.
    #[serde(default)]
    pub client_key_path: Option<String>,
    /// Path to CA cert file for verifying the worker.
    #[serde(default)]
    pub ca_cert_path: Option<String>,
}

fn default_bind() -> String {
    "0.0.0.0:3105".into()
}

fn default_metrics() -> String {
    "0.0.0.0:9090".into()
}

fn default_region() -> String {
    "us-east-1".into()
}

impl Config {
    /// Load configuration from `config.toml` (if present) plus `SUNBEAM_MEET_*`
    /// environment variables.
    pub fn load() -> anyhow::Result<Self> {
        let fig = Figment::new()
            .merge(Toml::file("config.toml"))
            .merge(Env::prefixed("SUNBEAM_MEET_").split("__"));
        let cfg: Self = fig.extract()?;
        Ok(cfg)
    }
}
