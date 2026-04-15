//! Integration test — per-IP rate limiting middleware
//! (`src/middleware/rate_limit.rs`).
//!
//! Runs under the default `cargo test` profile because the middleware has
//! no infra dependencies — the governor is an in-process GCRA bucket. We
//! still follow the integration-test conventions in this crate: no mocks,
//! no string-matching on state, assertions against structured fields.
//!
//! Strategy:
//!   - Build a minimal axum `Router` that mounts the real layer plus the
//!     `rate_limit_response` post-processor.
//!   - Fire N = burst + 1 requests through the router via
//!     `tower::ServiceExt::oneshot` — all from the same synthetic peer IP
//!     (set via `x-forwarded-for` so `SmartIpKeyExtractor` picks it up).
//!   - Assert the last response is 429, JSON content type, body is the
//!     structured `{"code":"rate_limited", …}` shape, and the Prometheus
//!     counter `rate_limit_rejected_total{route="/probe"}` incremented.

mod common;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, StatusCode};
use axum::routing::get;
use axum::Router;
use common::TestResult;
use http_body_util::BodyExt;
use serde_json::Value;
use sunbeam_meet_server::config::RateLimitConfig;
use sunbeam_meet_server::metrics::metrics;
use sunbeam_meet_server::middleware::rate_limit::{layer, rate_limit_response};
use tower::ServiceExt;

/// Build the probe router used by every test in this file. Mounts a single
/// `GET /probe` route guarded by the production rate-limit layer configured
/// with a tight budget (2 rps / burst 2) so we can hit the reject path with
/// a handful of requests.
fn probe_router(cfg: &RateLimitConfig) -> Router {
    let governor = layer(cfg).expect("layer must be Some when enabled");
    Router::new()
        .route("/probe", get(|| async { "ok" }))
        .layer(governor)
        .layer(axum::middleware::from_fn(rate_limit_response))
}

/// Fire a `GET /probe` carrying a synthetic client IP via
/// `x-forwarded-for`, which `SmartIpKeyExtractor` prefers over the TCP
/// peer address.
async fn send(router: Router, ip: &str) -> axum::response::Response {
    let req = Request::builder()
        .method("GET")
        .uri("/probe")
        .header("x-forwarded-for", ip)
        .body(Body::empty())
        .expect("valid request");
    router.oneshot(req).await.expect("oneshot")
}

/// After exhausting the burst the next request is rejected with a
/// structured JSON 429 and the Prometheus counter increments by exactly the
/// number of rejected requests.
#[tokio::test]
async fn it_rate_limit_rejects_after_burst_with_structured_body() -> TestResult {
    let cfg = RateLimitConfig {
        requests_per_second: 1,
        burst: 3,
        enabled: true,
    };
    let router = probe_router(&cfg);

    // Unique IP per test so we don't share a bucket with a sibling test
    // running in the same process.
    let ip = "198.51.100.10";

    // Snapshot the counter *before* any rejects.
    let before = metrics()
        .rate_limit_rejected
        .with_label_values(&["/probe"])
        .get();

    // Fire burst requests — all should pass.
    for i in 0..cfg.burst {
        let resp = send(router.clone(), ip).await;
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "request {i} within burst must pass"
        );
    }

    // The next request exceeds the burst and must be rejected.
    let rejected = send(router.clone(), ip).await;
    assert_eq!(
        rejected.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "post-burst request must be 429"
    );
    assert_eq!(
        rejected
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/json"),
        "429 body must be JSON"
    );

    // Parse the JSON body — structural assertion, no substring matching.
    let body_bytes = rejected
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).expect("valid JSON");
    assert_eq!(
        body.get("code").and_then(Value::as_str),
        Some("rate_limited"),
        "body.code"
    );
    assert!(
        body.get("message").and_then(Value::as_str).is_some(),
        "body.message must be present and a string"
    );

    // Counter must have incremented by exactly one on this route.
    let after = metrics()
        .rate_limit_rejected
        .with_label_values(&["/probe"])
        .get();
    assert_eq!(
        after - before,
        1,
        "rate_limit_rejected_total must increment by 1 per rejected request \
         (before={before}, after={after})"
    );

    Ok(())
}

/// When `enabled = false` the builder returns `None` and no layer is
/// mounted, so every request passes regardless of volume.
#[tokio::test]
async fn it_rate_limit_disabled_allows_all() -> TestResult {
    let cfg = RateLimitConfig {
        requests_per_second: 1,
        burst: 1,
        enabled: false,
    };
    assert!(
        layer(&cfg).is_none(),
        "layer must be None when enabled=false"
    );

    // Build a bare router without the layer to confirm the caller-side
    // pattern works: no governor, no post-processor.
    let router: Router = Router::new().route("/probe", get(|| async { "ok" }));

    for _ in 0..10_u32 {
        let req = Request::builder()
            .uri("/probe")
            .body(Body::empty())
            .expect("req");
        let resp = router.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
    }
    Ok(())
}
