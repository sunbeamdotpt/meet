//! Unit tests — Prometheus metrics registry.
//!
//! Mock-free. Exercises `metrics::metrics()` (the global handle returned by
//! the OnceLock initialiser), increments counters / histograms, then scrapes
//! the registry into the Prometheus text format and parses the output with
//! the `prometheus` crate's own `TextEncoder` / `gather` surface — we assert
//! on structured `MetricFamily` fields, never on the raw text.

mod common;

use common::TestResult;
use prometheus::TextEncoder;
use sunbeam_meet_server::metrics::metrics;

// ── helpers ───────────────────────────────────────────────────────────────

/// Gather all metric families from the shared registry, encode to text, and
/// return the decoded families. Callers assert on `MetricFamily` values.
fn gather_all() -> Vec<prometheus::proto::MetricFamily> {
    metrics().registry.gather()
}

/// Find a `MetricFamily` by name. Panics if not found.
fn family(name: &str) -> prometheus::proto::MetricFamily {
    gather_all()
        .into_iter()
        .find(|f| f.get_name() == name)
        .unwrap_or_else(|| panic!("metric family '{name}' not found in registry"))
}

/// Sum the `value` field over all counter metrics in a family.
fn counter_sum(fam: &prometheus::proto::MetricFamily) -> f64 {
    fam.get_metric()
        .iter()
        .map(|m| m.get_counter().get_value())
        .sum()
}

/// Sum the `sample_count` field over all histogram metrics in a family.
fn histogram_sample_count(fam: &prometheus::proto::MetricFamily) -> u64 {
    fam.get_metric()
        .iter()
        .map(|m| m.get_histogram().get_sample_count())
        .sum()
}

/// Touch every metric family so Prometheus includes them in `gather()` output.
/// Vec-families without observed children are omitted entirely.
fn prime_all_families() {
    let m = metrics();
    m.rpc_total.with_label_values(&["probe", "ok"]).inc_by(0);
    m.rpc_latency.with_label_values(&["probe"]).observe(0.0);
    m.fanout_subscribers.with_label_values(&["probe"]).set(0);
    m.webhook_total.with_label_values(&["probe"]).inc_by(0);
    m.agent_available.with_label_values(&["probe"]).set(0);
    m.egress_total.with_label_values(&["probe"]).inc_by(0);
    m.webhook_lag_seconds
        .with_label_values(&["probe"])
        .observe(0.0);
}

// ── registry sanity ───────────────────────────────────────────────────────

#[test]
fn metrics_returns_same_instance_on_every_call() {
    // OnceLock guarantees the same object — use raw pointer equality.
    let a = metrics() as *const _;
    let b = metrics() as *const _;
    assert_eq!(
        a, b,
        "metrics() must return the same OnceLock-initialised handle"
    );
}

#[test]
fn registry_contains_all_expected_families() {
    // Prometheus omits vec-families with no observed children from `gather()`,
    // so touch each expected family with a zero-value label before scraping.
    let m = metrics();
    m.rpc_total.with_label_values(&["probe", "ok"]).inc_by(0);
    m.rpc_latency.with_label_values(&["probe"]).observe(0.0);
    m.fanout_subscribers.with_label_values(&["probe"]).set(0);
    m.webhook_total.with_label_values(&["probe"]).inc_by(0);
    m.agent_available.with_label_values(&["probe"]).set(0);
    m.egress_total.with_label_values(&["probe"]).inc_by(0);
    m.webhook_lag_seconds
        .with_label_values(&["probe"])
        .observe(0.0);

    let names: Vec<String> = gather_all()
        .iter()
        .map(|f| f.get_name().to_owned())
        .collect();

    for expected in &[
        "meet_rpc_total",
        "meet_rpc_latency_seconds",
        "meet_fanout_subscribers",
        "meet_webhook_total",
        "meet_agent_available",
        "meet_egress_total",
        "meet_webhook_lag_seconds",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "registry must contain family '{expected}', got: {names:?}",
        );
    }
}

// ── webhook_total counter ─────────────────────────────────────────────────

#[test]
fn webhook_total_increments_correctly() {
    let m = metrics();

    // Prime the vec so the family appears in `gather()` before we snapshot.
    m.webhook_total.with_label_values(&["probe"]).inc_by(0);
    let before: f64 = counter_sum(&family("meet_webhook_total"));
    m.webhook_total
        .with_label_values(&["participant_joined"])
        .inc();
    m.webhook_total
        .with_label_values(&["participant_joined"])
        .inc();
    m.webhook_total.with_label_values(&["room_finished"]).inc();
    let after: f64 = counter_sum(&family("meet_webhook_total"));

    // We added 3 across the whole family; at least 3 more must be present.
    assert!(
        after >= before + 3.0,
        "webhook_total must have increased by at least 3, delta = {}",
        after - before,
    );
}

#[test]
fn webhook_total_carries_event_label() {
    let m = metrics();
    m.webhook_total
        .with_label_values(&["room_started"])
        .inc_by(5);

    let fam = family("meet_webhook_total");
    // Find the metric where the "event" label value equals "room_started".
    let metric = fam
        .get_metric()
        .iter()
        .find(|met| {
            met.get_label()
                .iter()
                .any(|lp| lp.get_name() == "event" && lp.get_value() == "room_started")
        })
        .expect("must have a 'room_started' label set");
    let value = metric.get_counter().get_value();
    assert!(
        value >= 5.0,
        "room_started counter must be at least 5.0, got {value}",
    );
}

// ── rpc_total counter ─────────────────────────────────────────────────────

