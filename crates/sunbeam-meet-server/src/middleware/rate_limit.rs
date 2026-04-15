//! Per-IP rate limiting for the public HTTP/gRPC surface.
//!
//! Built on [`tower_governor`] using a GCRA (generic cell rate algorithm)
//! bucket keyed by [`SmartIpKeyExtractor`] — which prefers
//! `x-forwarded-for` / `x-real-ip` headers so the limiter sees the real
//! client IP when we sit behind an ingress / LB, and falls back to the TCP
//! peer address otherwise.
//!
//! The layer is mounted by [`crate::lib`] on the public surface only;
//! `/healthz` and `/metrics` are mounted *outside* this layer so probes and
//! Prometheus scrapers can never be throttled.
//!
//! On rejection we do three things:
//!   1. Emit a structured JSON body `{"code":"rate_limited","message":…}`.
//!   2. Increment [`crate::metrics::Metrics::rate_limit_rejected`], labelled
//!      by the request path.
//!   3. Return HTTP 429 `Too Many Requests`.
//!
//! When [`RateLimitConfig::enabled`] is `false` the builder returns `None`
//! and the caller skips mounting any layer.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_governor::GovernorLayer;

use crate::config::RateLimitConfig;
use crate::metrics::metrics;

/// JSON body returned when a request is rate-limited.
///
/// Shape matches [`crate::error::ErrorBody`] so clients can parse
/// 429 responses with the same decoder they use for 4xx/5xx gRPC errors.
const REJECT_BODY: &str = r#"{"code":"rate_limited","message":"too many requests"}"#;

/// Build a `tower_governor` layer from the supplied configuration.
///
/// Returns `None` when [`RateLimitConfig::enabled`] is `false` so the caller
/// can omit the layer entirely (cheaper than inserting a pass-through).
#[must_use]
pub fn layer(
    cfg: &RateLimitConfig,
) -> Option<GovernorLayer<SmartIpKeyExtractor, governor::middleware::NoOpMiddleware>> {
    if !cfg.enabled {
        return None;
    }

    // tower_governor measures the sustained rate as "1 request per N
    // milliseconds"; `per_second` is the usual knob and we expose it
    // directly. A zero configuration is invalid — clamp to 1 to avoid a
    // divide-by-zero at runtime.
    let rps = cfg.requests_per_second.max(1);
    let burst = cfg.burst.max(1);

    let governor_conf = GovernorConfigBuilder::default()
        .per_second(u64::from(rps))
        .burst_size(burst)
        .key_extractor(SmartIpKeyExtractor)
        .finish()
        .expect("governor config: rps and burst were clamped ≥ 1");

    Some(GovernorLayer {
        config: Arc::new(governor_conf),
    })
}

/// Axum middleware that post-processes rate-limit rejections.
///
/// `tower_governor` returns a 429 with a plain-text body and a populated
/// `retry-after` header. We keep the header, swap the body for the
/// structured JSON shape used elsewhere in the service, and bump the
/// Prometheus counter labelled by request path.
///
/// Mount this *after* [`layer`] so it wraps the rejected response.
/// `.layer()` calls wrap outermost-last — the governor must be inner so it
/// produces the 429, and this middleware must be outer so it sees the reject
/// on the response path:
/// ```ignore
/// router
///     .layer(governor_layer)
///     .layer(axum::middleware::from_fn(rate_limit_response))
/// ```
pub async fn rate_limit_response(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_owned();
    let resp = next.run(req).await;

    if resp.status() != StatusCode::TOO_MANY_REQUESTS {
        return resp;
    }

    metrics()
        .rate_limit_rejected
        .with_label_values(&[path.as_str()])
        .inc();

    let (mut parts, _body) = resp.into_parts();
    parts.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Response::from_parts(parts, Body::from(REJECT_BODY))
}
