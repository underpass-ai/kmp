Inspect a ref copied from a result:
`kmp_inspect {"about":"project:sample","ref":"<returned ref>"}`.
Expect the canonical object, evidence and typed incoming/outgoing links. Each
rich link carries its own why and evidence. Mere adjacency does not prove a claim.

Follow returned continuations with their complete arguments. If reusing the
first object, use the supported object-reuse continuation; do not repeatedly
load the same body. Inspect is a current object view, not a historical proof.
Return to a temporal verb when the task asks what was known or held earlier.

Trace a relied-on connection using the exact two refs and about. The linked
audit contract gives the request shape. Finish all selected proof pages before
saying there is no path. A write receipt is the immutable accepted command,
not the current state of the memories it created.

More: `guide:kmp-agent:example:budget-proof` and
`guide:kmp-agent:example:decision-history`.

For bounded routes, optional `search.select.max_material_nodes` selects joint
material before full links are returned. Supply AND/OR `groups` only when the
required target refs are known. Read `search.material`: zero selected paths can
mean no complete group fits despite available candidates. Finish `next_actions`
for selected proof; use `search.material.expand_candidates` only when the omitted
alternatives are needed. See `guide:kmp-agent:example:bounded-trace`.

Use `search.prefer_dimensions` for a label hint; it prioritizes matching entries
while reserving FIFO exploration for bridges. `search.dimensions` is a hard
filter on every path entry and can remove those bridges. Both reuse native
selectors on coordinates admitted by the selected clock. See `search.routing`;
focus may return a longer route first and all work remains bounded. Focus yields
after small adjacency pages so queued children can progress; page positions and
partial rows are reused. A work cut or unexpanded boundary is not a known leaf.

For proof discovery from a known seed without destination refs, use
`search.seek:["verified_by",{"name":"permission","rel":"authorizes","direction":"incoming"}]`.
Optional `same_labels:["event"]` requires a shared event value at the witnesses.
Read `seek.status` and joint `groups`; missing labels stay unknown. No token score
prunes paths, but work cuts remain partial. Finish all pages before interpreting
indexes. Expand `guide:kmp-agent:example:evidence-seek` for identity, ordered
via/after paths, clocks and limits.

If intermediate moves are unknown, use `{"rel":"verified_by","via":"context"}`.
KMP discovers minimum-hop prefixes to reachable relation origins; ties survive.
Read `context_hops` and the arrows. `context_discovery:true` requires review and
keeps `declared_obligations_complete:false`: the neighbor's verification does
not automatically verify the seed. Longer prefixes are omitted. The example
explains discovery, witness constraints and the source-read step.
