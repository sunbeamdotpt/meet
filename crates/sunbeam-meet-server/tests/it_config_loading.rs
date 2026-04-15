//! Integration / unit tests — figment-based Config loading.
//!
//! Does not hit any external service. Exercises the `Config::load()` logic by
//! feeding it a TOML fixture (via `Figment` directly — the same provider chain
//! production uses) and env-var overrides. Asserts on parsed struct fields.
//!
//! Runs under both `default` and `integration` nextest profiles (no `it_`
//! prefix would normally gate it to default; the `it_` prefix is kept here
//! because the fixture file path must be resolved from the workspace root,
//! which the test runner provides).

mod common;

use common::TestResult;
use figment::providers::{Env, Format, Toml};
use figment::Figment;
use sunbeam_meet_server::config::Config;

/// Path to the fixture TOML — relative to the workspace root, which is where
/// `cargo nextest` / `cargo test` sets `$CARGO_MANIFEST_DIR/../..`.
fn fixture_path() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR is the *crate* root (sunbeam-meet-server/).
    let manifest =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set by Cargo");
    std::path::PathBuf::from(manifest)
        .join("tests")
        .join("fixtures")
        .join("config_base.toml")
}

/// Load a `Config` from the base fixture. Env-var prefix must not clash with
/// real env vars already set in the runner's environment, so the fixture tests
/// use `SUNBEAM_MEET_TEST__*` (note the double underscore means nested field
/// via figment's `split("__")` convention).
fn load_from_fixture() -> anyhow::Result<Config> {
    let fig = Figment::new()
        .merge(Toml::file(fixture_path()))
        .merge(Env::prefixed("SUNBEAM_MEET_").split("__"));
    Ok(fig.extract()?)
}

// ── happy path ────────────────────────────────────────────────────────────

#[test]
fn config_loads_bind_addr_from_toml() -> TestResult {
    let cfg = load_from_fixture()?;
    assert_eq!(cfg.bind_addr, "0.0.0.0:9900", "bind_addr");
    Ok(())
}

#[test]
fn config_loads_metrics_addr_from_toml() -> TestResult {
    let cfg = load_from_fixture()?;
    assert_eq!(cfg.metrics_addr, "0.0.0.0:9901", "metrics_addr");
    Ok(())
}

#[test]
fn config_loads_livekit_block() -> TestResult {
    let cfg = load_from_fixture()?;
    assert_eq!(cfg.livekit.url, "wss://livekit.fixture.test");
    assert_eq!(cfg.livekit.http_url, "https://livekit.fixture.test");
    assert_eq!(cfg.livekit.api_key, "fixture-api-key");
    assert_eq!(
        cfg.livekit.api_secret,
        "fixture-api-secret-long-enough-xxxxx"
    );
    Ok(())
}

#[test]
fn config_loads_kratos_block() -> TestResult {
    let cfg = load_from_fixture()?;
    assert_eq!(cfg.kratos.public_url, "http://kratos.fixture.test:4433");
    assert_eq!(cfg.kratos.admin_url, "http://kratos.fixture.test:4434");
    Ok(())
}

#[test]
fn config_loads_keto_block() -> TestResult {
    let cfg = load_from_fixture()?;
    assert_eq!(cfg.keto.read_url, "http://keto.fixture.test:4466");
    assert_eq!(cfg.keto.write_url, "http://keto.fixture.test:4467");
    Ok(())
}

#[test]
fn config_loads_s3_block() -> TestResult {
    let cfg = load_from_fixture()?;
    assert_eq!(cfg.s3.endpoint, "http://seaweedfs.fixture.test:8333");
    assert_eq!(cfg.s3.recordings_bucket, "fixture-recordings");
    assert_eq!(cfg.s3.access_key, "fixture-access-key");
    assert_eq!(cfg.s3.secret_key, "fixture-secret-key");
    assert_eq!(cfg.s3.region, "eu-west-1");
    Ok(())
}

#[test]
fn config_loads_caldav_block() -> TestResult {
    let cfg = load_from_fixture()?;
    assert_eq!(
        cfg.caldav.url,
        "http://stalwart.fixture.test:8080/dav/cal/admin/default"
    );
    assert_eq!(cfg.caldav.username, "fixture-user");
    assert_eq!(cfg.caldav.password, "fixture-password");
    Ok(())
}

#[test]
fn config_agent_workers_default_is_empty() -> TestResult {
    let cfg = load_from_fixture()?;
    assert!(
        cfg.agent_workers.is_empty(),
        "agent_workers must default to []"
    );
    Ok(())
}

#[test]
fn config_optional_fields_default_to_none() -> TestResult {
    let cfg = load_from_fixture()?;
    assert!(cfg.otlp_endpoint.is_none(), "otlp_endpoint");
    assert!(cfg.scaleway_api_key.is_none(), "scaleway_api_key");
    assert!(cfg.mistral_api_key.is_none(), "mistral_api_key");
    Ok(())
}

