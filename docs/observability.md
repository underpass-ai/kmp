# Observability

The kernel emits quality metrics through a hexagonal observer port with pluggable
backends. Every successful render (`GetContext`, `GetContextPath`, `RehydrateSession`)
produces metrics that flow through both OTel (Prometheus/Grafana) and structured logs
(Loki/Grafana).

Architecture reference: [ADR-007](adr/ADR-007-quality-metrics-observability.md)

Embedded local-telemetry reference:
[ADR-014](adr/ADR-014-embedded-quality-telemetry.md).

Product direction reference:
[Queryable Agentic Memory Layer](product/queryable-agentic-memory-layer.md).

## Current Status And Gap

Current observability is useful but incomplete for the agentic-memory product
direction.

Implemented:

- bundle quality metrics through a hexagonal observer port;
- OTel histograms/counters for render RPCs;
- structured JSON quality logs;
- structured KMP gRPC request, response, and error logs;
- structured MCP tool logs for every KMP move, including safe request/result
  counts and writer relation quality;
- OTel MCP counters and histograms for tool calls, duration, request counts,
  result warnings, path length, writer relation quality, and writer
  read-context coverage;
- OTel `KernelMemoryService` counters and histograms for gRPC calls, duration,
  success/error status, and tonic status code;
- projection processing latency via `rehydration.projection.lag` for NATS
  projection consumer loops;
- OTel mTLS configuration support.
- embedded quality observations for `kernel_wake`, `kernel_ask`, and
  `kernel_trace`, buffered fail-open into a separate
  `telemetry/quality.redb` journal with retention and time-window queries.

Still important:

- traces are disabled by default in the in-chart collector;
- there is no full end-to-end trace from KMP request through event append,
  projection, traversal, render, LLM consumption, and optional write-back;
- agentic process metrics are not yet complete: proof quality, known-at-time
  correctness, replay completeness, failed/successful path coverage, and
  decision traceability;
- dashboards should focus on agent outcomes, not only infrastructure health.

Observability is part of the kernel product. A temporal, multidimensional memory
layer must show not only that storage is healthy, but whether agents received
the right memory, at the right time, with enough proof.

The product target is observable, auditable, and secure. Logs, metrics, and
traces must support audit without leaking unnecessary raw memory or secrets.

## Agentic Metrics Backlog

These metric families should be treated as product metrics for agentic memory.

| Metric family | Status | Purpose |
|:--------------|:-------|:--------|
| KMP request count, latency, and errors by move | implemented for MCP tools and `KernelMemoryService` gRPC RPCs | Prove agents can reliably call memory moves. |
| Ingest accepted/rejected counts | partial via KMP logs; MCP records canonical request/result counts | Track entries, relations, evidence, dimensions, and validation failures. |
| Projection lag | implemented for NATS projection consumer processing time | Track projection handler latency; full publish-to-queryable lag remains a backlog metric. |
| Idempotency outcomes | partial logs | Detect duplicate writes, replay behavior, unsafe retries, and conflicts. |
| Traversal scope | partial logs; MCP records dimension scope/mode counts and path length | Observe selected abouts, dimensions, temporal windows, path length, and hop count. |
| Proof quality | partial via MCP result evidence/path/warning counts | Track evidence count, missing count, warning count, conflict count, and weak-proof count. |
| Known-at-time correctness | planned | Detect whether a response respected requested temporal bounds. |
| Replay completeness | planned | Measure failed attempts, successful terminal states, and final path coverage. |
| Context quality | implemented for render paths | Track compression ratio, causal density, noise ratio, token pressure, and detail coverage. |
| Consumer outcome | planned | Track unknown rate, missing-evidence rate, answer-with-proof rate, and trace-used rate. |
| Security and audit | planned | Track auth mode, rejected requests, denied scopes, redaction counts, inspect/raw access counts, and audit-event emission. |

Cardinality rule: labels must not include raw `about`, user ids, full refs, or
free-form questions. High-cardinality values belong in structured logs or trace
attributes with sampling, not metric labels.

Privacy rule: structured logs and trace attributes must not include secrets,
credentials, API keys, full raw prompts, or unrestricted raw memory details.
Use stable refs, counts, hashes, and authorized inspect handles instead.

## MCP Tool Observability

The MCP adapter emits one structured log event per completed or rejected
`tools/call` request:

```text
event=kernel_mcp_tool
kmp_move=kernel_write_memory|kernel_ingest|kernel_wake|...
backend=fixture|grpc
grpc_tls=disabled|server|mutual
status=success|error
error_kind=none|validation|backend
duration_ms=<milliseconds>
```

The event records only safe shapes:

- request counts: dimensions, entries, relations, evidence, `connect_to`, and
  `read_context` refs;
- selection shape: dimension mode, scope, explicit about count, dimension
  filter count, and scope id count;
