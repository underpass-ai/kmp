# Architecture Decision Records

## Recorded

| ADR | Decision | PR |
|:----|:---------|:---|
| [ADR-007](ADR-007-quality-metrics-observability.md) | BundleQualityMetrics as domain VO + hexagonal observer port | — (2026-03-27) |
| [ADR-008](ADR-008-graph-batch-ingestion-boundary.md) | GraphBatch as the experimental ingestion boundary | — (2026-04-08) |
| [ADR-009](ADR-009-embedded-storage-engine.md) | redb as the embedded edition storage engine | — (2026-07-21) |
| [ADR-010](ADR-010-embedded-graph-representation.md) | Embedded graph as materialized adjacency, no in-memory rebuild | — (2026-07-21) |
| [ADR-011](ADR-011-embedded-concurrency-model.md) | Embedded single-writer lock, fail-fast; daemon as evolution | — (2026-07-21) |
| [ADR-012](ADR-012-embedded-data-directory.md) | Embedded data dir: env override, project `.kernel/`, XDG fallback | — (2026-07-21) |
| [ADR-013](ADR-013-embedded-packaging.md) | One binary: `embedded` backend feature in `rehydration-mcp` | — (2026-07-21) |
| [ADR-014](ADR-014-embedded-quality-telemetry.md) | Separate fail-open redb journal for embedded quality telemetry | — (2026-07-22) |
| [ADR-015](ADR-015-consumer-memory-api-contract.md) | `rehydration-memory-api` — the contract an embedding product compiles against | — (2026-08-04) |

## Not Yet Written (decisions implicit in PRs)

These decisions were made but not recorded as ADRs. Writing them is planned
to improve traceability.

| ADR | Decision | PR | Date |
|:----|:---------|:---|:-----|
| ADR-001 | Command/query separation + AsyncAPI event contract | #1 | 2025-12 |
| ADR-002 | Node-centric projection model (Neo4j graph + Valkey detail) | #4-#6 | 2025-12 |
| ADR-003 | Compatibility bridge for migration (later removed in #52) | #8-#12 | 2026-01 |
| ADR-004 | Full TLS/mTLS on all transport boundaries | #32-#36 | 2026-02 |
| ADR-005 | Remove v1alpha1 compatibility — clean v1beta1 cut | #52, #57 | 2026-03 |
| ADR-006 | Multi-resolution tiers (L0/L1/L2) + RehydrationMode heuristic | #63, #64 | 2026-03 |
