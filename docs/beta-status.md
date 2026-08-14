# v1beta1 RPC Maturity Matrix

Status of each RPC in the `v1beta1` contract surface.

## Path to v1

The kernel targets a stable `v1` contract once these conditions are met:

| Condition | Status |
|:----------|:-------|
| All deprecated proto fields removed or implemented | Done — 8 fields pruned, numbers reserved |
| Experimental GraphBatch ingestion boundary documented and validated | Done — live vLLM smoke + minimal and incremental E2E |
| GraphBatch retry/timeout policy documented | Done — client-owned retries, idempotency required, no sidecar assumption |
| GraphBatch transport API frozen as stable public contract | Not started |
| Authorization backend for scope validation | Not started |
| Timeline and summary filtering in RehydrateSession | Not started |
| Quality metrics in all render paths (including RehydrateSession) | Done — GetContext, GetContextPath, and RehydrateSession emit quality metrics |
| Full benchmark matrix validated (3 agents x 3 judges x 4 noise) | Planned — see [kernel-roadmap-milestones.md](product/kernel-roadmap-milestones.md) |
| Async contract: `context.bundle.generated` actually emitted by runtime | Not started — contract-only today |
| OTLP mTLS to OTel Collector | Done — env-var TLS config, Helm wiring, cert-manager |
| Neo4j client mTLS (2FA since Neo4j 5.19+) | Partial — Helm + URI parsing done, neo4rs client cert pending |
| Grafana: disable anonymous admin access by default | Done — default `false`, toggle via `grafana.anonymousAccess` |
| Event store atomic CAS for optimistic concurrency | Done — NATS `expected_last_subject_sequence` + Valkey Lua EVAL CAS |
| Breaking change window communicated to consumers | Not started |

Until then, the `v1beta1` contract is **stable for current fields** — no breaking
changes to fields that are implemented. Deprecated fields may be removed in `v1`.

## ContextQueryService

| RPC | Status | Notes |
|-----|--------|-------|
| `GetContext` | Production-ready | `scope_validation` is `None` (no authorization backend). |
| `GetContextPath` | Production-ready | Returns `NOT_FOUND` when neither the path nor the target node exist. |
| `GetNodeDetail` | Production-ready | |
| `RehydrateSession` | Production-ready | `timeline_window` is echoed in the response but does not filter events. `snapshot_ttl` is required when `persist_snapshot` is true. |
| `ValidateScope` | Production-ready | Standalone set comparison — only `required_scopes` and `provided_scopes` are evaluated. |

## ContextCommandService

| RPC | Status | Notes |
|-----|--------|-------|
| `UpdateContext` | Production-ready | Persists full domain events (changes, requested_by, occurred_at) to the context event store as JSON. Optimistic concurrency via `expected_revision`. Content hash validated via `expected_content_hash`. Idempotency via `idempotency_key`; replaying a key with different content returns `ABORTED`. Returns `ABORTED` on revision or hash conflict. |

## KernelMemoryService

Typed memory API in `api/proto/underpass/rehydration/kernel/v1beta1/memory.proto`.
This is the API-first surface for Kernel Memory Protocol moves. It is additive;
`ContextQueryService` and `ContextCommandService` remain supported lower-level
kernel services.

| RPC | Status | Notes |
|-----|--------|-------|
| `Ingest` | Production-ready | Validates dimensions, entries, coordinates, relations, evidence, idempotency, provenance, and positive temporal coordinates. `about` namespaces submitted dimensions internally as `about:<about>:dimension:<dimension_id>`. Relation/evidence refs may be submitted in the request or already materialized in the read model. |
| `Wake` | Production-ready | Reads live context through the application query port. Honors budget detail, dimension selection, and dimension about scope. |
| `Ask` | Production-ready for deterministic memory answers | Honors answer policy, budget detail, dimension selection, and dimension about scope. Does not generate novel answers; `answer` is derived from selected evidence reasons, not from an anchor summary. `evidence_or_unknown` returns `UNKNOWN` when no evidence is available. |
| `Goto` | Production-ready | Domain-owned temporal traversal over `contains_entry` coordinates. Honors dimensions, window, entry limit, token limit, and include flags. |
| `Near` | Production-ready | Domain-owned temporal neighborhood traversal. |
| `Rewind` | Production-ready | Domain-owned backward traversal. |
| `Forward` | Production-ready | Domain-owned forward traversal. |
| `Trace` | Production-ready | Uses `GetContextPath` semantics and maps `goal` to the trace role. |
| `Inspect` | Production-ready for object/detail/link/raw audit lookup | Honors `details=false`. Explicit `incoming` and `outgoing` use the typed node relationship reader. `raw=true` returns typed raw audit refs for the inspected object. |

