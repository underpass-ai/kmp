"""Far trace pairs: entries a synth-v1 world connects only through several declared hops.

The world's declared graph is its `relations` records, directed from `from` to `to`,
within one about: `kmp_trace` with a single `to` runs a bidirectional search, but over
directed relations ("no directed trace reaches ..." otherwise).
A far pair is two entries of the ladder's lowest level whose shortest declared route
has at least `min_hops` hops. Both endpoints exist at every rung, so the same pairs
are traced at every level and the latency they show is the latency of a longer store,
not of a different question.

Which pairs are taken is fixed by their content alone: every candidate is ranked by
the SHA-256 of `seed|from|to` and the first `pairs` are kept (`pairs = ALL`: every one). Each pair carries one
reference route (the lexicographically smallest shortest route) and every declared
edge that lies on some shortest route, so any shortest declared route is accepted.
Reverse adjacency (`reverse(graph)`) gives the hop counts to a destination.
"""
from collections import deque
from dataclasses import dataclass

from . import cachekey
from .errors import BenchError


ALL = 'all'  # pairs: every far pair of the rung


class FarTraceInvalid(BenchError):
    code = 'FAR_TRACE_INVALID'


@dataclass(frozen=True)
class FarTraceSpec:
    min_hops: int  # k: shortest declared route of at least k hops
    pairs: int | str  # how many pairs a topology traces, or ALL

    def __post_init__(self):
        if isinstance(self.min_hops, bool) or not isinstance(self.min_hops, int) or self.min_hops < 2:
            raise FarTraceInvalid('far_trace.min_hops: an integer >= 2 (1 hop is a neighbour)')
        if self.pairs != ALL and (isinstance(self.pairs, bool) or not isinstance(self.pairs, int)
                                  or self.pairs < 1):
            raise FarTraceInvalid(f'far_trace.pairs: an integer >= 1 or "{ALL}"')


@dataclass(frozen=True)
class FarPair:
    about: str
    start: str
    end: str
    hops: int
    steps: tuple  # one shortest declared route, start..end
    accept_edges: tuple  # (a, b) declared edges on some shortest route, sorted


def declared_graph(relations, nodes):
    """{about: {ref: set(successor refs)}} over the relations whose two ends are in `nodes`."""
    graph = {}
    for relation in relations:
        start, end = relation['from'], relation['to']
        if start == end or start not in nodes or end not in nodes:
            continue
        if _about(start) != _about(end):
            continue  # a cross-about link is a proposal, not a declared same-about hop
        adjacency = graph.setdefault(_about(start), {})
        adjacency.setdefault(start, set()).add(end)
        adjacency.setdefault(end, set())
    return graph


def reverse(adjacency):
    """The same edges pointing the other way: hop counts to a node instead of from it."""
    backwards = {node: set() for node in adjacency}
    for node, successors in adjacency.items():
        for successor in successors:
            backwards[successor].add(node)
    return backwards


def _about(ref):
    return ref.rsplit(':', 1)[0]


def distances(adjacency, source):
    """Breadth-first hop counts from `source` along the adjacency's edges."""
    seen, queue = {source: 0}, deque([source])
    while queue:
        node = queue.popleft()
        for neighbour in sorted(adjacency.get(node, ())):
            if neighbour not in seen:
                seen[neighbour] = seen[node] + 1
                queue.append(neighbour)
    return seen


def histogram(graph):
    """{hops: ordered pairs} of every same-about pair with a directed route: what k can be chosen from."""
    counts = {}
    for adjacency in graph.values():
        for source in adjacency:
            for target, hops in distances(adjacency, source).items():
                if target != source:
                    counts[hops] = counts.get(hops, 0) + 1
    return dict(sorted(counts.items()))


def _route(adjacency, start, end, from_start, to_end):
    hops = from_start[end]
    edges = sorted((a, b) for a in from_start for b in adjacency[a]
                   if b in to_end and from_start[a] + 1 + to_end[b] == hops)
    steps, node = [start], start
    while node != end:
        node = min(n for n in adjacency[node] if n in to_end and to_end[n] == to_end[node] - 1)
        steps.append(node)
    return tuple(steps), tuple(edges)


def far_pairs(graph, spec, seed):
    """The `spec.pairs` far pairs of `graph` (all of them for ALL), in rank order; fewer when
    the graph has fewer."""
    ranked = []
    for about in sorted(graph):
        adjacency = graph[about]
        for start in sorted(adjacency):
            for end, hops in distances(adjacency, start).items():
                if hops >= spec.min_hops:
                    rank = cachekey.sha256_hex(f'{seed}|{start}|{end}'.encode('utf-8'))
                    ranked.append((rank, about, start, end))
    chosen = []
    for _, about, start, end in sorted(ranked)[:None if spec.pairs == ALL else spec.pairs]:
        adjacency = graph[about]
        from_start, to_end = distances(adjacency, start), distances(reverse(adjacency), end)
        steps, edges = _route(adjacency, start, end, from_start, to_end)
        chosen.append(FarPair(about, start, end, from_start[end], steps, edges))
    return tuple(chosen)
