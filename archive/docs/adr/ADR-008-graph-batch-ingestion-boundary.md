# ADR-008: GraphBatch as the experimental ingestion boundary

**Status:** Accepted
**Date:** 2026-04-08
**Context:** vLLM-backed graph extraction, translator hardening, live smoke, and incremental E2E

## Decision

Adopt `GraphBatch` as the **experimental write boundary** for graph
materialization.

This boundary is:

- aggregate-scoped around one `root_node_id`
- node-centric
- validated before translation
- translated into the existing async projection subjects

`UpdateContext` remains a separate command path. It is **not** the primary
ingestion API for model-generated graph materialization.

## Why

The repository now has evidence for the `GraphBatch` path:

- strict schema-constrained request to live `vLLM`
- translator from batch to projection events
- deterministic minimal E2E
- deterministic medium incremental E2E over the same root aggregate

That is enough to freeze a design direction even though the transport API is
still experimental.

## Boundary

### What `GraphBatch` means

`GraphBatch` is a bounded aggregate update around a single `root_node_id`.

It contains:

- `nodes`
- `relations`
- `node_details`

Each batch is local, not global:

- all non-root nodes must be reachable from the root through outward relations
- non-structural relations require support and confidence
- disconnected nodes and speculative duplicates are invalid

### What the kernel continues to own

The stable kernel-owned write boundary remains:

- `graph.node.materialized`
- `graph.relation.materialized`
- `node.detail.materialized`

Those async subjects are still the durable projection contract.

`GraphBatch` is the **ingress convenience boundary** that translates into those
events. It does not replace the underlying async contract.

## Why Not `UpdateContext`

`UpdateContext` is a generic command with:

- `ContextChange`
- `payload_json`
- optimistic preconditions
- event-store semantics

That shape is acceptable for generic command ingestion, but it is the wrong
primary boundary for model-generated graph extraction because:

- it is too weakly typed for graph materialization
- it does not encode graph-local invariants directly
- it pushes too much semantic burden into opaque JSON payloads
- it is not what the paper and the current E2E evidence actually validate

## Public Naming

The public term is `GraphBatch`, not `LlmGraphBatch`.

The codebase keeps `LlmGraphBatch` as a compatibility helper name today, but
the testkit now exports domain aliases:

- `GraphBatch`
- `GraphBatchNode`
- `GraphBatchRelation`
- `GraphBatchNodeDetail`

This keeps the public language domain-oriented while avoiding a breaking rename
across current test helpers.

## Consequences

- **Positive:** the write boundary matches the proven projection path.
- **Positive:** the API is small, local, and easy to validate.
- **Positive:** model concerns stay focused on graph semantics, not transport.
- **Trade-off:** the public transport shape still needs to be specified
  explicitly before it can be declared stable.
- **Trade-off:** command metadata and projection/event metadata remain separate
  concerns and must be mapped cleanly by the ingress layer.

## Next Step

Define one experimental API shape that wraps `GraphBatch` and
`CommandMetadata`, but do not freeze it as kernel-owned `v1beta1` until real
consumers validate it.