Dimension selection scope defaults to `CURRENT_ABOUT`. `ABOUTS` is valid only
with a non-empty `abouts` list. `ALL_ABOUTS` uses the kernel memory about index
to traverse every memory anchor. Temporal coverage preserves the requested
scope for audit instead of normalizing `CURRENT_ABOUT` to an `ABOUTS` list.
Temporal `raw_refs=true` returns typed raw audit refs for selected entries.
Temporal movement responses expose `PageInfo` through gRPC and MCP, including
`returned`, `total`, `has_more`, and `next_cursor` when a bounded move is
partial.
`Ask` currently uses deterministic evidence for all answer policies. Explicit
conflict relations are surfaced in `proof.conflicts`; inferred conflict
resolution and generated/best-effort fallback text are not implemented.

## Async Contract (NATS JetStream)

Event channels defined in [`context-projection.v1beta1.yaml`](../api/asyncapi/context-projection.v1beta1.yaml).

| Channel | Direction | Status | Notes |
|:--------|:----------|:-------|:------|
| `graph.node.materialized` | Subscribe (kernel consumes) | Production-ready | Materializes nodes + relationships into Neo4j. Full `EventEnvelope` with 7 required fields + `data` payload. `related_nodes` carries explanatory metadata (semantic_class, rationale, method, decision_id, caused_by_node_id, evidence, confidence, sequence) |
| `graph.relation.materialized` | Subscribe (kernel consumes) | Implemented experimental | Materializes one relation without re-materializing the source node. Runtime routing, AsyncAPI contract, projection mapping, and initial PIR-like container integration are implemented. Out-of-order convergence and idempotency-specific relation tests are still pending. |
| `node.detail.materialized` | Subscribe (kernel consumes) | Production-ready | Materializes extended detail into Valkey. Requires `node_id`, `detail`, `content_hash`, `revision` |
| `context.bundle.generated` | Publish (kernel emits) | **Contract only** | Defined in AsyncAPI but **not emitted by the kernel runtime**. Test fixtures simulate it. Implementation pending |

All channels use the shared `EventEnvelope` schema: `event_id`, `correlation_id`,
`causation_id`, `occurred_at`, `aggregate_id`, `aggregate_type`, `schema_version`.

Subject prefix is configurable via `KMP_EVENTS_PREFIX` (default: `rehydration`).

## Experimental Ingestion Boundary

The repo now documents and validates an experimental `GraphBatch` ingestion
shape for model-driven producers.

Status:

- **Recommended for model producers** — yes
- **Stable `v1beta1` transport contract** — no

What is stable:

- the async projection subjects above
- the gRPC query surface

What is implemented but still experimental:

- `graph.relation.materialized` as an additive relation-only async subject

What remains experimental:

- any direct ingress API that accepts `GraphBatch` as a request body
- the dedicated `repair-judge` helper used to salvage invalid model output before translation

What the experimental `repair-judge` is:

- an optional second-pass model used by the testkit and cluster examples
- a repair layer over invalid `GraphBatch` output from the primary model
- not part of the stable gRPC or async contract surface

See:

- [graph-batch-quickstart.md](graph-batch-quickstart.md)
- [graph-batch-ingestion-api.md](graph-batch-ingestion-api.md)
- [ADR-008](adr/ADR-008-graph-batch-ingestion-boundary.md)

