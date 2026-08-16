# Documentation Catalog

Date: 2026-05-11
Status: active documentation hygiene map

This catalog separates authoritative documentation from historical artifacts.
When a document conflicts with this catalog, prefer the active documents below
and fix or archive the conflicting note.

## Authoritative Current Docs

These documents are the current source of truth for users and maintainers:

| Area | Document |
| --- | --- |
| Entry point | [README.md](../README.md) |
| Navigation | [index.md](index.md) |
| v1beta1 maturity and limitations | [beta-status.md](beta-status.md) |
| Product roadmap | [product/kernel-roadmap-milestones.md](product/kernel-roadmap-milestones.md) |
| Embedded edition roadmap | [product/kmp-embedded-edition-roadmap.md](product/kmp-embedded-edition-roadmap.md) |
| MADE incident-memory integration | [integrations/made-kmp.md](integrations/made-kmp.md) |
| Embedded host recipes + playbook | [operations/embedded-hosts.md](operations/embedded-hosts.md) |
| Embedded releases + format compatibility | [operations/embedded-release.md](operations/embedded-release.md) |
| Release process (tags, crates.io, artefacts) | [release.md](release.md) |
| KMP product/API design | [product/kernel-context-api-design.md](product/kernel-context-api-design.md) |
| Typed KMS/gRPC status | [product/kernel-memory-service-grpc-plan.md](product/kernel-memory-service-grpc-plan.md) |
| Writer helper protocol | [product/kernel-write-protocol-plan.md](product/kernel-write-protocol-plan.md) |
| Kernel tool-operator model | [product/kernel-tool-operator-model-plan.md](product/kernel-tool-operator-model-plan.md) |
| Operator training process | [product/operator-training-experiment-process.md](product/operator-training-experiment-process.md), [product/operator-training-runs/README.md](product/operator-training-runs/README.md) |
| Operator benchmark status | [product/operator-benchmark-status-and-next-steps-2026-05-14.md](product/operator-benchmark-status-and-next-steps-2026-05-14.md) |
| Operator MCP/API coverage audit | [product/operator-mcp-api-contract-gap-audit-2026-05-14.md](product/operator-mcp-api-contract-gap-audit-2026-05-14.md) |
| Operator publication gate | [product/kernel-tool-operator-publication-plan.md](product/kernel-tool-operator-publication-plan.md) |
| Hugging Face publication assets | [product/huggingface/README.md](product/huggingface/README.md) |
| Plugin architecture | [product/kernel-plugin-architecture.md](product/kernel-plugin-architecture.md) |
| Interpretation plugins | [product/reusable-interpretation-plugins.md](product/reusable-interpretation-plugins.md) |
| MCP stdio operations | [operations/mcp-stdio.md](operations/mcp-stdio.md) |
| Deployment/security boundary | [operations/deployment-boundary.md](operations/deployment-boundary.md), [security-model.md](security-model.md), [operations/mtls-deployment.md](operations/mtls-deployment.md) |
| Observability | [observability.md](observability.md) |
| Tests and quality gates | [testing.md](testing.md) |
| Getting started / usage guide | [usage-guide.md](usage-guide.md) |
| GraphBatch ingestion quickstart | [graph-batch-quickstart.md](graph-batch-quickstart.md) |
| Runtime guarantees | [runtime-guarantees.md](runtime-guarantees.md) |

## Active Roadmap

The current roadmap is:

1. Keep KMP as the API-first memory protocol: ingest, wake, ask, temporal
   moves, trace, and inspect.
2. Keep MCP as an adapter over KMP, not the owner of memory behavior.
3. Keep `kernel_ingest` / `KernelMemoryService.Ingest` as the canonical
   low-level write path.
4. Use `kernel_write_memory` as the writer-friendly MCP helper above canonical
   ingest, with strict relation quality and read-context proof.
5. Treat MemoryArena and MemoryAgentBench as primary agentic-memory benchmarks.
6. Keep LongMemEval as a secondary conversational-memory regression and reader
   stress test.
7. Move domain operators such as money, dates, counting, current/latest, and
   dedupe into plugins outside kernel core.
8. Add hybrid candidate retrieval and reranking behind ports, without turning
   KMP into a vector database API.
9. Scale the small kernel tool-operator model beyond the current V6 holdout:
   keep grouped anonymized splits, compare baselines, and validate raw
   predictions through live MCP/gRPC before any publication claim.
