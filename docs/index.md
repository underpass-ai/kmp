# Documentation Index

Navigation hub for kernel documentation.

Current integration stance:

- `GraphBatch -> translator -> async projection events` is the recommended write path for model-driven producers.
- The stable kernel boundary remains typed gRPC plus async projection subjects.
- Kernel Memory Protocol is implemented API-first over domain/application/gRPC;
  MCP is an adapter over that API, not the owner of memory behavior.
- The active product roadmap is
  [`product/kernel-roadmap-milestones.md`](product/kernel-roadmap-milestones.md).
- Documentation authority and historical/obsolete areas are cataloged in
  [`documentation-catalog.md`](documentation-catalog.md).
- The dedicated `repair-judge` is an experimental stabilization helper for model extraction. It is not part of the stable kernel contract.

## Guides

| Document | Content |
|:---------|:--------|
| [usage-guide.md](usage-guide.md) | Getting started: 3 steps to graph-aware LLM context |
| [documentation-catalog.md](documentation-catalog.md) | Authoritative docs, historical docs, active roadmap, and documentation hygiene rules |
| [graph-batch-quickstart.md](graph-batch-quickstart.md) | Fastest path for model-driven graph ingestion |
| [beta-status.md](beta-status.md) | v1beta1 maturity matrix, path to v1, known limitations |
| [security-model.md](security-model.md) | Transport security, threat model, TLS configuration |
| [testing.md](testing.md) | Unit, integration, benchmark, live vLLM smoke, and experimental repair-judge coverage |
| [observability.md](observability.md) | Quality metrics, OTel, Loki, Grafana stack |

## Product Plans

| Document | Content |
|:---------|:--------|
| [kernel-context-traversal-and-mcp-action-plan.md](product/kernel-context-traversal-and-mcp-action-plan.md) | Public Kernel 1.0 plan/status for Kernel Memory Protocol, typed gRPC, MCP adapter, and honest benchmark follow-up |
| [kernel-roadmap-milestones.md](product/kernel-roadmap-milestones.md) | Milestone roadmap for queryable agentic memory, benchmarks, observability, plugins, and infrastructure independence |
| [kernel-context-api-design.md](product/kernel-context-api-design.md) | Kernel Memory Protocol design: ingest, wake, ask, temporal movement, trace, inspect, and MCP/gRPC/NATS bindings |
| [queryable-agentic-memory-layer.md](product/queryable-agentic-memory-layer.md) | Product direction for temporal, multidimensional memory as a queryable persistence layer for humans and LLMs |
| [kernel-memory-service-grpc-plan.md](product/kernel-memory-service-grpc-plan.md) | API-first plan for the typed `KernelMemoryService` gRPC boundary and MCP live-mode migration |
| [kernel-plugin-architecture.md](product/kernel-plugin-architecture.md) | Exportable plugin API architecture and implementation guide for external plugin crates |
| [kernel-write-protocol-plan.md](product/kernel-write-protocol-plan.md) | Writer-first plan for LLM/human memory ingestion above canonical `kernel_ingest` |
| [kernel-tool-operator-model-plan.md](product/kernel-tool-operator-model-plan.md) | P1 plan for a small sidecar model that learns bounded KMP/MCP tool operation from audited trajectories |
| [operator-training-experiment-process.md](product/operator-training-experiment-process.md) | Process for versioned Operator training attempts, dataset provenance, stop gates, and required evidence |
| [operator-training-runs/](product/operator-training-runs/README.md) | Per-attempt Operator training records |
| [operator-benchmark-status-and-next-steps-2026-05-14.md](product/operator-benchmark-status-and-next-steps-2026-05-14.md) | Current Operator benchmark status, MemoryArena vs LongMemEval boundaries, and next execution steps |
| [operator-mcp-api-contract-gap-audit-2026-05-14.md](product/operator-mcp-api-contract-gap-audit-2026-05-14.md) | Contract coverage audit for Operator vs KMP/MCP tools, cursor modes, dimensions, pagination, and required P0 datasets |
| [kernel-tool-operator-publication-plan.md](product/kernel-tool-operator-publication-plan.md) | Hugging Face publication gate, model/dataset packaging, and repository visibility checklist |
| [huggingface/README.md](product/huggingface/README.md) | Draft Hugging Face model card, dataset card, eval summary, and repo visibility assets |

