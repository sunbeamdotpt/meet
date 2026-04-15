//! Prometheus metrics registry and HTTP endpoint.

use std::sync::OnceLock;

use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use prometheus::{
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry, HistogramVec, IntCounterVec, IntGaugeVec, Registry,
    TextEncoder,
};

/// Container for named metrics used throughout the service.
pub struct Metrics {
    /// Shared registry.
    pub registry: Registry,
    /// Counter of RPCs served, labelled by method + status.
    pub rpc_total: IntCounterVec,
    /// Histogram of RPC latency, labelled by method.
    pub rpc_latency: HistogramVec,
    /// Gauge of active fan-out subscribers per room.
    pub fanout_subscribers: IntGaugeVec,
    /// Counter of webhook events received, labelled by type.
    pub webhook_total: IntCounterVec,
    /// Gauge of agent worker availability, labelled by worker name.
    pub agent_available: IntGaugeVec,
    /// Counter of Egress start/fail, labelled by outcome.
    pub egress_total: IntCounterVec,
    /// Webhook delivery lag (seconds), labelled by event type.
    pub webhook_lag_seconds: HistogramVec,
    /// Counter of requests rejected by the per-IP rate limiter,
    /// labelled by `route` (matched request URI path).
    pub rate_limit_rejected: IntCounterVec,
}

static METRICS: OnceLock<Metrics> = OnceLock::new();

/// Get (lazily initialize) the global metrics handle.
pub fn metrics() -> &'static Metrics {
    METRICS.get_or_init(|| {
        let registry = Registry::new();
        let rpc_total = register_int_counter_vec_with_registry!(
            "meet_rpc_total",
            "RPC calls",
            &["method", "status"],
            registry
        )
        .expect("register rpc_total");
        let rpc_latency = register_histogram_vec_with_registry!(
            "meet_rpc_latency_seconds",
            "RPC latency",
            &["method"],
            registry
        )
        .expect("register rpc_latency");
        let fanout_subscribers = register_int_gauge_vec_with_registry!(
            "meet_fanout_subscribers",
            "Fan-out subscribers per room",
            &["room_id"],
            registry
        )
        .expect("register fanout_subscribers");
        let webhook_total = register_int_counter_vec_with_registry!(
            "meet_webhook_total",
            "LiveKit webhook events",
            &["event"],
            registry
        )
        .expect("register webhook_total");
        let agent_available = register_int_gauge_vec_with_registry!(
            "meet_agent_available",
            "Agent worker availability",
            &["worker"],
            registry
        )
        .expect("register agent_available");
        let egress_total = register_int_counter_vec_with_registry!(
            "meet_egress_total",
            "Egress lifecycle",
            &["outcome"],
            registry
        )
        .expect("register egress_total");
        let webhook_lag_seconds = register_histogram_vec_with_registry!(
            "meet_webhook_lag_seconds",
            "Webhook delivery lag",
            &["event"],
            registry
        )
        .expect("register webhook_lag_seconds");
        let rate_limit_rejected = register_int_counter_vec_with_registry!(
            "rate_limit_rejected_total",
            "Requests rejected by the per-IP rate limiter",
            &["route"],
            registry
        )
        .expect("register rate_limit_rejected_total");
        Metrics {
            registry,
            rpc_total,
            rpc_latency,
            fanout_subscribers,
            webhook_total,
            agent_available,
            egress_total,
            webhook_lag_seconds,
            rate_limit_rejected,
        }
    })
}

/// Axum router mounting `/metrics`.
pub fn router() -> Router {
    Router::new().route("/metrics", get(metrics_handler))
}

async fn metrics_handler() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let mf = metrics().registry.gather();
    match encoder.encode_to_string(&mf) {
        Ok(s) => (axum::http::StatusCode::OK, s),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("encode: {e}"),
        ),
    }
}
