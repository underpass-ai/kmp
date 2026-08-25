# PIR Kernel Relation-Materialized RFC

Status: accepted; initial kernel implementation and container coverage landed
Date: 2026-04-14  
Scope: boundary evolution needed to complete the `PIR` sequential intervention spine without re-materializing anchor nodes

## Intent

Define the smallest kernel-facing boundary change that allows `PIR` to emit the
full semantic spine:

- `incident -HAS_FINDING-> finding`
- `finding -SUPPORTED_BY-> evidence`
- `decision -ADDRESSES-> finding`
- `decision -BASED_ON-> evidence`
- `decision -IMPLEMENTED_BY-> task`
- `task -VERIFIED_BY-> evidence`

without forcing later waves to re-emit earlier source nodes purely to satisfy
the current node-centric async shape.

This RFC is intentionally narrower than a full `GraphBatch` redesign. It
focuses on one missing capability:

- materializing a relation whose source node already exists from an earlier wave

## Problem Statement

The previous kernel boundary was node-centric in a very specific way:

- it exposed only:
  - `graph.node.materialized`
  - `node.detail.materialized`
- `graph.node.materialized` carries one source node plus its outgoing
  `related_nodes`
- the current `GraphBatch -> translator` path still serializes every
  relation as "outgoing from a node materialized in this same wave"

This is visible in:

- [api/asyncapi/context-projection.v1beta1.yaml](../../../api/asyncapi/context-projection.v1beta1.yaml)
- [llm_graph.rs](../../../crates/kmp-testkit/src/llm_graph.rs)
- [projection_application_service.rs](../../../crates/kmp-application/src/projection/projection_application_service.rs)

That shape works well for:

- root-attached graphs
- single-wave local subgraphs
- bounded model-produced graph extraction

It is weaker for the incremental `PIR` intervention story because later waves
need to add canonical edges whose source node was emitted earlier:

- rehydration wants `finding -> SUPPORTED_BY -> evidence`
- patch application wants `decision -> IMPLEMENTED_BY -> task`
- verification wants `task -> VERIFIED_BY -> evidence`

Under the current boundary, that forces one of two things:

1. re-materialize earlier nodes as anchors in later waves
2. keep root-attached compatibility edges as the primary semantic shape

The first is a migration patch. The second preserves the star topology longer
than desired.

## Why the Current Boundary Blocks the Full Spine

The difficulty is not Neo4j itself. The internal projection model already
supports relation upserts independently:

- [projection_mutation.rs](../../../crates/kmp-domain/src/projection/projection_mutation.rs)
- [node_relation_projection.rs](../../../crates/kmp-domain/src/projection/node_relation_projection.rs)
- [upsert_relation_projection_query.rs](../../../crates/kmp-adapter-neo4j/src/adapter/queries/upsert_relation_projection_query.rs)

The actual constraint is the public async boundary and its translator:

1. `GraphBatch` validation requires `source_node_id` to exist in `nodes[]`
2. `GraphBatch` reachability is checked via outward relations from the root
3. the translator emits relations only as `related_nodes` on the source node
4. the NATS consumer only recognizes `graph.node.materialized` and
   `node.detail.materialized`

So the full spine is blocked at the ingestion boundary, not in the graph store.

## Design Goal

Add one new experimental async subject that allows relation materialization
without coupling that operation to a node re-materialization event.

The design goal is:

- keep the stable node-centric boundary intact
- add the smallest missing primitive
- let deterministic producers such as `PIR` use that primitive
- avoid forcing a full `GraphBatch` redesign in the same step

## Non-Goals

This RFC does **not** attempt to:

- replace `graph.node.materialized`
- make `GraphBatch` obsolete
- introduce a new command RPC
- freeze a new `v1` contract immediately
- prove that all producers should switch to relation-only events

This is a focused extension for cases where the semantic source of a new edge
already exists from an earlier wave.

## Proposal

Introduce one new async subject:

- `graph.relation.materialized`

This subject is initially **experimental**, like the documented `GraphBatch`
ingress direction.

### Proposed message shape

