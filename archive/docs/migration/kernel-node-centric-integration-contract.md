# Kernel Node-Centric Integration Contract

Status: v1beta1 (see [archived beta-status.md](../beta-status.md) for the maturity assessment recorded at the time)

## Purpose

Define the public contract that any system may depend on to integrate with
the rehydration kernel.

The kernel originated as a context engine for `swe-ai-fleet` and is designed
to be adopted by that product as its primary context provider. However, the
contract is generic — no product-specific nouns are part of the kernel surface.

## Contract Boundary

### 1. gRPC contract

Package: `underpass.rehydration.kernel.v1beta1`

Services:

- `ContextQueryService` — GetContext, GetContextPath, GetNodeDetail, RehydrateSession, ValidateScope
- `ContextCommandService` — UpdateContext
- `KernelMemoryService` — Ingest, Wake, Ask, Goto, Near, Rewind, Forward, Trace, Inspect

Primary identifiers:

- `root_node_id` — bundle anchor (any node in the graph)
- `node_id`, `node_kind` — entity identity and type
- `relationship_type` — edge label
- `semantic_class` — causal, motivational, evidential, constraint, procedural, structural
- `BundleNodeDetail` — extended per-node content
- `RehydrationBundle` — validated context container

Memory API identifiers:

- `about` — memory anchor/root node for one memory graph
- `MemoryDimension` — dimension scope, for example conversation, entity, or benchmark record
- `MemoryEntry` — typed memory item with one or more temporal coordinates
- `TemporalCursor` — exactly one of `ref`, `time`, or `sequence`
- `DimensionSelection` — all, only, or except over memory coordinate dimensions

`KernelMemoryService` is additive and API-first. It composes the lower-level
query/command behavior inside the kernel application layer; consumers should not
need to call `ContextQueryService` or `ContextCommandService` directly for KMP
memory moves once they adopt this service.

### 2. Async contract (NATS JetStream)

Inbound (kernel consumes):

- `{prefix}.graph.node.materialized` — nodes + relationships
- `{prefix}.graph.relation.materialized` — relation-only async projection
  between existing or future graph nodes
- `{prefix}.node.detail.materialized` — extended detail

Outbound (kernel publishes):

- `{prefix}.context.bundle.generated` — **contract-only; not yet emitted by runtime**

Defined in [`api/asyncapi/context-projection.v1beta1.yaml`](../../../api/asyncapi/context-projection.v1beta1.yaml).

### 3. Experimental GraphBatch ingress

The repo also documents an experimental `GraphBatch` ingestion shape for
model-driven producers.

Important:

- it is **recommended guidance**, not a stable `v1beta1` transport contract
- it translates into the async subjects above
- it does not replace the owned async projection boundary

See:

- [`graph-batch-quickstart.md`](../graph-batch-quickstart.md)
- [`graph-batch-ingestion-api.md`](../graph-batch-ingestion-api.md)

## What Consumers May Rely On

- `root_node_id` is the primary bundle anchor
- Graph reads are expressed in terms of nodes and relationships
- Extended node context is carried as generic node detail
- Render budget, focus, tiers, and mode are generic request concerns
- Snapshot persistence is generic
- Async projection inputs remain node-centric
- Kernel Memory Protocol moves are available as typed gRPC through
  `KernelMemoryService`

## What the Kernel Does NOT Own

The kernel does not contain product-specific nouns. If an integration requires
the kernel to understand domain-specific entity types, that mapping belongs in
an anti-corruption layer outside the kernel.

## Stability Rules

For `v1beta1` gRPC and async subjects:

- Do not rename services, methods, or field names without a version bump
- Do not replace `root_node_id` with product-specific identifiers
- Do not add product nouns to message or field names
- Do not couple async subjects to one product's event vocabulary
- Prefer additive changes over semantic rewrites

Breaking: renaming `root_node_id`, changing service names, changing semantic
meaning of fields, renaming async subjects.

Non-breaking: adding optional request/response fields, adding new services
or methods, adding new async subjects.

## Evidence

This contract is backed by:

- Proto: [`api/proto/underpass/rehydration/kernel/v1beta1`](../../../api/proto/underpass/rehydration/kernel/v1beta1)
- AsyncAPI: [`api/asyncapi/context-projection.v1beta1.yaml`](../../../api/asyncapi/context-projection.v1beta1.yaml)
- Fixtures: [`api/examples/kernel/v1beta1`](../../../api/examples/kernel/v1beta1)
- Contract tests: `kernel_v1beta1_contract_tests.rs`, `reference_fixture_contract_tests.rs`, `asyncapi_contract_tests.rs`
