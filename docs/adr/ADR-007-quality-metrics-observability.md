# ADR-007: BundleQualityMetrics — DDD value object + hexagonal observer port

**Status:** Accepted
**Date:** 2026-03-27
**Context:** P0 BundleQualityMetrics bugs from audit + need for multi-backend observability

## Decision

Model quality metrics as a **domain value object** with a **hexagonal observer port** and multiple adapter implementations.

## Architecture

```
Domain Layer
├── BundleQualityMetrics       (value object, invariant-validated)
│   ├── new() → Result         (constructor with [0,1] range checks)
│   └── compute()              (factory: bundle + tokens + estimator → metrics)
└── QualityMetricsObserver     (port trait)
    └── observe(metrics, ctx)

Observability Adapters (kmp-observability)
├── OTelQualityObserver        (OTLP histograms → Prometheus/Grafana)
├── TracingQualityObserver     (structured JSON logs → Loki/Grafana)
├── CompositeQualityObserver   (fan-out to N backends)
└── NoopQualityObserver        (tests / disabled)

Transport Layer
└── QueryGrpcServiceV1Beta1    (holds Arc<dyn QualityMetricsObserver>)
    ├── get_context()          → observer.observe()
    ├── get_context_path()     → observer.observe()
    └── rehydrate_session()    → observer.observe()   (per-role bundles)

Composition Root (main.rs)
└── CompositeQualityObserver(OTel + Tracing)
```

## BundleQualityMetrics Value Object

Five metrics computed per render:

| Metric | Type | Invariant | Semantics |
|--------|------|-----------|-----------|
| `raw_equivalent_tokens` | `u32` | ≥ 0 | Flat text dump token count (baseline) |
| `compression_ratio` | `f64` | ≥ 0.0 | raw / rendered. >1.0 = compression |
| `causal_density` | `f64` | [0.0, 1.0] | Causal+Motivational+Evidential / total rels |
| `noise_ratio` | `f64` | [0.0, 1.0] | Noise/distractor nodes / total nodes |
| `detail_coverage` | `f64` | [0.0, 1.0] | Nodes with detail / total nodes |

The `compute()` factory owns the **canonical raw dump text format** — identical
output to testkit's `raw_dump.rs` for the same data. This ensures compression_ratio
is consistent between kernel and benchmarks.

## QualityObservationContext

```rust
pub struct QualityObservationContext {
    pub rpc: String,          // "GetContext", "GetContextPath", "RehydrateSession"
    pub root_node_id: String, // Graph root being queried
    pub role: String,         // Role for which the bundle was rendered
}
```

## Helm Chart Integration

Grafana, Loki, and OTel Collector are optional subcharts:

```yaml
# values.yaml
loki:
  enabled: false
grafana:
  enabled: false
otelCollector:
  enabled: false
```

When `otelCollector.enabled=true`, the deployment auto-wires
`OTEL_EXPORTER_OTLP_ENDPOINT` to the in-chart collector service alias
`<release>-otel`.

Grafana auto-provisions Loki + Prometheus datasources.

## Observability Paths

### Loki (structured logs)

The `TracingQualityObserver` emits structured JSON via `tracing::info!`:

```json
{
  "timestamp": "2026-03-27T16:00:00Z",
  "level": "INFO",
  "target": "rehydration.quality",
  "fields": {
    "rpc": "GetContext",
    "root_node_id": "node:case:123",
    "role": "developer",
    "quality_raw_equivalent_tokens": 342,
    "quality_compression_ratio": 1.87,
    "quality_causal_density": 0.67,
    "quality_noise_ratio": 0.0,
    "quality_detail_coverage": 0.75
  },
  "message": "bundle quality metrics"
}
```

Query in Grafana via LogQL:
```logql
{app_kubernetes_io_name="kmp"} | json | quality_compression_ratio > 0
```

### OTel (metrics)

The `OTelQualityObserver` records 5 histograms:

- `rehydration.quality.raw_equivalent_tokens` (u64)
- `rehydration.quality.compression_ratio` (f64)
- `rehydration.quality.causal_density` (f64)
- `rehydration.quality.noise_ratio` (f64)
- `rehydration.quality.detail_coverage` (f64)

Exported via OTLP gRPC to the OTel Collector, then scraped by Prometheus.

Query in Grafana:
```promql
rehydration_quality_compression_ratio_bucket{rpc="GetContext"}
```

## P0 Bug Fixes (6/6)

| Bug | Root Cause | Fix |
|-----|-----------|-----|
| Missing `caused_by_node_id` | Kernel omitted field from raw dump | Added to `raw_dump_text()` |
| Detail formatting | `\n` vs inline ` ` | Kernel matches testkit: inline |
| Semantic class format | `.as_str()` vs `{:?}` | Testkit changed to `.as_str()` |
| Zero test coverage | No tests for 5 metrics | 15 domain + 8 app integration tests |
| `unwrap_or_default()` | Silent zero on missing quality | `.ok_or("missing quality")` |
| OTel only in get_context | get_context_path missing | Observer covers both RPCs |

## Consequences

- **Positive:** Quality metrics are a first-class domain concept with invariants.
  Adding a new backend (e.g., CloudWatch, Datadog) requires only implementing the
  trait — no changes to domain, application, or transport layers.
- **Positive:** Compression ratio is now trustworthy for paper claims. Kernel and
  testkit produce identical raw dump text.
- **Update (superseded since this ADR):** `rehydrate_session()` now renders
  per-role bundles and emits per-role quality metrics through the same observer
  (`QualityObservationContext { rpc: "RehydrateSession", .. }`). All three render
  RPCs — `GetContext`, `GetContextPath`, and `RehydrateSession` — are covered.
  `get_node_detail()` stays uninstrumented because it returns a single node
  without a rendered bundle.
