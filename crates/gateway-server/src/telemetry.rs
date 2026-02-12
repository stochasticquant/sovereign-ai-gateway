//! Telemetry initialization for logging and distributed tracing.
//!
//! Sets up a layered tracing subscriber:
//! - Always: fmt layer for stdout logging (JSON or pretty)
//! - Optional: OpenTelemetry layer for distributed tracing via OTLP to Jaeger

use gateway_core::config::TelemetryConfig;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize the tracing/telemetry pipeline.
///
/// When `otlp_endpoint` is configured, sets up a dual-layer subscriber:
/// 1. fmt layer (stdout logging in JSON or pretty format)
/// 2. OpenTelemetry layer (sends spans to Jaeger/collector via OTLP gRPC)
///
/// When `otlp_endpoint` is `None`, only the fmt layer is active.
///
/// Returns an optional `TracerProvider` that must be shut down on exit.
pub fn init_telemetry(
    config: &TelemetryConfig,
) -> Option<opentelemetry_sdk::trace::TracerProvider> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    // Build the fmt layer based on log_format config
    let fmt_layer = tracing_subscriber::fmt::layer();

    if config.log_format == "json" {
        if let Some(ref endpoint) = config.otlp_endpoint {
            let provider = init_otel_provider(endpoint);
            let tracer =
                opentelemetry::trace::TracerProvider::tracer(&provider, "sovereign-gateway");
            let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer.json().flatten_event(true))
                .with(otel_layer)
                .init();

            Some(provider)
        } else {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer.json().flatten_event(true))
                .init();

            None
        }
    } else if let Some(ref endpoint) = config.otlp_endpoint {
        let provider = init_otel_provider(endpoint);
        let tracer = opentelemetry::trace::TracerProvider::tracer(&provider, "sovereign-gateway");
        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer.pretty())
            .with(otel_layer)
            .init();

        Some(provider)
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer.pretty())
            .init();

        None
    }
}

/// Create an OpenTelemetry TracerProvider with OTLP gRPC exporter.
fn init_otel_provider(endpoint: &str) -> opentelemetry_sdk::trace::TracerProvider {
    use opentelemetry::KeyValue;
    use opentelemetry_otlp::{SpanExporter, WithExportConfig};
    use opentelemetry_sdk::{Resource, runtime, trace::TracerProvider};

    let exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .expect("Failed to create OTLP span exporter");

    TracerProvider::builder()
        .with_batch_exporter(exporter, runtime::Tokio)
        .with_resource(Resource::new([
            KeyValue::new("service.name", "sovereign-ai-gateway"),
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
        ]))
        .build()
}

/// Gracefully shut down the OpenTelemetry pipeline, flushing pending spans.
pub fn shutdown_telemetry(provider: Option<opentelemetry_sdk::trace::TracerProvider>) {
    if let Some(provider) = provider {
        if let Err(e) = provider.shutdown() {
            eprintln!("Failed to shut down OTel tracer provider: {:?}", e);
        }
    }
}
