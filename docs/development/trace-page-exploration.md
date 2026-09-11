# Resumable focused adjacency exploration

Increment after PR706. F126 reproduces a 100-child parent consuming N16 before
any queued child expands. Preserve the default complete-parent BFS. When a
dimensional preference is supplied, use four raw semantic rows per expansion
turn, rather than reading all adjacency before selecting the next state. This
is an internal fixed policy, not another writer or reader parameter.

A pending path retains its offset into admitted shared adjacency. Each node
retains its exact indexed position and move index; alternatives reuse partial
and complete admitted rows in the same ReadTx. Count storage pages, coordinates,
and resumed expansion turns. Replay at most four cached rows per focused turn;
no repeated disk read or duplicate extension for the same path and offset.
Single-path mode can discard consumed rows while keeping the unfinished cursor.

The three-priority/one-FIFO schedule now counts expansion turns. Priority is
lexicographic (matches_preference, -effective_depth, -enqueue_ticket), with
effective_depth = path_depth + 1 for a resumed parent and path_depth for a new
state. Enqueue the continuation after its new children. It competes at child
depth instead of permanently outranking them. Fresh monotonically increasing
tickets prevent stale heap/FIFO entries from consuming a resumed state twice.
No score exclusion, shortest-path or global completeness guarantee.

The quantum is fixed at four before measurement and is not tuned to the new
outcomes. Coordinate admission still needs whole admitted label sets and may
consume the remaining N/E by itself. A useful edge late in index order may stay
unseen; no priority formula knows unread labels. E charges every actual raw row,
S charges eligible extension attempts including rejected cycles. A resumed turn
is not a new path state. Only exhausted adjacency with no eligible link is a
leaf; an unfinished parent or depth/work cut is not. Normal calls retain their
existing ordering, work and response shape.

Freeze a new protocol and input before comparing the PR706 binary to this
increment. Keep useful, misleading-bridge, dense, temporal and 100-hop controls;
add a late useful edge, multiple directed moves, partial-cache reuse under
alternative paths and work cuts. Compare exact evidence and semantic results,
not ingestion timestamps from separate stores. Rotate calls within each binary,
report local latency as exploratory, and preserve negative results. No API,
writer changes or independent readers before human Cala review.

## Frozen native comparison, 2026-09-11

Run `trace-pages-paired-20260911-v1` uses both executables sequentially against
eight once-seeded stores, reversing engine order on alternate cases. One warmup
and five rotating BFS/hard/focus triplets per engine/case. 373 RPCs include 11
expected old-cursor rejections; zero API. No tuning or implementation change
after freezing source388baf47, input8912601c72a66a766990f4180bc2c328de85c470b8fc375d4d1492001f55e19f
and binary51a86a4b2ced4a7ea7352d3eba1fdf00ff06c202204c9bf5f3d13dbc8b3d3e40.

| Focused control | Complete before → after | N before → after | E before → after | Median ms before → after |
| --- | --- | --- | --- | --- |
| Dense early child, N16 | 0 → 1 | 16 → 8 | 15 → 17 | 0.71 → 3.00 |
| Shared hub, two routes | 1 → 1 | 16 → 11 | 43 → 28 | 6.45 → 2.69 |
| 100-hop AND, N180 | 1 → 1 | 142 → 143 | 382 → 384 | 34.34 → 27.21 |
| Necessary nonpreferred bridge | 1 → 1 | 8 → 8 | 15 → 15 | 2.99 → 3.63 |
| Late-index useful edge, N16 | 0 → 0 | 16 → 16 | 15 → 39 | 2.10 → 6.62 |

All default BFS packets are byte-equivalent; hard proof, ordering and work are
equivalent except the three additional routing counters and their bound material
hash. Focus retains exact stored arrows and proof, real page reconstruction and
100-hop AND material103. Mixed directions and the future membership cut pass.
836 relevant Rust tests, workspace Clippy, guides/probes/surfaces/capabilities
pass, including independent-commit snapshot and partial-cache reconvergence.

Dense recovery is different work from a cheap failed read. Shared rows decrease
34.9%, with both routes retained. Local five-sample times are exploratory, not
confidence intervals or general speedups. Late-index failure spends more work
without recovery; it remains explicit. Whole response JSON grows by18 tokens in
most successful short cases from the routing observations. Deep13196→13216;
Trace definition3296→3369 (+73). No token-size CI gate or new required field.

Operational report, full CSV, PNG/SVG/PDF and freeze are in kmp-eval
`reports/TRACE-PAGES-20260911.md`. Next close the graph/body evidence snapshot
gap under #539; keep late-index discovery and a possible reverse search from
known destinations as separate hypotheses under #538. Do not turn these eight
designed registers into an overall benchmark score or a claim of semantic proof.