10. Record every serious Operator training attempt in
    [product/operator-training-runs/](product/operator-training-runs/README.md)
    with dataset provenance, evidence, stop gates, and final status.
11. Publish the operator model and trajectory dataset to Hugging Face only
    after the publication gate is clean, then update repo visibility around
    reproducible KMP evidence rather than broad claims.
12. Continue reducing infrastructure coupling through conformance tests and
    backend-independent semantics.

## Active Research And Benchmark Docs

Use these for current benchmark work:

| Benchmark / topic | Document |
| --- | --- |
| Benchmark positioning | [research/agentic-memory-benchmark-strategy-2026-05-06.md](research/agentic-memory-benchmark-strategy-2026-05-06.md) |
| MemoryArena | [research/memoryarena-benchmark.md](research/memoryarena-benchmark.md) |
| MemoryArena evaluator | [research/memoryarena-paper-aligned-evaluator.md](research/memoryarena-paper-aligned-evaluator.md) |
| MemoryAgentBench | [research/memoryagentbench-benchmark.md](research/memoryagentbench-benchmark.md) |
| LongMemEval | [research/longmemeval-benchmark.md](research/longmemeval-benchmark.md) |

## Historical Or Needs Review

These documents are useful for traceability but are not authoritative for the
current kernel contract:

| Area | Status |
| --- | --- |
| [archived/](archived/README.md) | Explicitly historical or superseded. |
| [migration/](migration/README.md) | Historical migration area. Use only the files listed by its README and review against current KMP before applying. |
| [research/ROADMAP_MASTER.md](research/ROADMAP_MASTER.md) | Legacy master roadmap. The active roadmap is now [product/kernel-roadmap-milestones.md](product/kernel-roadmap-milestones.md). |
| [research/PLAN_GRAPH_EXPLORER.md](research/PLAN_GRAPH_EXPLORER.md), [research/REQUIREMENTS_GRAPH_EXPLORER.md](research/REQUIREMENTS_GRAPH_EXPLORER.md) | Completed graph-explorer plan and requirements. Retained for traceability; the depth clamp described was since removed, so check current explorer contracts before reuse. |
| PIR/fix-planning migration reports | Historical integration evidence. They should not drive current kernel API decisions without revalidation. |
| Paper drafts under [paper/](paper/README.md) and [research/PAPER_*](research/README.md) | Publication artifacts. They may lag implementation and must be checked against `beta-status.md` before reuse. |

## Gaps Found In This Pass

Fixed or clarified:

- README claimed OTLP mTLS was still in progress. It now matches the current
  implementation: OTLP supports TLS/mTLS through env/Helm configuration.
- README overclaimed "TLS/mTLS on all infrastructure boundaries". It now states
  the real boundary: gRPC, Valkey, NATS, and OTLP can use mTLS; Neo4j client
  certificate auth remains partial.
- KMP API design still said only ingest aliases were implemented in MCP live
  mode. It now states that live MCP exposes all canonical KMP tools backed by
  `KernelMemoryService`.
- Observability docs said `rehydration.projection.lag` was not recorded. It is
  now documented as NATS projection consumer processing time, not full
  publish-to-queryable latency.
- Plugin docs now state explicitly that interpretation plugins are not
  automatically run by `kernel_ask`; readers/adapters must compose them.
- Writer protocol docs now reflect the implemented `kernel_write_memory` helper
  and leave the remaining P1 work visible.

Open documentation gaps:

- Add a short KMP "happy path" guide showing a human and an LLM using
  `kernel_write_memory`, `near`, `trace`, and `inspect` together.
- Add a conformance-oriented API page that says which behavior is protocol
  semantics and which behavior is adapter/backend-specific.
- Keep benchmark docs marked as "official", "local scorecard", "reader check",
  or "planned" so public claims do not overreach.
- Add an operations note for external GPU/RunPod benchmark execution once the
  first serious run is completed.

## Documentation Rules

- Do not call a plan "implemented" unless a code path, fixture, or test exists.
- Do not call a benchmark result "official" unless the dataset split, evaluator,
  and forbidden fields match the benchmark protocol.
- Keep MCP, gRPC, NATS, and future HTTP/SDKs described as bindings over KMP.
- Keep plugins above kernel core.
- Keep known limitations close to user-facing docs, not buried only in research
  notes.