- result counts: warnings, entries, relations, evidence, path length, and raw
  ref count;
- writer quality: rich/anemic/structural/suspect relation counts and
  read-context required/observed counts.

It does not log raw entry text, evidence text, natural-language questions, raw
memory bodies, API keys, or backend error strings. Backend error messages are
represented by a stable short hash plus low-cardinality `error_kind`.

The MCP adapter also emits OTel metrics:

| Metric | Type | Labels | Description |
|:-------|:-----|:-------|:------------|
| `rehydration.kmp.tool.calls` | counter | `move`, `backend`, `grpc_tls`, `status`, `error_kind` | Tool call volume and failure classification. |
| `rehydration.kmp.tool.duration` | histogram | `move`, `backend`, `grpc_tls`, `status`, `error_kind` | End-to-end MCP tool latency. |
| `rehydration.kmp.request.entries` | histogram | `move`, `backend` | Canonical write entry counts. |
| `rehydration.kmp.request.relations` | histogram | `move`, `backend` | Canonical write relation counts. |
| `rehydration.kmp.request.evidence` | histogram | `move`, `backend` | Canonical write evidence counts. |
| `rehydration.kmp.result.warnings` | histogram | `move`, `backend` | Warnings returned by the move. |
| `rehydration.kmp.result.path_length` | histogram | `move`, `backend` | Trace/proof path length when present. |
| `rehydration.kmp.writer.relations` | counter | `quality` | Writer relation quality volume. |
| `rehydration.kmp.writer.read_context.required` | histogram | `move`, `backend` | Rich external relations requiring prior read context. |
| `rehydration.kmp.writer.read_context.observed` | histogram | `move`, `backend` | Rich external relations backed by prior read context. |

## KernelMemoryService gRPC Metrics

The typed Kernel Memory gRPC facade emits service-level OTel metrics for every
RPC in `KernelMemoryService`:

| Metric | Type | Labels | Description |
|:-------|:-----|:-------|:------------|
| `rehydration.kmp.grpc.calls` | counter | `rpc`, `status`, `code` | gRPC request volume and success/error classification. |
| `rehydration.kmp.grpc.duration` | histogram | `rpc`, `status`, `code` | End-to-end handler latency including validation and application execution. |

`status` is `success` or `error`. `code` is `none` for success and the low
cardinality tonic status label for failures, for example `invalid_argument`,
`not_found`, `internal`, or `unavailable`.

## BundleQualityMetrics

A domain value object computed on every render. Invariant-validated at construction.

| Metric | Type | Range | Description |
|:-------|:-----|:------|:------------|
| `raw_equivalent_tokens` | u32 | >= 0 | Token count for a flat text dump of the same data |
| `compression_ratio` | f64 | >= 0.0 | `raw_equivalent_tokens / rendered_tokens`. >1.0 = compression |
| `causal_density` | f64 | [0.0, 1.0] | Fraction of causal/motivational/evidential relationships |
| `noise_ratio` | f64 | [0.0, 1.0] | Fraction of noise/distractor nodes |
| `detail_coverage` | f64 | [0.0, 1.0] | Fraction of nodes with extended detail |

Computed by `BundleQualityMetrics::compute(bundle, rendered_tokens, estimator)` in the
domain layer. The testkit provides `seed_to_bundle()` to convert generated seeds into
domain bundles, and `seed_raw_equivalent_tokens()` to compute raw tokens through the
same code path as the kernel. Token count parity is verified by test.

## Observer Port

```
QualityMetricsObserver (domain port)
├── OTelQualityObserver      → OTLP histograms → Prometheus → Grafana
├── TracingQualityObserver   → structured JSON logs → Loki → Grafana
├── BufferedQualityMetricsObserver → bounded local journal channel
├── CompositeQualityObserver → fan-out to N backends
└── NoopQualityObserver      → tests / disabled
```

The composition root creates `CompositeQualityObserver(OTel + Tracing)` by default.
Both backends are always active. The composite observer spawns adapter calls via
`tokio::spawn` (fire-and-forget) — observer I/O does not block the gRPC handler.

## Embedded Local Journal

Embedded mode does not require an OTel service. Its composition root connects a
bounded `BufferedQualityMetricsObserver` to a runtime-neutral worker and a
separate redb writer:

```text
kernel_wake | kernel_ask | kernel_trace
        → QualityMetricsObserver
        → bounded try_send (drop counter on overflow)
        → batch worker
        → <data-dir>/telemetry/quality.redb
```

`store/kernel.redb` remains canonical state with immediate durability.
`telemetry/quality.redb` uses relaxed batch durability, a periodic durable
checkpoint, a final clean-shutdown flush, and independent retention. Telemetry
startup and write failures never fail the kernel. The reader supports bounded
time windows, RPC filtering, latest observations, and counts without adding an
embedded-only MCP operation.

