# ADR-010: Materialized adjacency in the storage engine, not an in-memory graph

**Status:** Accepted
**Date:** 2026-07-21
**Context:** [KMP Embedded Edition Roadmap](../product/kmp-embedded-edition-roadmap.md), milestone E0

## Decision

The embedded edition represents the graph as **materialized adjacency tables
in the storage engine** ([redb](ADR-009-embedded-storage-engine.md)), updated
synchronously at ingest by the projection runtime.

There is **no in-memory graph** (petgraph-style) rebuilt from the event log
at startup, and no startup replay on the read path.

## Shape

The graph port surface is neighborhood- and projection-shaped, not general
Cypher:

- `GraphNeighborhoodReader::load_neighborhood(root_node_id, depth)`
- `GraphNeighborhoodReader::load_context_path(root, target, subtree_depth)`
- `NodeRelationshipReader::load_node_relationships(node_id)` (incoming +
  outgoing)
- `MemoryAboutIndexReader::list_memory_abouts[_by_dimensions]`
- `NodeDetailReader` point lookups

Every one of these is answerable from point reads and prefix range scans:

- **node details:** `node_id -> detail` table;
- **adjacency:** `(node_id, ordinal) -> relation` tables, one keyed by
  source and one by target, so incoming and outgoing edges are both a prefix
  range over a typed tuple key — no full-table filtering;
- **about/dimension index:** dedicated index tables maintained by the same
  projection write;
- **neighborhood at depth N:** iterative breadth-first expansion doing one
  prefix scan per frontier node, bounded by the `depth`/budget arguments the
  ports already take.

## Why

- **Reopen time is the product constraint.** The MCP stdio binary starts per
  agent session ([ADR-009](ADR-009-embedded-storage-engine.md)); rebuilding
  an in-memory graph from a 100k-event log at every session start would turn
  a 3ms open into seconds, and its memory footprint would grow unbounded with
  corpus size. Materialized adjacency keeps session start O(1).
- **The spike sized the traversal primitive.** Adjacency prefix scans on redb
  ran at ~213k scans/s on a 20k-node / 204k-edge corpus — a depth-bounded
  neighborhood expansion touching hundreds of nodes costs single-digit
  milliseconds without any in-memory index.
- **Same semantics as the infrastructure edition.** Neo4j/Valkey adapters
  serve projections materialized by the event-driven runtime; the embedded
  edition materializes the same projections synchronously. The conformance
  suite (E1) can then assert identical read results, which an independently
  designed in-memory representation would make harder to guarantee.
- **Crash recovery stays one story.** Projections are replay-safe and
  rebuildable from the append-only event log (E2's compaction/replay tool);
  an in-memory graph would add a second, divergent recovery path.

## Why not an in-memory graph

Rejected as the primary representation, not as a future optimization:

- startup rebuild cost scales with the event log, violating the session-start
  budget;
- memory ceiling forces size limits exactly where the roadmap wants
  "store survives 100k-event corpus" guarantees;
- it duplicates state that must then be kept coherent with the durable store.

If profiling in E6 benchmarks ever shows traversal hot spots, an in-memory
**cache** of hot neighborhoods may be layered behind the same ports — as an
optimization with the tables as source of truth, never as the representation.

## Consequences

- **Positive:** O(1) session start regardless of corpus size; one recovery
  story; read results directly comparable across editions in the conformance
  suite.
- **Positive:** traversal cost is explicit (scans per frontier node), so the
  existing depth/budget port arguments bound work naturally.
- **Trade-off:** double-write of adjacency (by-source and by-target rows) on
  ingest. Accepted: ingest is fsync-bound, not row-bound, and the spike's
  write rates already include two edge rows per event.
- **Trade-off:** deep or high-fanout traversals do many small range scans
  instead of one in-memory walk. Accepted at measured rates; revisit only
  with E6 benchmark evidence.

## Next Step

E1 encodes neighborhood, context-path, relationship, and about-index
semantics as engine-agnostic conformance scenarios; E2 implements these
tables on redb and must pass that suite unchanged.
