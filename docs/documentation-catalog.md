# Documentation Catalog

Date: 2026-08-25
Status: active documentation hygiene map

This catalog separates authoritative documentation from historical artifacts.
When a document conflicts with this catalog, prefer the active documents below
and fix or archive the conflicting note.

The exhaustive judgement is machine-readable in
[`documentation-inventory.tsv`](documentation-inventory.tsv): every Markdown
file is `current`, `research`, or `historical`, exactly once. The first class is
checked against the binary's public vocabulary. The other two remain
navigable evidence but do not define what a current KMP install promises.

## Authoritative Current Docs

These documents are the current source of truth for users and maintainers:

| Area | Document |
| --- | --- |
| Entry point | [README.md](../README.md) |
| Navigation | [index.md](index.md) |
| v1beta1 maturity and limitations | [beta-status.md](beta-status.md) |
| Product roadmap | [product/kernel-roadmap-milestones.md](product/kernel-roadmap-milestones.md) |
| Embedded edition roadmap | [product/kmp-embedded-edition-roadmap.md](product/kmp-embedded-edition-roadmap.md) |
| Embedded host recipes + playbook | [operations/embedded-hosts.md](operations/embedded-hosts.md) |
| Embedded releases + format compatibility | [operations/embedded-release.md](operations/embedded-release.md) |
| Release process (tags, crates.io, artefacts) | [release.md](release.md) |
| KMP product/API design | [product/kernel-context-api-design.md](product/kernel-context-api-design.md) |
| Typed KMS/gRPC status | [product/kernel-memory-service-grpc-plan.md](product/kernel-memory-service-grpc-plan.md) |
| Writer helper protocol | [product/kernel-write-protocol-plan.md](product/kernel-write-protocol-plan.md) |
| Kernel tool-operator model | [product/kernel-tool-operator-model-plan.md](product/kernel-tool-operator-model-plan.md) |
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
3. Keep `kmp_ingest` / `KernelMemoryService.Ingest` as the canonical
   low-level write path.
4. Use `kmp_write_memory` as the writer-friendly MCP helper above canonical
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

## Research Status

[Research Summary](research/README.md) records what the research phase
completed and the active direction toward state-of-the-art agentic context,
including temporal and multidimensional memory analysis. Detailed benchmarks,
paper drafts, demos, and incident reports are preserved in the
[research archive](../archive/docs/research/README.md).

## Historical Or Needs Review

These documents are useful for traceability but are not authoritative for the
current kernel contract:

| Area | Status |
| --- | --- |
| [archive/docs/](../archive/docs/README.md) | Explicitly historical or superseded documentation. |
| [archive/docs/migration/](../archive/docs/migration/README.md) | Migration-era integration evidence. Review against current KMP before applying. |
| [archive/docs/integrations/](../archive/docs/integrations/made-kmp.md) | Superseded product-integration guidance. |
| [archive/docs/research/](../archive/docs/research/README.md) | Benchmark notebooks, paper drafts, demos, incidents, and legacy roadmaps. |
| [archive/docs/showcase/](../archive/docs/showcase/README.md) | Former public-pitch recordings and their reproducible sources. |
| PIR/fix-planning migration reports | Historical integration evidence. They should not drive current kernel API decisions without revalidation. |
| Paper sources under [paper/](paper/README.md) and drafts in the [research archive](../archive/docs/research/README.md) | Publication artifacts. They may lag implementation and must be checked against `beta-status.md` before reuse. |

## Phase 03 authority decision

The inventory is now the decision, rather than an open question:

- dated Operator audits, training runs, model plans and Hugging Face templates
  are **research evidence** for a separate benchmark project; they are not a
  live KMP runtime contract;
- `archive/docs/` and all of its subdirectories are **historical evidence**,
  even where a file remains useful to an integrating product;
- papers, completed research roadmaps, incident reports and ADR spikes are
  **research evidence** and may describe the vocabulary that existed when an
  experiment ran;
- guides, operations, accepted ADRs and the core product/protocol documents
  listed above are **current** and must not use a former public tool name.

The archive is deliberately outside the current documentation spine. Old names
inside an experiment are evidence about that experiment; putting them in the
current contract is the contradiction. `scripts/ci/documentation-spine.sh`
ensures every current document stays classified and reachable from
`docs/index.md` in at most two links.

## Gaps Found In Earlier Passes

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
  automatically run by `kmp_ask`; readers/adapters must compose them.
- Writer protocol docs now reflect the implemented `kmp_write_memory` helper
  and leave the remaining P1 work visible.

Open documentation gaps:

- Keep benchmark docs marked as "official", "local scorecard", "reader check",
  or "planned" so public claims do not overreach.
- Add an operations note for external GPU/RunPod benchmark execution once the
  first serious run is completed.

The former happy-path and conformance gaps are closed by the README first
memory route, the live tool descriptions, the transport-neutral
[`product/recall-projection-contract.md`](product/recall-projection-contract.md)
and the four-path parity matrix in [`testing.md`](testing.md).

## Documentation Rules

- Do not call a plan "implemented" unless a code path, fixture, or test exists.
- Do not call a benchmark result "official" unless the dataset split, evaluator,
  and forbidden fields match the benchmark protocol.
- Keep MCP, gRPC, NATS, and future HTTP/SDKs described as bindings over KMP.
- Keep plugins above kernel core.
- Keep known limitations close to user-facing docs, not buried only in research
  notes.