## OTel Metrics

### Emitted by `GetContext`

| Metric | Type | Description |
|:-------|:-----|:------------|
| `rehydration.rpc.duration` | f64 histogram (s) | RPC latency |
| `rehydration.bundle.nodes` | u64 histogram | Nodes in bundle |
| `rehydration.bundle.relationships` | u64 histogram | Relationships in bundle |
| `rehydration.bundle.details` | u64 histogram | Node details in bundle |
| `rehydration.rendered.tokens` | u64 histogram | Rendered token count |
| `rehydration.truncation.total` | u64 counter | Renders requiring truncation |
| `rehydration.mode.selected` | u64 counter | Resolved RehydrationMode (label: `mode`) |
| `rehydration.quality.*` | via observer | 5 quality metrics (see below) |
| `rehydration.session.*` | f64/u64 histograms | Timing breakdown (when available) |

### Emitted by `GetContextPath`

| Metric | Type | Description |
|:-------|:-----|:------------|
| `rehydration.rpc.duration` | f64 histogram (s) | RPC latency |
| `rehydration.bundle.nodes` | u64 histogram | Nodes in bundle |
| `rehydration.bundle.relationships` | u64 histogram | Relationships in bundle |
| `rehydration.bundle.details` | u64 histogram | Node details in bundle |
| `rehydration.rendered.tokens` | u64 histogram | Rendered token count |
| `rehydration.truncation.total` | u64 counter | Renders requiring truncation |
| `rehydration.mode.selected` | u64 counter | Resolved RehydrationMode (label: `mode`) |
| `rehydration.quality.*` | via observer | 5 quality metrics (see below) |
| `rehydration.session.*` | f64/u64 histograms | Timing breakdown (when available) |

### Emitted by `RehydrateSession`

| Metric | Type | Description |
|:-------|:-----|:------------|
| `rehydration.rpc.duration` | f64 histogram (s) | RPC latency |
| `rehydration.bundle.nodes` | u64 histogram | Per-role node count (label: `role`) |
| `rehydration.bundle.relationships` | u64 histogram | Per-role relationship count |
| `rehydration.bundle.details` | u64 histogram | Per-role detail count |
| `rehydration.rendered.tokens` | u64 histogram | Per-role rendered token count |
| `rehydration.truncation.total` | u64 counter | Per-role truncation events |
| `rehydration.mode.selected` | u64 counter | Per-role resolved mode |
| `rehydration.quality.*` | via observer | Per-role quality metrics (5 histograms) |
| `rehydration.session.*` | f64/u64 histograms | Timing breakdown |

### Quality Metrics (via observer, all three RPCs)

| Metric | Type | Label |
|:-------|:-----|:------|
| `rehydration.quality.raw_equivalent_tokens` | u64 histogram | `rpc` |
| `rehydration.quality.compression_ratio` | f64 histogram | `rpc` |
| `rehydration.quality.causal_density` | f64 histogram | `rpc` |
| `rehydration.quality.noise_ratio` | f64 histogram | `rpc` |
| `rehydration.quality.detail_coverage` | f64 histogram | `rpc` |

### Timing Breakdown (`GetContext` + `GetContextPath` + `RehydrateSession`)

| Metric | Type | Unit |
|:-------|:-----|:-----|
| `rehydration.session.graph_load.duration` | f64 histogram | seconds |
| `rehydration.session.detail_load.duration` | f64 histogram | seconds |
| `rehydration.session.bundle_assembly.duration` | f64 histogram | seconds |
| `rehydration.session.batch_size` | u64 histogram | — |
| `rehydration.session.role_count` | u64 histogram | — |

### Projection Runtime

| Metric | Type | Description |
|:-------|:-----|:------------|
| `rehydration.projection.lag` | f64 histogram (s) | Event processing time per message (label: `subject`) |

## Structured Logs (Loki)

When `REHYDRATION_LOG_FORMAT=json`, the `TracingQualityObserver` emits:

```json
{
  "timestamp": "2026-03-27T16:47:37Z",
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

`KernelMemoryService` also emits structured request, response, and error logs
at the gRPC boundary for every KMP RPC. These events use the message values
`kernel memory grpc request`, `kernel memory grpc response`, and
`kernel memory grpc error`.

Request logs include:

- `rpc`;
- `about` for memory reads/writes;
- `dimension_mode`, `dimension_scope`, `dimensions`, requested `abouts`, and
  `scope_ids` for dimensioned reads;
- submitted counts for `Ingest`;
- source and target refs for `Trace`;
- include flags for `Inspect`.

Response logs include:

- accepted counts and `read_after_write_ready` for `Ingest`;
- evidence/answer/warning counts for `Ask`;
- entry/warning counts for temporal traversal;
- `selected_abouts` for dimensioned reads after `CURRENT_ABOUT`, `ABOUTS`, or
  `ALL_ABOUTS` resolution;
- path/warning counts for `Trace`;
- incoming/outgoing/evidence/warning counts for `Inspect`.

Error logs include:

- `rpc`;
- tonic `code`;
- the mapped error `message`.

### LogQL Examples

```logql
# All quality metrics events
{app_kubernetes_io_name="rehydration-kernel"} | json | quality_compression_ratio > 0

