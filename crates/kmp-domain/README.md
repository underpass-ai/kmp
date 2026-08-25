# kmp-domain

[KMP](https://github.com/underpass-ai/kmp) is local-first agent memory that
preserves what happened, when and why. This crate is its domain model:
aggregates, value objects, repository traits and projection types. No IO, no
transport, no storage engine.

Memory here is what the protocol says it is: an **about** scope, the
**dimensions** inside it, typed **relations** carrying semantic class,
rationale, evidence and provenance, and a temporal reading of all of it. Those
are types, not conventions — an about id or a semantic class is checked at
construction rather than trusted afterwards.

## What lives here

- **Model** — bundles, nodes, node detail, relationships, stats and temporal
  traversal: what a caller gets back when it wakes, asks or moves through
  memory.
- **Projections** — the materialization events (`graph.node.materialized`,
  `graph.relation.materialized`, `node.detail.materialized`) and the
  checkpoint/envelope machinery that applies them exactly once.
- **Repositories** — the persistence traits the kernel reads and writes
  through: graph neighbourhood, node detail, the append-only context event
  store, snapshots, the about index.
- **Plugins** — a re-export of [`kmp-plugin-api`](https://crates.io/crates/kmp-plugin-api)
  for runtime crates. Plugin authors should depend on that crate directly.

Errors are typed as `DomainError`; IO errors do not appear, because IO does
not.

## Stability

Published so the rest of the kernel can be published, not as a curated public
API. It moves with the kernel's releases. A consumer that wants a surface
versioned by meaning should use
[`kmp-memory-api`](https://crates.io/crates/kmp-memory-api).

## License

Apache-2.0.
