//! Tracing + OpenTelemetry OTLP bootstrap.

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use crate::config::Config;

/// Initialize `tracing_subscriber` with an optional OTLP layer.
pub fn init(config: &Config) -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sunbeam_meet_server=debug"));

    let fmt = tracing_subscriber::fmt::layer().json();

    let registry = tracing_subscriber::registry().with(filter).with(fmt);

    if let Some(endpoint) = &config.otlp_endpoint {
        use opentelemetry::trace::TracerProvider as _;
        use opentelemetry_otlp::WithExportConfig;
        use opentelemetry_sdk::Resource;

        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint.clone())
            .build()?;
        let provider = opentelemetry_sdk::trace::TracerProvider::builder()
            .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
            .with_resource(Resource::new(vec![opentelemetry::KeyValue::new(
                "service.name",
                "sunbeam-meet",
            )]))
            .build();
        let tracer = provider.tracer("sunbeam-meet");
        opentelemetry::global::set_tracer_provider(provider);

        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
        registry.with(otel_layer).try_init()?;
    } else {
        registry.try_init()?;
    }

    Ok(())
}
