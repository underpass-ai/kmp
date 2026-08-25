# Requirements: Deep Graph Traversal & Rehydration from Any Node

> **Historical (fulfilled).** The graph-explorer requirements described here have been delivered. Retained for historical context.

**Priority:** P0 — blocks demo and interactive graph explorer
**Date:** 2026-03-18
**Related:**
- [`docs/PLAN_GRAPH_EXPLORER.md`](./PLAN_GRAPH_EXPLORER.md)

## Context

We are building an interactive application that lets a user navigate the full task graph visually — drilling into any node at any depth, viewing its details from Valkey, and rehydrating context from that point. The current kernel limits all queries to depth 1, which makes this impossible.

## Requirement 1: Unlimited Depth Traversal

The kernel must traverse the graph to **any depth** the client requests. No artificial caps.

- If the graph has 10 levels, depth=10 must return all 10 levels.
- The current `clamp(1, 3)` in the transport layer must be removed.
- The Cypher query must use variable-length paths (`*1..N`), not a hardcoded single-hop pattern.
- The `GraphNeighborhoodReader` port must accept a `depth` parameter and the Neo4j adapter must honor it.
- `GetContext` and `GetGraphRelationships` must both support depth.

A practical default (e.g., depth=10) is fine if no depth is specified, but the client must be able to override it.

## Requirement 2: Rehydrate from Any Node

`GetContext` must accept **any node** as `root_node_id` — not just top-level mission or story nodes.

- If I pass a subtask node ID, the kernel rehydrates from that subtask downward: its children, its relationships, its details, its context bundle.
- If I pass a leaf node with no children, the kernel returns that single node with its full detail.
- This enables an interactive explorer where the user clicks on any node and gets its rehydrated context — as if that node were the root of a smaller graph.
- The rendered context (`RenderedContext`) should reflect the subgraph from that node, not the full tree from the original root.

## Requirement 3: Node Detail Lookup from Valkey

A standalone RPC to fetch the full detail of a single node from the Valkey detail store.

When a user visits a node in the explorer, they need:
- Title, description/detail content
- All properties (as stored in `properties_json`)
- Content hash and revision
- Any history or timeline data associated with that node

Proposed proto:

```protobuf
rpc GetNodeDetail(GetNodeDetailRequest) returns (GetNodeDetailResponse);

message GetNodeDetailRequest {
  string node_id = 1;
}

message GetNodeDetailResponse {
  string node_id = 1;
  string title = 2;
  string detail = 3;
  string content_hash = 4;
  uint64 revision = 5;
  map<string, string> properties = 6;
}
```

This RPC reads directly from the Valkey detail store — it does not need to touch Neo4j.

## Use Cases

### Interactive Graph Explorer
1. User opens the explorer → `GetContext(root, depth=MAX)` → renders full tree
2. User clicks on a node → `GetNodeDetail(node_id)` → shows content panel from Valkey
3. User wants to "zoom in" on a subtask → `GetContext(subtask_id, depth=MAX)` → re-renders tree from that point
4. User navigates back up → `GetContext(parent_id, depth=MAX)` → re-renders from parent

### Demo Phase 7 (Rehydration)
1. TUI calls `GetContext("node:mission:engine-core-failure", role="implementer", depth=3)` → gets full 8-node task graph
2. TUI renders the tree with real statuses (done, abandoned, active, pending)
3. Ship's Log shows "KERNEL: GetContext: 8 nodes, 7 rels, N tokens" — all real

### Agent Context Rehydration
1. Agent is assigned to `node:task:seal-hull` → calls `GetContext("node:task:seal-hull")` → gets context for that specific task
2. Context includes only what's relevant to sealing the hull — not the full mission tree
3. This is the "surgical context" principle: 394 tokens, not 128,000

## Acceptance Criteria

- [ ] `GetContext` with a 3-level graph returns all levels (not just root + direct children)
- [ ] `GetContext` with a leaf node as root returns that node with its detail
- [ ] `GetContext` with a mid-level node returns that node + its subtree
- [ ] `GetGraphRelationships` with `depth=N` returns N levels
- [ ] `GetNodeDetail` returns the Valkey-stored detail for any valid node ID
- [ ] No hardcoded depth caps — client controls traversal depth
