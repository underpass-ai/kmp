mod buffered_quality_metrics_observer;
mod embedded_telemetry_guard;
pub mod metrics;
pub mod quality_observers;
mod quality_telemetry_observation;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_otlp::WithTonicConfig;
use opentelemetry_otlp::tonic_types::transport::{Certificate, ClientTlsConfig, Identity};
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

pub use buffered_quality_metrics_observer::BufferedQualityMetricsObserver;
pub use embedded_telemetry_guard::EmbeddedTelemetryGuard;
pub use metrics::KernelMetrics;
pub use quality_telemetry_observation::QualityTelemetryObservation;

/// Resources returned by `init_observability` for lifecycle management.
pub struct ObservabilityGuard {
    trace_provider: Option<SdkTracerProvider>,
    meter_provider: Option<SdkMeterProvider>,
    pub metrics: KernelMetrics,
}

/// Initializes structured logging with optional OpenTelemetry trace and metric export.
///
/// ## Environment variables
///
/// - `RUST_LOG`: log level filter (default: `info`)
/// - `REHYDRATION_LOG_FORMAT`: `json` | `pretty` | compact (default)
/// - `OTEL_EXPORTER_OTLP_ENDPOINT`: OTLP endpoint for metric export and, when
///   enabled, trace export (e.g. `http://localhost:4317`)
/// - `OTEL_TRACES_EXPORTER`: standard OTel traces exporter selector. When set
///   to `none`, trace export is disabled even if OTLP metrics remain enabled.
///   When unset, traces follow the OTLP endpoint configuration.
pub fn init_observability(service_name: &str) -> ObservabilityGuard {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let log_format = std::env::var("REHYDRATION_LOG_FORMAT").unwrap_or_default();

    let trace_provider = init_otel_tracer(service_name);
    let otel_layer = trace_provider.as_ref().map(|provider| {
        let tracer = provider.tracer(service_name.to_string());
        tracing_opentelemetry::layer().with_tracer(tracer)
    });

    let meter_provider = metrics::init_otel_metrics(service_name);
    if let Some(ref provider) = meter_provider {
        opentelemetry::global::set_meter_provider(provider.clone());
    }
    let meter = opentelemetry::global::meter("rehydration-kernel");
    let kernel_metrics = KernelMetrics::new(&meter);

    match log_format.as_str() {
        "json" => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(otel_layer)
                .with(
                    fmt::layer()
                        .json()
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_file(false)
                        .with_line_number(false),
                )
                .init();
        }
        "pretty" => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(otel_layer)
                .with(fmt::layer().pretty())
                .init();
        }
        _ => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(otel_layer)
                .with(fmt::layer().compact())
                .init();
        }
    }

    tracing::info!(service = service_name, "observability initialized");

    ObservabilityGuard {
        trace_provider,
        meter_provider,
        metrics: kernel_metrics,
    }
}

fn init_otel_tracer(service_name: &str) -> Option<SdkTracerProvider> {
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok()?;
    if !traces_export_enabled(
        std::env::var("OTEL_TRACES_EXPORTER").ok().as_deref(),
        Some(endpoint.as_str()),
    ) {
        return None;
    }
    let endpoint = endpoint.trim().to_string();

    let mut builder = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint);
    if let Some(tls_config) = build_otlp_tls_config() {
        builder = builder.with_tls_config(tls_config);
    }
    let exporter = builder.build().ok()?;

    let provider = SdkTracerProvider::builder()
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name(service_name.to_string())
                .build(),
        )
        .with_batch_exporter(exporter)
        .build();

    Some(provider)
}

fn traces_export_enabled(traces_exporter: Option<&str>, endpoint: Option<&str>) -> bool {
    let Some(endpoint) = endpoint.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    let _ = endpoint;

    !matches!(
        traces_exporter
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase()),
        Some(value) if value == "none"
    )
}

/// Build a TLS config for OTLP exporters from environment variables.
///
/// - `OTEL_EXPORTER_OTLP_CA_PATH` — CA certificate for server verification
/// - `OTEL_EXPORTER_OTLP_CERT_PATH` — client certificate for mTLS
/// - `OTEL_EXPORTER_OTLP_KEY_PATH` — client key for mTLS
///
/// Returns `None` if no TLS variables are set (plaintext mode).
pub(crate) fn build_otlp_tls_config() -> Option<ClientTlsConfig> {
    let ca_path = std::env::var("OTEL_EXPORTER_OTLP_CA_PATH").ok();
    let cert_path = std::env::var("OTEL_EXPORTER_OTLP_CERT_PATH").ok();
    let key_path = std::env::var("OTEL_EXPORTER_OTLP_KEY_PATH").ok();

    if ca_path.is_none() && cert_path.is_none() && key_path.is_none() {
        return None;
    }

    let mut tls_config = ClientTlsConfig::new();

    if let Some(ca_path) = ca_path {
        let ca_pem = std::fs::read(ca_path.trim()).ok()?;
        tls_config = tls_config.ca_certificate(Certificate::from_pem(ca_pem));
    }

    if let (Some(cert_path), Some(key_path)) = (cert_path, key_path) {
        let cert_pem = std::fs::read(cert_path.trim()).ok()?;
        let key_pem = std::fs::read(key_path.trim()).ok()?;
        tls_config = tls_config.identity(Identity::from_pem(cert_pem, key_pem));
    }

    Some(tls_config)
}

/// Shuts down the OpenTelemetry providers, flushing pending data.
pub fn shutdown_observability(guard: ObservabilityGuard) {
    if let Some(provider) = guard.trace_provider
        && let Err(error) = provider.shutdown()
    {
        tracing::warn!(%error, "opentelemetry trace shutdown failed");
    }
    if let Some(provider) = guard.meter_provider
        && let Err(error) = provider.shutdown()
    {
        tracing::warn!(%error, "opentelemetry metrics shutdown failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_metrics_instruments_are_constructible() {
        let meter = opentelemetry::global::meter("test");
        let metrics = KernelMetrics::new(&meter);
        // Verify instruments exist and can record without panic
        metrics.rpc_duration.record(0.1, &[]);
        metrics.bundle_nodes.record(5, &[]);
        metrics.bundle_relationships.record(3, &[]);
        metrics.bundle_details.record(2, &[]);
        metrics.rendered_tokens.record(100, &[]);
        metrics.truncation_total.add(1, &[]);
        metrics.projection_lag.record(0.05, &[]);
    }

    #[test]
    fn traces_export_is_disabled_when_exporter_is_none() {
        assert!(!traces_export_enabled(
            Some("none"),
            Some("https://collector:4317")
        ));
    }

    #[test]
    fn traces_export_is_disabled_without_endpoint() {
        assert!(!traces_export_enabled(Some("otlp"), None));
        assert!(!traces_export_enabled(None, Some("   ")));
    }

    #[test]
    fn traces_export_defaults_to_enabled_when_endpoint_exists() {
        assert!(traces_export_enabled(None, Some("https://collector:4317")));
        assert!(traces_export_enabled(
            Some("otlp"),
            Some("https://collector:4317")
        ));
    }
}