#[test]
fn rpc_total_records_method_and_status_labels() {
    let m = metrics();
    m.rpc_total.with_label_values(&["JoinRoom", "ok"]).inc_by(3);
    m.rpc_total
        .with_label_values(&["JoinRoom", "unauthenticated"])
        .inc();

    let fam = family("meet_rpc_total");
    let ok_metric = fam
        .get_metric()
        .iter()
        .find(|met| {
            let labels: std::collections::HashMap<_, _> = met
                .get_label()
                .iter()
                .map(|l| (l.get_name(), l.get_value()))
                .collect();
            labels.get("method") == Some(&"JoinRoom") && labels.get("status") == Some(&"ok")
        })
        .expect("must have JoinRoom/ok label pair");
    assert!(
        ok_metric.get_counter().get_value() >= 3.0,
        "JoinRoom/ok must be >= 3",
    );
}

// ── rpc_latency histogram ─────────────────────────────────────────────────

#[test]
fn rpc_latency_histogram_records_observations() {
    let m = metrics();
    m.rpc_latency
        .with_label_values(&["CreateRoom"])
        .observe(0.042);
    m.rpc_latency
        .with_label_values(&["CreateRoom"])
        .observe(0.100);

    let fam = family("meet_rpc_latency_seconds");
    let count = histogram_sample_count(&fam);
    assert!(
        count >= 2,
        "histogram must record at least 2 observations, got {count}"
    );
}

// ── webhook_lag_seconds histogram ─────────────────────────────────────────

#[test]
fn webhook_lag_histogram_records_observations() {
    let m = metrics();
    m.webhook_lag_seconds
        .with_label_values(&["participant_joined"])
        .observe(1.5);
    m.webhook_lag_seconds
        .with_label_values(&["participant_joined"])
        .observe(0.3);

    let fam = family("meet_webhook_lag_seconds");
    let count = histogram_sample_count(&fam);
    assert!(
        count >= 2,
        "webhook_lag histogram must record >= 2 samples, got {count}"
    );
}

// ── fanout_subscribers gauge ─────────────────────────────────────────────

#[test]
fn fanout_subscribers_gauge_set_and_read() {
    let m = metrics();
    m.fanout_subscribers
        .with_label_values(&["room-test"])
        .set(7);

    let fam = family("meet_fanout_subscribers");
    let gauge_metric = fam
        .get_metric()
        .iter()
        .find(|met| {
            met.get_label()
                .iter()
                .any(|l| l.get_name() == "room_id" && l.get_value() == "room-test")
        })
        .expect("must have room_id=room-test gauge");
    assert_eq!(
        gauge_metric.get_gauge().get_value() as i64,
        7,
        "gauge must read back 7",
    );
}

// ── agent_available gauge ─────────────────────────────────────────────────

#[test]
fn agent_available_gauge_set_and_read() {
    let m = metrics();
    m.agent_available
        .with_label_values(&["worker-probe"])
        .set(1);

    let fam = family("meet_agent_available");
    let gauge = fam
        .get_metric()
        .iter()
        .find(|met| {
            met.get_label()
                .iter()
                .any(|l| l.get_name() == "worker" && l.get_value() == "worker-probe")
        })
        .expect("must have worker=worker-probe gauge");
    assert_eq!(
        gauge.get_gauge().get_value() as i64,
        1,
        "agent_available gauge must be 1",
    );
}

// ── egress_total counter ──────────────────────────────────────────────────

#[test]
fn egress_total_increments_by_outcome_label() {
    let m = metrics();
    m.egress_total.with_label_values(&["started"]).inc_by(2);
    m.egress_total.with_label_values(&["failed"]).inc();

    let fam = family("meet_egress_total");
    let started = fam
        .get_metric()
        .iter()
        .find(|met| {
            met.get_label()
                .iter()
                .any(|l| l.get_name() == "outcome" && l.get_value() == "started")
        })
        .map_or(0.0, |m| m.get_counter().get_value());
    assert!(
        started >= 2.0,
        "egress 'started' counter must be >= 2, got {started}"
    );
}

// ── text-format scrape ─────────────────────────────────────────────────────

/// Encode the registry to the Prometheus text format. Must not error and
/// must produce output that begins with a `# HELP` or `# TYPE` line.
#[test]
fn text_format_scrape_produces_well_formed_output() -> TestResult {
    prime_all_families();
    let encoder = TextEncoder::new();
    let mf = metrics().registry.gather();
    let text = encoder.encode_to_string(&mf)?;

    assert!(!text.is_empty(), "encoded metrics must not be empty");
    assert!(
        text.lines()
            .any(|l| l.starts_with("# HELP") || l.starts_with("# TYPE")),
        "Prometheus text output must contain at least one # HELP or # TYPE comment",
    );
    // Every line must be valid UTF-8 (already guaranteed by String) and must
    // not be a raw error message.
    for line in text.lines() {
        assert!(
            !line.starts_with("encode:"),
            "metrics endpoint must not contain error lines, got: {line}",
        );
    }
    Ok(())
}

/// The encoded output must contain all metric family names.
#[test]
fn text_format_contains_all_metric_names() -> TestResult {
    prime_all_families();
    let encoder = TextEncoder::new();
    let mf = metrics().registry.gather();
    let text = encoder.encode_to_string(&mf)?;

    for name in &[
        "meet_rpc_total",
        "meet_rpc_latency_seconds",
        "meet_fanout_subscribers",
        "meet_webhook_total",
        "meet_agent_available",
        "meet_egress_total",
        "meet_webhook_lag_seconds",
    ] {
        assert!(text.contains(name), "encoded text must contain '{name}'",);
    }
    Ok(())
}
