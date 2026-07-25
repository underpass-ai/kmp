# ADR-014: Embedded quality telemetry uses a separate fail-open redb journal

**Status:** Accepted
**Date:** 2026-07-22
**Context:** Embedded edition E3 follow-up to ADR-007 and ADR-013

## Decision

The embedded edition records `BundleQualityMetrics` locally without requiring
an OTel Collector, Prometheus, Loki, or any other service:

- `QualityMetricsObserver` remains the only domain port;
- `BufferedQualityMetricsObserver` implements that port with bounded
  `try_send`; a full or disconnected channel increments a drop counter and
  never blocks or fails a kernel read;
- `EmbeddedTelemetryGuard` owns a plain OS worker thread, batches observations,
  drains on shutdown, and performs a final durable flush;
- `RedbQualityTelemetryWriter` persists batches to
  `<data-dir>/telemetry/quality.redb`;
- `RedbQualityTelemetryReader` provides bounded time-window and latest-first
  queries for maintenance tooling;
- the embedded composition root owns the observer, worker, and writer. The MCP
  adapter only emits successful `kernel_wake`, `kernel_ask`, and `kernel_trace`
  renders through the domain port.

`rehydration-observability` keeps OTEL behind its `otel` Cargo feature. The
crate enables that feature by default for direct deployed use, while workspace
consumers opt in explicitly and the embedded adapter/composition remain on the
local-only feature set. Therefore the in-process kernel does not link an OTLP
client, `reqwest`, `tonic`, or protobuf code merely to obtain the buffered
observer and telemetry guard.

No embedded-only MCP tool is added. The KMP protocol surface remains identical
between live and embedded editions. Local inspection starts as an out-of-band
maintenance API; it can become a shared protocol operation only if every
edition can implement the same semantics.

## Storage And Durability

Telemetry does not share `store/kernel.redb` with canonical memory. This gives
it independent retention and durability policies:

- canonical memory keeps the ADR-009 immediate-durability crash contract;
- telemetry batches normally commit with `redb::Durability::None`;
- every sixteenth batch commits with `Durability::Immediate`, making preceding
  relaxed commits durable;
- clean worker shutdown performs an additional immediate empty commit, so only
  the telemetry tail may be lost after an abrupt process crash;
- retention defaults to the newest 100,000 observations and is enforced in the
  same transaction as each batch;
- keys are `(observed_at_millis, sequence)`. The writer recovers the highest
  sequence on reopen, avoiding restart collisions even when timestamps repeat.

Opening the telemetry database or starting its worker is fail-open. The kernel
continues with a disconnected bounded observer, exposes the startup error, and
counts subsequent discarded observations. Write failures are counted by the
writer and never affect memory reads or writes.

## Why

Embedded KMP must be useful on a developer machine with zero infrastructure,
while preserving ADR-007's optional OTel path for deployed environments. A
JSONL append log would be simple but would not provide structured retention or
efficient time-window scans. Sharing the canonical database would either force
telemetry to pay an fsync per commit or weaken the memory crash contract.

Keeping the invocation hook out of the application layer preserves the
hexagonal boundary: quality telemetry describes successful rendered transport
operations, not domain state transitions.

## Consequences

- **Positive:** local-first quality history is queryable with no services.
- **Positive:** zero-infrastructure is also a binary dependency property, not
  only a runtime configuration.
- **Positive:** the kernel hot path remains non-blocking and fail-open.
- **Positive:** canonical state and operational telemetry retain independent
  crash guarantees and retention policies.
- **Trade-off:** an abrupt crash may lose the most recent relaxed telemetry
  batches; this is explicit and acceptable.
- **Trade-off:** the initial reader is a maintenance surface, not an MCP tool.
- **Constraint:** future OTEL + local fan-out must use the existing
  `CompositeQualityObserver`; it must not introduce a second domain port.
- **Constraint:** CI rejects remote observability dependencies in the
  `rehydration-embedded` normal dependency graph.