// ── override merge semantics ───────────────────────────────────────────────
//
// Env-var override is exercised via `Serialized` rather than by mutating the
// real process env — workspace lint forbids `unsafe_code`, so `std::env::set_var`
// is off-limits. `Serialized` uses the same figment merge machinery as `Env`
// does in production, so this still exercises the override precedence.

#[test]
fn later_provider_overrides_bind_addr() -> TestResult {
    use figment::providers::Serialized;
    let cfg: Config = Figment::new()
        .merge(Toml::file(fixture_path()))
        .merge(Serialized::default("bind_addr", "127.0.0.1:7777"))
        .extract()?;
    assert_eq!(cfg.bind_addr, "127.0.0.1:7777");
    Ok(())
}

#[test]
fn later_provider_overrides_nested_livekit_api_key() -> TestResult {
    use figment::providers::Serialized;
    let cfg: Config = Figment::new()
        .merge(Toml::file(fixture_path()))
        .merge(Serialized::default("livekit.api_key", "env-api-key"))
        .extract()?;
    assert_eq!(cfg.livekit.api_key, "env-api-key");
    Ok(())
}

#[test]
fn later_provider_sets_otlp_endpoint() -> TestResult {
    use figment::providers::Serialized;
    let cfg: Config = Figment::new()
        .merge(Toml::file(fixture_path()))
        .merge(Serialized::default(
            "otlp_endpoint",
            "http://otel.test:4317",
        ))
        .extract()?;
    assert_eq!(cfg.otlp_endpoint.as_deref(), Some("http://otel.test:4317"));
    Ok(())
}

// ── default values ────────────────────────────────────────────────────────

/// When bind_addr is absent from both TOML and env, the serde default fires.
#[test]
fn default_bind_addr_when_absent_from_toml() -> TestResult {
    // Build a minimal figment with *only* the required fields, no bind_addr.
    let toml_str = r#"
database_url = "postgres://x:x@localhost/x"
valkey_url   = "redis://localhost:6379"
nats_url     = "nats://localhost:4222"

[livekit]
url        = "wss://livekit.test"
http_url   = "https://livekit.test"
api_key    = "k"
api_secret = "s"

[kratos]
public_url = "http://k:4433"
admin_url  = "http://k:4434"

[keto]
read_url  = "http://k:4466"
write_url = "http://k:4467"

[s3]
endpoint          = "http://s3.test:8333"
recordings_bucket = "bucket"
access_key        = "ak"
secret_key        = "sk"

[caldav]
url      = "http://caldav.test/dav"
username = "u"
password = "p"
"#;
    let fig = Figment::new().merge(figment::providers::Toml::string(toml_str));
    let cfg: Config = fig.extract()?;
    assert_eq!(cfg.bind_addr, "0.0.0.0:8080", "default bind_addr");
    assert_eq!(cfg.metrics_addr, "0.0.0.0:9090", "default metrics_addr");
    assert_eq!(cfg.s3.region, "us-east-1", "default s3.region");
    Ok(())
}

// ── agent_workers deserialization ─────────────────────────────────────────

#[test]
fn config_deserializes_agent_workers_list() -> TestResult {
    let toml_str = r#"
database_url = "postgres://x:x@localhost/x"
valkey_url   = "redis://localhost:6379"
nats_url     = "nats://localhost:4222"

[livekit]
url        = "wss://livekit.test"
http_url   = "https://livekit.test"
api_key    = "k"
api_secret = "s"

[kratos]
public_url = "http://k:4433"
admin_url  = "http://k:4434"

[keto]
read_url  = "http://k:4466"
write_url = "http://k:4467"

[s3]
endpoint          = "http://s3.test:8333"
recordings_bucket = "bucket"
access_key        = "ak"
secret_key        = "sk"

[caldav]
url      = "http://caldav.test/dav"
username = "u"
password = "p"

[[agent_workers]]
name = "whisper-1"
url  = "http://agent-1:50051"
kind = "whisper_stt"

[[agent_workers]]
name = "mistral-1"
url  = "https://agent-2:50051"
kind = "mistral_summarizer"
ca_cert_path = "/certs/ca.pem"
"#;
    let fig = Figment::new().merge(figment::providers::Toml::string(toml_str));
    let cfg: Config = fig.extract()?;
    assert_eq!(
        cfg.agent_workers.len(),
        2,
        "must parse two agent_workers entries"
    );

    let w0 = &cfg.agent_workers[0];
    assert_eq!(w0.name, "whisper-1");
    assert_eq!(w0.url, "http://agent-1:50051");
    assert_eq!(w0.kind, "whisper_stt");
    assert!(w0.ca_cert_path.is_none());

    let w1 = &cfg.agent_workers[1];
    assert_eq!(w1.name, "mistral-1");
    assert_eq!(w1.kind, "mistral_summarizer");
    assert_eq!(w1.ca_cert_path.as_deref(), Some("/certs/ca.pem"));
    assert!(w1.client_cert_path.is_none());
    assert!(w1.client_key_path.is_none());

    Ok(())
}
