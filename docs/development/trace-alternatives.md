# Bounded candidate routes

The previous shared BFS keeps one predecessor per entry. That is sufficient for
one shortest route per destination, but discards alternatives whose shared
material may make a set of routes cheaper to read.

`search.paths_per_target` requests up to 1..8 simple candidate routes per known
destination (default 1). `search.follow` optionally names up to 16 allowed
`{rel, direction}` moves, instead of global `direction`/`relations`. This is an
unordered permission set, not an ordered path expression or a query language.
Stored arrows, rationale and evidence are unchanged by reverse traversal.

For source s, targets T and allowed directed moves A, candidates are simple
paths p=(s,...,t), t in T, with each move in A and every entry/link admitted by
the existing about and clock selection. Enumerate breadth first, break ties by
relation name, outgoing before incoming, then the storage keyset order. For
paths_per_target=1, retain the shared visited-node BFS. For larger quotas, keep
parent-linked path states and reject any repeated node in that path. A source
that is itself a target needs only its zero-hop path.

N counts distinct discovered refs including excluded endpoints and coordinate
labels; E counts decoded adjacency/coordinate rows; D bounds hops. New S
(`max_states`, default4096, maximum32768) counts the admitted root and every
eligible extension attempted, including cycle and visited-node rejections.
Reserve S before checking a path. Parent lookup costs at most O(D) per attempt;
state storage is O(S), with at most O(E) distinct cached edges. A node's eligible
adjacency is cached only after complete expansion in the same read transaction.
Process each first-read page immediately so useful early paths survive a later
N/E cutoff. No full-graph catalogue is materialized before discovery.

Targets reached means the requested quotas were obtained, not enumeration of
all paths or semantic proof completeness. `unreached_targets` have no route;
`incomplete_targets` have fewer routes than requested. Exhaustion is relative
to the selected moves/about/clock. Depth or work cuts are never reported as
leaves. Unknown link clocks retain their explicit uncertainty.

The result carries source, allowed moves, work counters and shared relation
indexes. Starting at source, matching each stored endpoint reconstructs the
walking direction. Join all pages before resolving indexes. Transport paging
repeats this selection; it does not resume search or promise a cross-call snapshot.

This delivers candidate discovery, not the final M2 selector. Compare candidate
sets on identical frozen stores before applying the external frozen M2 beam
(B4, alpha0.5, lambda0). Keep N explored separate from unique material selected.
No optimality guarantee: quotas and budgets can omit the best joint set. Next
controls cover shared material, mixed directions, deep AND requirements, cycles,
hubs and exclusions. Dimensional predicates and body snapshot consistency remain
separate pending work under #538/#539. Free-text goal is not a scoring feature.
