# Bounded directed navigation

The first storage building block is `BoundedRelationReader::read_adjacency`.
It reads one node's outgoing or incoming index with an explicit positive row
limit and an exclusive `(neighbor, relation)` position. Incoming reads preserve
the original source, target, type, explanation and evidence. They never create
an inverse semantic relationship.

SQLite seeks the composite primary key and applies `LIMIT` before returning
values. A later page does not use OFFSET or load the complete adjacency first.
Outgoing reads retrieve at most the requested number of relation values.
Incoming reads retrieve that many index rows and then that many canonical
values, within the same read transaction. This does not bound the byte size of
one stored explanation or all internal SQLite page activity.

A full page is **not** evidence that more relations exist or that the node is a
leaf. It carries a next key without reading a sentinel row. Only `exhausted`
establishes the end of that directed adjacency, sometimes after an empty final
page. An empty page says nothing about whether the node itself exists.

The position is an internal index key, not an MCP continuation, authorization or
revision token. Separate public port calls open separate snapshots. The
transaction-local `bounded_adjacency::read_page` can be reused by one complete
traversal so all its pages see the same snapshot. A concurrent independent
writer test verifies this distinction. The port does not apply about ownership,
dimension filters or clocks; the memory traversal must admit each candidate
before including it in a route or exposing it to an agent.

## Remaining integration

This increment does not add or change an MCP tool. Existing temporal/Inspect
paths still use their current readers. There is no agent-facing route selector
or end-to-end N guarantee yet. Do not describe the port's row limit as a limit
on all graph nodes, proof bodies or the completed memory query.

The next layer must keep one traversal snapshot; apply scope and time to nodes
and relations; choose typed, directed actions; share a ledger across candidate
paths for unique memory/auxiliary nodes, examined edges and search states; and
distinguish reached targets, adjacency exhaustion, excluded evidence and budget
cutoff. It must preserve the complete stored proof after selecting several
paths. A full catalogue followed by a cap is insufficient. No new writer field
is needed. #538 and #539 retain those acceptance criteria.

Behavioral controls cover direction and exact evidence, multi-relation cursor
keys, full-page versus exhaustion, opaque ids and a same-snapshot read across
another engine's commit. A late page of seven among 10,000 neighbors uses the
composite-key search with 109 SQLite VM steps in the local control; this is not
whole-query performance. No new editorial CI gate is introduced.
# Native bounded Trace baseline

`kmp_trace` now uses this port when `to` is an array (1–8 destinations), or
`search` is supplied. Embedded and gRPC map the same request/result. The embedded
adapter borrows one SQLite `ReadTx` for source ownership, every adjacency page,
every admitted endpoint's ownership and every returned relation explanation.
Other graph adapters reject this capability explicitly rather than falling back
to a full graph load. Ordinary single-destination Trace is unchanged.

The coordinator is the deterministic **unit-cost baseline**, not M2:

\[
  c(e)=1,\quad d(v)=\min_{p:s\leadsto v}|p|,\quad
  R=\bigcup_{t\in T_{reached}}p_t.
\]

A shared BFS visits each admitted entry at most once. It returns one discovered
shortest-hop route per explicit destination, preserving arrows when walking the
incoming index. Edges are eligible only when non-structural, carrying nonempty
stored rationale/evidence and matching any requested exact type filter. This
qualifies a stored link, not truth. Shared edges occupy one table row; each route
contains zero-based indexes into the full unpaged table. Finish transport pages
before resolving them. The selection fingerprint includes all selected edges,
route indexes and search metadata, including content outside the returned page.

The request limits discovered refs (`max_nodes`, N), decoded adjacency rows
(`max_edges`, E) and hop depth (`max_depth`, D). Discovery includes rejected
foreign/structural/missing endpoints. Before reading a page of q rows reserve
`q <= min(N - discovered, E - scanned, 32)`. Each row can reveal at most one
new endpoint. Counters never refund filtered work. N bounds identities and node
lookups; it does **not** bound bytes in a stored node or explanation. Incoming
rows also require canonical relation point gets. E is decoded adjacency rows,
not SQLite VM instructions, physical pages or all point gets. BFS states are
at most N, with O(N + E) retained graph state; ordered maps/sets add log N CPU
factors. No graph-wide catalogue or detail-body batch is requested.

`targets_reached` establishes those graph connections only. `frontier_exhausted`
and the leaf count are relative to admitted about/direction/type/rich-link
filters. `node_budget`, `edge_budget` and `depth_budget` mean incomplete search.
No probe after exhausted allowance is made merely to certify a leaf. There is
no resumable search cursor in this increment: transport actions finish the
selected relation table; a larger search is a new selection and snapshot.

Scope is current same-about **entries** verified from the stored projection;
foreign endpoints, dimensions and direct evidence nodes are not expanded. This
mode does not apply a historical cut, label predicates, lifecycle filtering or
cross-about equivalences; it does not return entry bodies or direct source-node
proof. Nodes contain projected payload metadata, so reading them can internally
decode entry text even though it is not returned. No claim that #538 or #539 is
complete follows from this baseline. Pending: temporal admission, useful route
alternatives, M2 group selection, direct proof loading in the same snapshot,
byte-aware discovery and revision-keyed reader phrases. Writers gain no fields.
