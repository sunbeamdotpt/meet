//! Shared helpers for sunbeam-meet test suites.
//!
//! This module is included from both `unit_*.rs` and `it_*.rs` test binaries
//! via `mod common;`. Because Cargo compiles each `tests/*.rs` file as its
//! own binary, some helpers may appear unused in a given binary — that's
//! fine, `#[allow(dead_code)]` on the module suppresses the warning.
#![allow(dead_code)]

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
