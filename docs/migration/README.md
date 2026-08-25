# Migration

> Historical migration area. After the `v1beta1` cut and compatibility removal,
> several files in this directory are obsolete or require detailed review
> before reuse. Do not treat this folder as the primary source of truth for the
> current kernel contract.

Migration plans, parity reports, and shadow-mode notes will live here.

Legacy compatibility documents archived in [docs/archived/](../archived/).

Use this directory only for migration-specific references that still matter to
an integrating product:

- [Kernel node-centric integration contract](kernel-node-centric-integration-contract.md)
- [Kernel agentic integration E2E](kernel-agentic-integration-e2e.md)
- [Kernel agentic event-trigger E2E](kernel-agentic-event-trigger-e2e.md)
- [Kernel runtime integration reference](kernel-runtime-integration-reference.md)
- [PIR kernel integration reference](pir-kernel-integration-reference.md)
- [PIR real-kernel integration plan](pir-kernel-real-integration-plan.md)
- [PIR live context-consumption evidence](pir-kernel-live-context-consumption-evidence.md)
- [PIR live-smoke failure report](pir-fix-planning-live-smoke-failure-report-2026-04-12.md)
- [PIR long-budget retry plan](pir-fix-planning-long-budget-retry-plan.md)
- [PIR model research notes](pir-fix-planning-model-research-notes.md)
- [PIR experiment matrix](pir-fix-planning-experiment-matrix.md)
- [PIR A0 repeatability report](pir-fix-planning-a0-repeatability-report-2026-04-12.md)
- [PIR A0 scenario matrix](pir-fix-planning-a0-scenario-matrix-report-2026-04-12.md)
- [PIR D1 live-failure analysis](pir-fix-planning-d1-live-failure-analysis-2026-04-12.md)
- [PIR next-session handoff](pir-fix-planning-next-session-handoff-2026-04-12.md)
- [Qwen 3.5 configuration reference](qwen35-configuration-reference.md)
- [PIR graph-inspection reranker smoke](pir-kernel-graph-inspection-smoke-reranker.md)
- [PIR graph-inspection late-waves smoke](pir-kernel-graph-inspection-smoke-late-waves.md)
- [PIR first event-driven agent plan](pir-first-event-driven-agent-plan.md)
- [PIR sequential graph-shape proposal](pir-kernel-sequential-graph-shape-proposal.md)
- [PIR materialized-relation RFC](pir-kernel-relation-materialized-rfc.md)
- [PIR blind structural evidence](pir-kernel-blind-structural-evidence.md)
- [PIR blind context-consumption evidence](pir-kernel-blind-context-consumption-evidence.md)
- [LLM response determinism strategy](llm-response-determinism-strategy.md)
- [KMP v1beta1 ask/evidence normalization](kmp-v1beta1-ask-evidence-normalization.md)

Historical closeout and compatibility planning material belongs in
[`docs/archived/`](../archived/README.md).

For the current documentation authority map, see
[`docs/documentation-catalog.md`](../documentation-catalog.md). If a migration
note conflicts with `beta-status.md`, the product roadmap, or the KMP API docs,
the migration note is stale until revalidated.

Phase 0 status:

- complete
- kernel contract freeze, contract CI, and reference fixtures: complete
- generic agentic integration proof: complete
- event-driven agentic trigger proof: complete
- runtime integration reference for external consumers: complete
- runnable runtime reference client outside tests: complete
- LLM response determinism strategy: planned and documented
- transport security v1: implemented for gRPC, outbound NATS, outbound Valkey, and Neo4j CA wiring; Neo4j client identity remains open
- repo closeout and handoff to integrating products: archived as historical documentation
- shadow mode specification for `swe-ai-fleet`: archived as historical documentation
- deferred kernel maintenance milestone: consolidate the integration harness
  and reduce CI runtime before the next major growth phase
- next milestone outside the kernel: implement the `swe-ai-fleet` adapter using the node-centric integration strategy and checklist

Historical internal substrate plans archived in [docs/archived/](../archived/).