```yaml
event_id: evt-...
correlation_id: incident-run-...
causation_id: wave-...
occurred_at: 2026-04-14T18:00:00Z
aggregate_id: relation:decision:123:IMPLEMENTED_BY:task:123
aggregate_type: node_relation
schema_version: v1beta1
data:
  source_node_id: decision:123
  target_node_id: task:123
  relation_type: IMPLEMENTED_BY
  explanation:
    semantic_class: procedural
    rationale: "the patch application task executes the accepted fix-planning decision"
    method: "rollout through production deployment pipeline"
    decision_id: decision:123
    confidence: high
    sequence: 4
```

### Proposed data model

`GraphRelationMaterializedEvent`

- envelope: same shared `EventEnvelope`
- data:
  - `source_node_id`
  - `target_node_id`
  - `relation_type`
  - `explanation`

The `explanation` object should reuse the same shape already used inside
`RelatedNodeReference.explanation`.

## Why this is the smallest clean change

This proposal reuses what the kernel already knows how to do internally:

- `ProjectionMutation::UpsertNodeRelation`
- Neo4j upsert semantics for source, target, and edge

It avoids:

- re-materializing earlier nodes only to attach outgoing relations
- stretching `GraphBatch` semantics before we decide whether that path should
  stay purely model-oriented
- inventing product-specific PIR nouns in the kernel contract

## Ordering and Convergence

This is the most important operational concern.

If a producer publishes:

1. node event
2. relation event

the boundary should obviously converge.

But the kernel also needs an honest answer for this case:

1. relation event arrives first
2. source and/or target node event arrives later

The current Neo4j writer already tolerates this:

- [upsert_relation_projection_query.rs](../../../crates/kmp-adapter-neo4j/src/adapter/queries/upsert_relation_projection_query.rs)

It creates placeholder nodes if either side is missing, then later node upserts
fill in the real node data.

That means the first experimental version can adopt this explicit policy:

- **supported and convergence-safe**
- **not the preferred producer ordering**

Recommended producer discipline:

1. publish node materialization first when possible
2. publish relation materialization after the relevant nodes
3. rely on placeholder convergence only as a safety property, not as the
   intended steady-state strategy

## Placeholder Policy

The current Neo4j relation upsert path already creates implicit placeholder
nodes when a source or target node does not yet exist.

That behavior is useful for convergence, but the current placeholder shape is
too weak for an honest public boundary because it can surface as effectively
empty graph content:

- `node_kind = "unknown"`
- `title = ""`
- `summary = ""`
- `status = "STATUS_UNSPECIFIED"`

If that reaches the render path before the real node is materialized, it risks
polluting the rehydrated bundle with non-semantic noise.

### Proposed placeholder shape

The additive relation boundary should therefore standardize a **minimal explicit
placeholder**, not a rich provisional node.

Recommended placeholder state:

```yaml
node_kind: placeholder
title: "[unmaterialized node]"
summary: "Referenced by relation before node materialization"
status: UNMATERIALIZED
labels:
  - placeholder
properties:
  placeholder: "true"
  placeholder_reason: "relation_materialized_before_node"
```

### Placeholder rules

1. A placeholder is a convergence aid, not a graph-domain concept.
2. A later `graph.node.materialized` event with the same `node_id` fully
   overwrites placeholder fields with the real node projection.
3. Placeholders should be filtered from normal rendered bundles while they
   remain unmaterialized.
4. Relation traversal may still use them internally for convergence and
   path-completion.
5. Diagnostic or projection-debug views may choose to expose placeholders
   explicitly.

### Why filter them from render

The kernel render path today would happily serialize an empty or near-empty
placeholder node into LLM context.

That is the wrong operational default.

For `PIR`, the placeholder should preserve graph integrity while waiting for the
real node event, but it should not count as usable incident evidence.

### Why not enrich them further

The placeholder should remain minimal on purpose.

It should not attempt to guess:

- the real node kind
- a human title
- lifecycle state
- domain labels

Those belong to the eventual real node event. The placeholder should only say:

- this node id is referenced
- the relation is preserved
- full node materialization has not happened yet

## Contract Position

Current maturity stance:

- `graph.node.materialized`: stable `v1beta1`
- `node.detail.materialized`: stable `v1beta1`
- `graph.relation.materialized`: implemented experimental additive subject

Why:

- it expands the async surface
- it changes the practical integration story for incremental producers
- it has initial contract/runtime/container evidence, but still needs broader
  convergence and idempotency coverage before being declared stable

This mirrors the current stance for `GraphBatch`:

- recommended for some producers
- but not yet frozen as the stable public transport contract

## Compatibility

This proposal is additive.

