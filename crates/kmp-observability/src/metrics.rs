use opentelemetry::metrics::{Counter, Histogram, Meter};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_otlp::WithTonicConfig;
use opentelemetry_sdk::metrics::SdkMeterProvider;

/// Kernel-wide metric instruments.
///
/// When `OTEL_EXPORTER_OTLP_ENDPOINT` is set, these export via OTLP.
/// When not set, the instruments are still valid but discard data (noop meter).
pub struct KernelMetrics {
    pub rpc_duration: Histogram<f64>,
    pub bundle_nodes: Histogram<u64>,
    pub bundle_relationships: Histogram<u64>,
    pub bundle_details: Histogram<u64>,
    pub rendered_tokens: Histogram<u64>,
    pub truncation_total: Counter<u64>,
    pub projection_lag: Histogram<f64>,
}

impl KernelMetrics {
    pub fn new(meter: &Meter) -> Self {
        Self {
            rpc_duration: meter
                .f64_histogram("rehydration.rpc.duration")
                .with_description("RPC latency in seconds")
                .with_unit("s")
                .build(),
            bundle_nodes: meter
                .u64_histogram("rehydration.bundle.nodes")
                .with_description("Number of nodes in rehydrated bundle")
                .build(),
            bundle_relationships: meter
                .u64_histogram("rehydration.bundle.relationships")
                .with_description("Number of relationships in rehydrated bundle")
                .build(),
            bundle_details: meter
                .u64_histogram("rehydration.bundle.details")
                .with_description("Number of node details in rehydrated bundle")
                .build(),
            rendered_tokens: meter
                .u64_histogram("rehydration.rendered.tokens")
                .with_description("Rendered token count after budget enforcement")
                .build(),
            truncation_total: meter
                .u64_counter("rehydration.truncation.total")
                .with_description("Number of renders that required truncation")
                .build(),
            projection_lag: meter
                .f64_histogram("rehydration.projection.lag")
                .with_description("Projection processing lag in seconds")
                .with_unit("s")
                .build(),
        }
    }
}

pub(crate) fn init_otel_metrics(service_name: &str) -> Option<SdkMeterProvider> {
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok()?;
    if endpoint.trim().is_empty() {
        return None;
    }

    let mut builder = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint);
    if let Some(tls_config) = super::build_otlp_tls_config() {
        builder = builder.with_tls_config(tls_config);
    }
    let exporter = builder.build().ok()?;

    let reader = opentelemetry_sdk::metrics::PeriodicReader::builder(exporter).build();

    let provider = SdkMeterProvider::builder()
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name(service_name.to_string())
                .build(),
        )
        .with_reader(reader)
        .build();

    Some(provider)
}
