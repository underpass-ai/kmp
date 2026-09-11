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