It does not remove or rename:

- existing subjects
- existing proto services
- existing event fields

Existing producers can continue using:

- root-attached node materialization only
- `GraphBatch -> translator -> graph.node.materialized + node.detail.materialized`

`PIR` can adopt the new subject selectively for the later-wave edges that do
not fit naturally into the current node-centric event shape.

## Recommended PIR Usage

For `PIR`, the intended first use is narrow:

### Keep as node materialization

- `incident`
- `finding`
- `decision`
- `task`
- `evidence`
- `node_detail`

### Move to relation materialization when source belongs to an earlier wave

- `finding -> SUPPORTED_BY -> evidence`
- `decision -> IMPLEMENTED_BY -> task`
- `task -> VERIFIED_BY -> evidence`

Potentially also:

- `decision -> ADDRESSES -> finding`
- `decision -> BASED_ON -> evidence`

if those edges are emitted in a later wave than the source decision node.

## Why not redesign GraphBatch first

That is a reasonable long-term option, but it is a larger move.

Changing `GraphBatch` first would require deciding:

- whether relations may reference nodes outside `nodes[]`
- how to express external node references
- how reachability should work when a batch intentionally points to nodes from
  earlier waves
- whether the shape stays equally suitable for model output

Those are important questions, but they are broader than the immediate PIR
need.

This RFC therefore recommends:

1. add one relation-materialization primitive first
2. validate it with deterministic and live tests
3. only then decide whether `GraphBatch` should evolve to target that richer
   async boundary

## Implementation Sketch

### Domain / application

- add `GraphRelationMaterializedData`
- add `GraphRelationMaterializedEvent`
- extend `ProjectionEvent`
- extend the projection application service to map that event directly to
  `ProjectionMutation::UpsertNodeRelation`
- standardize placeholder node values when relation upsert creates a missing
  endpoint
- filter unmaterialized placeholders from default bundle rendering

### Async contract

- extend AsyncAPI with `graph.relation.materialized`
- add example payload fixture
- extend contract tests

### NATS adapter

- extend subject routing
- extend payload decoding
- extend stream subjects
- add relation consumer configuration

### Testkit

- add a deterministic publisher helper for relation events
- do **not** force `GraphBatch` to target this path in the first cut

## Proposed Test Plan

The implementation should be considered valid only if the following tests are
added and pass.

### 1. Contract / unit tests

#### `asyncapi_contract_tests` extension

Assert that:

- `graph.relation.materialized` exists
- its schema reuses the same explanation shape
- existing subjects remain unchanged

Status: implemented.

#### `nats_consumer_routes_graph_relation_materialized`

Assert that:

- the NATS consumer recognizes the new subject
- unsupported subjects still fail deterministically

Status: implemented.

#### `projection_application_service_handles_relation_event`

Assert that:

- the event maps to one `UpsertNodeRelation` mutation
- no node detail mutation is emitted

Status: implemented.

### 2. Container integration tests

These should live alongside the current kernel container tests and use the same
Neo4j + Valkey + NATS + gRPC harness style.

Suggested location:

- `crates/kmp-tests-kernel/tests/`

#### `relation_materialization_integration`

Goal:

- prove that a relation can be added between nodes across waves without
  re-materializing the source node

Shape:

1. publish `incident` and `finding`
2. publish `decision`
3. publish `decision -ADDRESSES-> finding` through the new subject
4. read back via `GetContext`

Assertions:

- all three nodes exist
- `ADDRESSES` exists
- no duplicate root node is needed

Status: implemented as
`relation_materialized_events_extend_a_pir_like_spine_without_rematerializing_the_source_node`.

#### `relation_materialization_out_of_order_integration`

Goal:

- prove eventual convergence if the relation arrives before one or both nodes

Shape:

1. publish `finding -SUPPORTED_BY-> evidence` first
2. publish `finding`
3. publish `evidence`
4. read back via `GetContext`

Assertions:

- final graph contains one relation only
- source and target no longer remain placeholder-only after node materialization
- the read path returns the expected node titles/summaries
- placeholders do not leak into rendered context once the real nodes exist

#### `pir_sequential_spine_relation_integration`

Goal:

- mirror the intended `PIR` intervention spine as closely as possible

Shape:

1. wave 1:
   - `incident`
   - `finding`
   - `incident -HAS_FINDING-> finding`
