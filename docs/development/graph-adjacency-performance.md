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

A test-only `ReadTx` recorder over the scoped cyclic fixture measured the
operation boundary directly. The former algorithm made 7 adjacency calls,
returned 13 rows / 1,239 explanation bytes and decoded 7 rows / 659 bytes
during relation filtering. The operation-scoped reader made 4 calls, returned
7 rows / 659 bytes and decoded the 5 retained rows / 463 bytes. These counts
describe that fixture, not a production telemetry surface.

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

The serialized quiet run used two fresh processes per revision and shape, for
two first-read observations and 60 warm observations per side. The complete
result digest, neighbor count and relation count matched between revisions for
every shape.

| shape | selected entries / retained / discarded degree | former warm p50 / p95 | operation-scoped warm p50 / p95 | change at p50 / p95 |
| --- | --- | ---: | ---: | ---: |
| small | 8 / 1 / 1 | 0.357 / 1.034 ms | 0.707 / 1.059 ms | +97.9% / +2.4% |
| medium | 128 / 2 / 4 | 16.000 / 16.555 ms | 10.140 / 10.742 ms | -36.6% / -35.1% |
| high degree | 256 / 8 / 16 | 66.397 / 67.215 ms | 41.274 / 41.908 ms | -37.8% / -37.7% |
| large payload | 64 / 2 / 8 | 107.618 / 109.466 ms | 37.401 / 38.034 ms | -65.2% / -65.3% |

The two first-read observations in shape order were 0.456/0.453 versus
0.725/0.356 ms, 15.688/16.359 versus 10.083/10.325 ms, 65.802/67.389 versus
40.806/41.786 ms, and 110.709/108.429 versus 38.283/39.387 ms. They are
descriptive cold-process controls; the OS page cache was not evicted.

The small fixture is an unfavorable case. Its total work is below a
millisecond, and both revisions showed scheduling outliers during later warm
samples; the candidate still raised the pooled median by about 0.35 ms. The
ordered retained map and final decode add fixed work that the removed scan
does not repay at this size. No absolute latency assertion is based on this
control.

Whole-process peak RSS was mixed and effectively flat at this resolution:
5,472–5,540 KiB versus 5,472–5,632 KiB for small, 10,648–10,656 versus
10,584–10,856 KiB for medium, 21,480–21,500 versus 21,612–21,760 KiB for high
degree, and 48,244–48,412 versus 48,172–48,400 KiB for large payload. This
does not demonstrate a resident-memory improvement. Whole-process CPU
(fixture writes plus 31 reads) decreased on the three substantive shapes:
1.29–1.33 to 1.14–1.15 seconds, 4.53 to 3.79–3.80 seconds, and 11.04–11.12 to
8.68–8.75 seconds. Small ranged from 0.03–0.05 to 0.04–0.06 seconds.
