# Enterprise observability

The enterprise server emits structured logs and can export OpenTelemetry over
OTLP. Telemetry is diagnostic: it is not required for memory writes or reads.

## What the chart exposes

- `observability.logFormat` selects server log formatting;
- `observability.otlpEndpoint` points at an operator-managed collector;
- `observability.serviceName` sets the OpenTelemetry service name;
- `otelCollector.enabled` can deploy a reference collector;
- `loki.enabled` can deploy a reference log store;
- chart-managed Grafana is deprecated and disabled by default.

The optional chart components are development or reference integrations, not
a production monitoring architecture. Prefer the organization's existing
collector, metrics backend, log store and dashboards.

## Signals to operate

At minimum, alert on:

- kernel or projection process restarts;
- failed or stalled NATS projection handling;
- gRPC error and latency changes;
- storage connectivity and capacity for Neo4j, Valkey and NATS;
- TLS, OIDC discovery or JWT validation failures at enabled boundaries;
- low relation/evidence quality only as a product-quality signal, never as a
  replacement for availability monitoring.

Do not place memory evidence, tokens, connection URIs or certificate material
in logs or metric labels.

## Verify the configured path

Render the chart and inspect the resulting environment and mounts. Test that
signals reach the selected collector in the target environment. A green chart
render only proves configuration shape; it does not prove that the external
observability service is reachable.

## Authority

- [`crates/kmp-observability`](../../crates/kmp-observability/)
- [`crates/kmp-server`](../../crates/kmp-server/)
- [`distribution/charts/kmp/values.yaml`](../../distribution/charts/kmp/values.yaml)
- chart templates for the collector and log components