2. wave 2:
   - `rehydration evidence`
   - `finding -SUPPORTED_BY-> evidence`
3. wave 3:
   - `decision`
   - `decision -ADDRESSES-> finding`
   - `decision -BASED_ON-> evidence`
4. wave 4:
   - `task`
   - `decision -IMPLEMENTED_BY-> task`
5. wave 5:
   - `verification evidence`
   - `task -VERIFIED_BY-> verification evidence`

Assertions:

- the full spine is queryable from the root
- `GetContextPath` from root to verification traverses the intended narrative
- rendered content preserves the intervention chain under `reason_preserving`

#### `relation_materialization_idempotency_integration`

Goal:

- prove that redelivery or republish of the same relation event does not create
  duplicate edges

Shape:

1. publish nodes
2. publish one relation event
3. republish the same relation event with the same `event_id`

Assertions:

- graph contains one edge only
- projection dedup reports duplicate handling as expected

#### `placeholder_filtering_integration`

Goal:

- prove that placeholder-only nodes do not pollute default rehydrated context

Shape:

1. publish one relation event before either endpoint exists
2. query the root neighborhood or path while one side is still placeholder-only

Assertions:

- the relation may exist in storage
- placeholder nodes are not rendered as normal semantic nodes in default
  `GetContext`
- once the real node materializes, it becomes visible normally

### 3. Live / in-cluster smoke tests

These should look like the current kernel smokes: minimal, contract-oriented,
and runnable against a deployed kernel.

Suggested locations:

- cluster smoke wrappers:
  `e2e/kernel-runner/tests/`
- live testkit-backed roundtrip tests:
  `crates/kmp-testkit/tests/`
- optional helper binaries:
  `crates/kmp-testkit/src/bin/`

#### `graph_relation_roundtrip_smoke`

Purpose:

- prove the new subject works against a live kernel/NATS deployment

Shape:

1. publish a minimal incident + finding fixture
2. publish one evidence node
3. publish one relation-only event
4. poll `GetContext`

Success criteria:

- live kernel returns the new relation in the neighborhood

#### `pir_sequential_spine_roundtrip_smoke`

Purpose:

- prove the boundary is good enough for the exact PIR sequential shape we want

Shape:

- same five-wave sequence as the container integration above
- internal cluster NATS + mTLS gRPC, same style as existing runner jobs

Success criteria:

- `GetContext` sees the full spine
- `GetContextPath` from incident to verification path is legible
- no anchor-node re-materialization is required

## Acceptance Criteria

This RFC is partially validated. Current state:

1. the new async subject exists and passes contract tests;
2. deterministic container tests prove cross-wave relation materialization;
3. out-of-order convergence is not yet tested;
4. live cluster smoke is not yet added;
5. the full PIR-like sequential spine is not yet demonstrated end to end;
6. placeholder behavior is implemented for relation-before-node convergence,
   but still needs dedicated rendered-output coverage.

The subject should remain experimental until:

1. out-of-order convergence is explicit and tested;
2. a live smoke proves the path against a deployed kernel;
3. the PIR-like sequential spine is demonstrated without anchor-node patching;
4. placeholder behavior is explicit, minimal, and does not pollute normal
   rehydrated output

## Decision Recommendation

`graph.relation.materialized` is accepted as the clean additive boundary for
incremental relation-only projection. Current recommendation:

1. keep it experimental while the remaining tests land;
2. validate out-of-order, idempotency, placeholder filtering, and live smoke;
3. only after that decide whether `GraphBatch` should evolve to target it
   directly.

## Suggested Implementation Phases

To keep blast radius low, this work is split into three phases.

### Phase 1 — contract and projection

Status: implemented.

- AsyncAPI extension
- domain/application event support
- NATS subject routing and decoding
- Neo4j projection path for relation events

Current evidence:

- relation events materialize edges through the real projection runtime.

### Phase 2 — placeholder hygiene

Status: partial; dedicated coverage still pending.

- explicit placeholder shape
- overwrite semantics confirmed
- default render filtering for unmaterialized placeholders

Success condition:

- convergence safety does not degrade LLM-facing context quality

### Phase 3 — PIR-shaped validation

Status: partial; initial PIR-like cross-wave relation test exists.

- container E2E reproducing the full sequential spine remains pending
- live roundtrip smoke against deployed kernel remains pending

Success condition:

- the exact PIR spine can be represented without anchor-node patching