## Removed

- **ContextAdminService** — removed entirely. The admin RPCs (`GetProjectionStatus`, `ReplayProjection`, `GetBundleSnapshot`, `GetGraphRelationships`, `GetRehydrationDiagnostics`) were placeholder-backed and have been deleted.
- **HTTP admin** — never implemented; crate deleted.

## Quality Metrics (v1beta1)

`BundleQualityMetrics` is a domain value object computed on every `GetContext` and
`GetContextPath` render. Five metrics with invariant validation:

| Metric | Description |
|--------|-------------|
| `raw_equivalent_tokens` | Flat text baseline token count |
| `compression_ratio` | raw / rendered ratio (>1.0 = compression) |
| `causal_density` | Fraction of explanatory relationships |
| `noise_ratio` | Fraction of noise/distractor nodes |
| `detail_coverage` | Fraction of nodes with detail |

Delivered through the `QualityMetricsObserver` hexagonal port with two active adapters:
- **OTel**: 5 histograms via OTLP (see [observability.md](observability.md))
- **Tracing**: structured JSON logs for Loki/Grafana

All three render RPCs (`GetContext`, `GetContextPath`, `RehydrateSession`) emit quality
metrics. `RehydrateSession` renders per-role bundles and emits quality via the observer.

## Known Limitations

**Not implemented in v1beta1:**

- **No authorization backend (deliberate)** — `ValidateScope` is a pure set-comparison utility, not an access control gate. `GetContext` does not invoke scope validation. This is a conscious design decision for v1beta1: the kernel delegates access control to the transport layer (mTLS client certificates) and the integrating product. Scope enforcement is not planned for the kernel itself — consumers are expected to validate scopes at their own boundary
- **No timeline filtering** — `RehydrateSession` echoes `timeline_window` but does not filter events by time range
- **`context.bundle.generated` not emitted** — defined in AsyncAPI contract but the kernel runtime does not publish this event. Test fixtures simulate it for downstream integration tests
- **No generated `Ask` answers** — `KernelMemoryService.Ask` returns deterministic evidence-derived answer text or `UNKNOWN` according to the answer policy.
- **Single token estimator** — `cl100k_base` BPE via `tiktoken-rs` for all counting. No model-specific estimator selection
- **Idempotency outcome** — outcome publish is fire-and-forget. If it fails, retries are treated as new requests

**Implemented in this version:**

- MCP live adapter is a thin `KernelMemoryService` client for KMP tools. It has
  no compatibility fallback to `ContextQueryService` or `ContextCommandService`.
- `KernelMemoryService` emits structured request/response/error logs for all
  nine KMP RPCs, including dimension mode/scope/abouts/scope IDs where
  applicable, resolved `selected_abouts` on dimensioned read responses, and
  tonic code/message on errors.
- **Event store atomic CAS** — NATS uses `expected_last_subject_sequence` header; Valkey uses a Lua EVAL script. Both reject concurrent writes with `PortError::Conflict`. Validated with container integration tests.
- **Async quality observer** — `CompositeQualityObserver` spawns observer calls via `tokio::spawn` (fire-and-forget). Observer I/O no longer blocks the gRPC handler hot path.
- **TruncationMetadata in proto** — `RenderedContext.truncation` (field 8) carries budget_requested, budget_used, sections_kept, sections_dropped, token_estimator when a budget is applied.
- **Render content hash** — `RenderedContext.content_hash` (field 9) is a deterministic hash of the flat rendered content for audit verification.
- **Provenance on relationships** — `GraphRelationship.provenance` (field 5) carries source_kind, source_agent, observed_at — same as nodes.
- **Per-role quality in RehydrateSession** — `GraphRoleBundle.rendered` (field 6) carries per-role RenderedContext with quality metrics, tiers, truncation, and resolved mode.
- **Planner v2** — `mode_heuristic.rs` uses causal density alongside token pressure. High causal density (>50%) keeps ReasonPreserving even under budget pressure.