## Operations

| Document | Content |
|:---------|:--------|
| [kubernetes-deploy.md](operations/kubernetes-deploy.md) | Helm deployment, values, TLS, observability stack |
| [container-image.md](operations/container-image.md) | OCI image, environment variables, tags |
| [mcp-stdio.md](operations/mcp-stdio.md) | Local stdio MCP adapter for Kernel Memory Protocol tools |
| [viewer.md](operations/viewer.md) | Local web viewer: graph, notes, timeline, traces over the embedded kernel |
| [kubernetes-transport-smoke.md](operations/kubernetes-transport-smoke.md) | In-cluster TLS smoke test |
| [deployment-boundary.md](operations/deployment-boundary.md) | What this repo owns vs. what it does not |

## Architecture Decisions

| ADR | Decision |
|:----|:---------|
| [ADR-007](adr/ADR-007-quality-metrics-observability.md) | BundleQualityMetrics as domain VO + hexagonal observer port |
| [ADR-008](adr/ADR-008-graph-batch-ingestion-boundary.md) | GraphBatch as the experimental ingestion boundary |
| [ADR-009](adr/ADR-009-embedded-storage-engine.md) | redb as the embedded edition storage engine |
| [ADR-010](adr/ADR-010-embedded-graph-representation.md) | Embedded graph as materialized adjacency, no in-memory rebuild |
| [ADR-011](adr/ADR-011-embedded-concurrency-model.md) | Embedded single-writer lock, fail-fast; daemon as evolution |
| [ADR-012](adr/ADR-012-embedded-data-directory.md) | Embedded data dir: env override, project `.kernel/`, XDG fallback |
| [ADR-013](adr/ADR-013-embedded-packaging.md) | One binary: `embedded` backend feature in `rehydration-mcp` |
| [ADR-014](adr/ADR-014-embedded-quality-telemetry.md) | Separate fail-open redb journal for embedded quality telemetry |
| [ADR-015](adr/ADR-015-consumer-memory-api-contract.md) | `rehydration-memory-api` — the contract an embedding product compiles against |
| [ADR-016](adr/ADR-016-consumer-memory-record-contract.md) | The consumer contract gains a record surface |
| [ADR-017](adr/ADR-017-embedded-memory-viewer.md) | In-process web viewer over the embedded kernel |

Six earlier decisions (command/query split, projection model, TLS, compatibility
removal, multi-resolution tiers) are documented in PRs but not yet written as
formal ADRs. See [adr/README.md](adr/README.md) for the source PRs.

## Current Integration Contracts

| Document | Content |
|:---------|:--------|
| [kernel-node-centric-integration-contract.md](migration/kernel-node-centric-integration-contract.md) | Public gRPC + async contract for consumers |
| [kernel-runtime-integration-reference.md](migration/kernel-runtime-integration-reference.md) | Recommended consumer runtime shape |
| [graph-batch-ingestion-api.md](graph-batch-ingestion-api.md) | Experimental ingress API proposal over GraphBatch |

## Historical Migration References

PIR and fix-planning migration notes are retained for traceability but are not
the current kernel contract. Use [migration/README.md](migration/README.md) and
[documentation-catalog.md](documentation-catalog.md) to decide whether a note is
still relevant before applying it to current work.

## Research

Papers, roadmaps, benchmarks, and incident reports: [research/](research/README.md)

LaTeX submission package: [paper/](paper/README.md)

## Archived

Historical and superseded documents: [archived/](archived/README.md)
