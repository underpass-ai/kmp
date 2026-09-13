# Graph adjacency read performance (#767)

## Problem and boundary

The embedded graph catalogue previously read outgoing adjacency once to find
reachable nodes and then read the same selected sources again to materialize
relations. The second pass decoded every relation explanation before checking
whether its target was selected. A high-degree source with large explanations
therefore paid twice for the SQLite row materialization and decoded payloads
that the scoped result discarded.

This change is internal to the embedded adapter. The domain ports, persisted
tables, relation format, traversal direction, depth and scope rules remain the
same. It does not add a process cache or a revision identity.

## Operation-scoped read

`OutwardNeighborhoodRead` runs inside the existing `ReadTx`, so adjacency,
nodes and explanations come from one SQLite snapshot. While walking an
interior source, it retains raw explanation bytes only when the target is
already selected or is admitted and therefore becomes selected by that edge.
Once traversal establishes the final set, it scans only the selected boundary
sources and path ancestors that were not traversed. The retained rows are
ordered by `(source, target, relation type)` before explanation decoding,
matching the former source-ordered relation pass.

The transient retained map is bounded by the serialized explanations and keys
of relations in the response. It does not retain discarded adjacency or
survive the operation. During one source scan, SQLite's returned row vector can
coexist with prior retained result rows; this preserves the existing engine
interface and portability at the cost of one source's complete outgoing
payload. The decoded result gradually replaces retained raw rows as the map is
consumed. There is no steady retained-heap saving to claim.

The catalogue keeps its prior isolated-root and depth-zero behavior: an empty
neighbor set has no relations. Context-path discovery still uses the existing
shortest-path walk. Its target subtree now shares adjacency with relation
materialization, while path ancestors visited during shortest-path discovery
may still be read again because retaining every searched branch would make
memory proportional to discarded search space.

## Correctness controls

The focused adapter tests compare the complete selected-ref and relation
result against an independent copy of the former two-pass algorithm. The
fixture covers scoped-out dimensions, duplicate typed edges between the same
nodes, cycles, depth-boundary relations, and nodes whose lifecycle status is
`SUPERSEDED` or `EXPIRED`. A recording transaction verifies one adjacency scan
per selected source and reports rows and stored explanation bytes.

A malformed scoped-out explanation proves discarded payloads are not decoded;
the same malformed bytes on a retained edge must still fail validation. A
pinned read transaction is held across an independent writer commit to prove
that relation membership and explanation text stay on the old snapshot, while
a new transaction observes the new explanation. Embedded conformance remains
the broader control for outgoing direction, depth, missing roots and targets,
shortest context paths, target subtrees, and incoming/outgoing relationship
reads.

## Informational benchmark

`graph_adjacency_benchmark` seeds an isolated SQLite store and performs scoped
neighborhood reads for four synthetic shapes: small, medium, high degree, and
large explanation payload. `scripts/performance/graph_adjacency.py` runs
baseline and candidate dev-profile executables in ABBA process order. Each
process reports its first read and at least 30 warm reads; the runner checks a
SHA-256 digest of the complete debug projection across revisions and records
whole-process CPU and peak RSS with `/usr/bin/time -v`.

The first read uses a freshly seeded process without evicting the OS page
cache. Whole-process peak RSS includes fixture construction and writes, so it
is only a coarse ceiling. The benchmark does not measure physical I/O or live
heap allocations. Source revision, toolchain, host, executable and runner
hashes are recorded with the raw samples under `artifacts/performance-767`.

Measured results will be recorded after a serialized quiet run.
