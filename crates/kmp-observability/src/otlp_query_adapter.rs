use std::future::Future;
use std::pin::Pin;

use kmp_application::{
    ObservabilityExemplar, ObservabilityProjection, ObservabilityQuery, ObservabilityQueryPort,
    ObservabilitySeries,
};
use kmp_domain::PortError;

/// Query response produced by a concrete telemetry backend (Prometheus,
/// Tempo, vendor API, etc.) over data originally exported with OTLP.
///
/// OTLP itself is an ingest protocol and deliberately has no query RPC. This
/// seam keeps that backend-specific API client outside KMP while the adapter
/// gives every renderer one stable application contract.
#[derive(Debug, Clone, PartialEq)]
pub struct OtlpQueryResponse {
    pub series: Vec<ObservabilitySeries>,
    pub exemplars: Vec<ObservabilityExemplar>,
    pub missing: Vec<String>,
    pub truncated: bool,
}

pub trait OtlpMetricsQueryClient: Send + Sync {
    fn query_range<'a>(
        &'a self,
        query: &'a ObservabilityQuery,
    ) -> Pin<Box<dyn Future<Output = Result<OtlpQueryResponse, PortError>> + Send + 'a>>;
}

/// Maps an OTLP-compatible backend's query API onto the application port.
/// Values retain backend-provided unit, scope and exemplars; this adapter
/// derives neither health scores nor causal links.
#[derive(Debug, Clone)]
pub struct OtlpObservabilityQueryAdapter<C> {
    client: C,
}

impl<C> OtlpObservabilityQueryAdapter<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }
}

impl<C> ObservabilityQueryPort for OtlpObservabilityQueryAdapter<C>
where
    C: OtlpMetricsQueryClient,
{
    fn query<'a>(
        &'a self,
        query: ObservabilityQuery,
    ) -> Pin<Box<dyn Future<Output = Result<ObservabilityProjection, PortError>> + Send + 'a>> {
        Box::pin(async move {
            let response = self.client.query_range(&query).await?;
            Ok(ObservabilityProjection {
                contract: "kmp.observability.projection.v1".to_string(),
                from_millis: query.from_millis,
                to_millis: query.to_millis,
                series: response.series,
                exemplars: response.exemplars,
                missing: response.missing,
                truncated: response.truncated,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeClient;

    impl OtlpMetricsQueryClient for FakeClient {
        fn query_range<'a>(
            &'a self,
            _query: &'a ObservabilityQuery,
        ) -> Pin<Box<dyn Future<Output = Result<OtlpQueryResponse, PortError>> + Send + 'a>>
        {
            Box::pin(async {
                Ok(OtlpQueryResponse {
                    series: vec![ObservabilitySeries {
                        name: "rpc_duration".to_string(),
                        unit: "seconds".to_string(),
                        scope: "rpc".to_string(),
                        points: Vec::new(),
                    }],
                    exemplars: Vec::new(),
                    missing: Vec::new(),
                    truncated: false,
                })
            })
        }
    }

    #[tokio::test]
    async fn adapter_preserves_exact_metric_semantics() {
        let adapter = OtlpObservabilityQueryAdapter::new(FakeClient);
        let result = adapter
            .query(ObservabilityQuery {
                about: Some("project:kmp".to_string()),
                from_millis: 10,
                to_millis: 20,
                series: vec!["rpc_duration".to_string()],
                max_points: 100,
            })
            .await
            .expect("backend query");
        assert_eq!(result.series[0].unit, "seconds");
        assert_eq!(result.series[0].scope, "rpc");
    }
}