# High compression renders
{app_kubernetes_io_name="rehydration-kernel"} | json | quality_compression_ratio > 2.0

# Low causal density (structural-heavy graphs)
{app_kubernetes_io_name="rehydration-kernel"} | json | quality_causal_density < 0.3

# Quality by RPC
{app_kubernetes_io_name="rehydration-kernel"} | json | rpc = "GetContext"

# KMP calls scoped to the current memory anchor
{app_kubernetes_io_name="rehydration-kernel"} | json | message = "kernel memory grpc request" | dimension_scope = "current_about"

# Intentional all-about KMP reads
{app_kubernetes_io_name="rehydration-kernel"} | json | message = "kernel memory grpc request" | dimension_scope = "all_abouts"

# Resolved memory anchors for all-about KMP reads
{app_kubernetes_io_name="rehydration-kernel"} | json | message = "kernel memory grpc response" | selected_abouts != ""

# KMP fail-fast or storage conflict errors
{app_kubernetes_io_name="rehydration-kernel"} | json | message = "kernel memory grpc error"
```

### PromQL Examples

The OTel Collector Prometheus exporter is configured with `namespace: rehydration`,
so all metric names are prefixed with `rehydration_`.

```promql
# Average compression ratio
histogram_quantile(0.5, rate(rehydration_quality_compression_ratio_bucket[5m]))

# P99 raw token count
histogram_quantile(0.99, rate(rehydration_quality_raw_equivalent_tokens_bucket[5m]))

# Causal density by RPC
histogram_quantile(0.5, rate(rehydration_quality_causal_density_bucket{rpc="GetContext"}[5m]))

# RPC latency P95
histogram_quantile(0.95, rate(rehydration_rpc_duration_bucket[5m]))
```

## Helm Configuration

Enable the full observability stack in your values overlay:

```yaml
loki:
  enabled: true

grafana:
  enabled: true
  adminPassword: admin  # change in production

otelCollector:
  enabled: true
```

When `otelCollector.enabled=true`, the kernel deployment automatically sets
`OTEL_EXPORTER_OTLP_ENDPOINT` to the in-chart collector service alias
`<release>-otel`. Helm uses
`http://...:4317` in plaintext mode and `https://...:4317` when
`otelCollector.tls.enabled=true`.
The in-chart collector currently runs metrics by default and optional logs via
Loki; traces are disabled by default by setting `OTEL_TRACES_EXPORTER=none`.

Grafana auto-provisions two datasources:
- **Loki** at `http://<release>-loki:3100`
- **Prometheus** at `http://<release>-otel:9090` (when OTel enabled)

### Accessing Grafana

```bash
kubectl port-forward svc/<release>-grafana 3000:3000 -n <namespace>
# Open http://localhost:3000 (admin/admin by default)
```

> **Security**: Embedded Grafana is disabled by default and
> `grafana.anonymousAccess=false` in base values. Some development overlays
> enable anonymous access for convenience; disable it for production. See
> [security-model.md](security-model.md).

## Environment Variables

| Variable | Default | Description |
|:---------|:--------|:------------|
| `REHYDRATION_LOG_FORMAT` | `compact` | Log format: `json` (for Loki), `pretty`, `compact` |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | — | OTLP gRPC endpoint (auto-set when otelCollector enabled) |
| `OTEL_TRACES_EXPORTER` | — | Standard OTel traces exporter selector. Set to `none` to disable trace export while keeping metrics enabled |
| `OTEL_EXPORTER_OTLP_CA_PATH` | — | CA certificate for OTLP server verification |
| `OTEL_EXPORTER_OTLP_CERT_PATH` | — | Client certificate for OTLP mTLS |
| `OTEL_EXPORTER_OTLP_KEY_PATH` | — | Client key for OTLP mTLS |
| `OTEL_SERVICE_NAME` | — | Override service name in OTel metadata |
| `RUST_LOG` | `info` | Log level filter |

## Limitations

- OTLP export supports mTLS via `OTEL_EXPORTER_OTLP_{CA,CERT,KEY}_PATH` env vars. Plaintext by default when no env vars set.
- `rehydration.projection.lag` currently records NATS projection consumer
  processing time, not full end-to-end publish-to-queryable latency.
- All three render RPCs (`GetContext`, `GetContextPath`, `RehydrateSession`)
  emit per-role quality metrics via the observer. `RehydrateSession` renders
  per-role bundles with quality, tiers, truncation, and resolved mode.
