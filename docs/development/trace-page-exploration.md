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
