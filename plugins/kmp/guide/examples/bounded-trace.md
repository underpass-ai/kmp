# Several destinations, one search

The source register says: artifact A requires B; B requires C; a separate
report V verifies A. These facts and their relation-specific why/evidence
are already stored. Obtain their exact entry refs from the write receipt or
a read. Below `ref-A`, `ref-C`, `ref-V` and `about` stand for those returned
values, not refs to construct or send literally.

Call `kmp_trace` with:

```json
{"about":"about","from":"ref-A","to":["ref-C","ref-V"],
 "search":{"max_nodes":128,"max_edges":256,"max_depth":100,
           "direction":"outgoing","relations":["depends_on","verified_by"]}}
```

Expect two routes if the stored graph reaches both destinations. Each route
indexes the complete `trace` relation table in hop order. Shared edges appear
once in that table. Follow `next_actions` until all pages are retained before
resolving the zero-based `edge_indexes`. The route A→B→C preserves two declared
dependency statements; the route A→V preserves the report's verification
statement. Neither tells you that those are all the sources a question needs.
Inspect the returned entry refs when the bodies or direct source evidence matter.

For “what depends on C?”, start from C with `direction:"incoming"` and
destinations A and B. The traversal goes C←B←A; the returned statements still
say A depends on B and B depends on C. Never reverse their meaning.

If `search.stop_reason` is `node_budget`, `edge_budget`, `depth_budget` or `state_budget`,
unreached destinations remain unknown. `page.has_more:false` only finishes
the selected relation table. There is no hidden search continuation: a larger
search allowance starts a new selection. Even an exact-size final storage page
can leave exhaustion unknown when no allowance remains for another probe.

For “which declared paths were known at noon?”, keep the same call and add:

```json
{"as_of":{"time":"2026-09-10T12:00:00.500Z"},"axis":"observed"}
```

This fragment extends the complete call above. Suppose A, B and C were observed
at 10:00, V at 13:00, and the A→V link was also first observed at 13:00. Only
A→B→C is eligible at noon. Even if V had existed earlier, the 13:00 link would
still be excluded. Occurred time answers when events happened, not when they
became known. Use `interval` instead of `as_of` for a half-open span; never send
both. `axis` without either is rejected.

An `as_of.ref` resolves the earliest canonical coordinate of that returned entry
on the selected clock. `search.resolved_as_of` reports it. Coordinate discovery
shares the node and edge budget with path search. If the budget cannot resolve
the cut, `temporal_selection_resolved:false` and the stop reason say so. A known
source outside the cut returns `source_outside_selection`, not a missing ref.
`coordinate_rows` is included in `scanned_edges`; label refs also count as nodes.

A selected link without a usable clock remains explicitly unknown. Its index in
`search.clock_unknown_edges` refers to the complete relation table after pages
are joined. It is not evidence that the link already existed at the cut.

This mode does not discover destinations from a question, evaluate supersession, or infer an AND/OR proof
group. Use temporal verbs to discover entries and Inspect for their full sources.


## Alternative paths and mixed directions

A later claim R corrects old claim A; verification V verifies R. The stored
arrows are R --corrects--> A and R --verified_by--> V. To walk from A to V:

```json
{"about":"about","from":"ref-A","to":["ref-V"],
 "search":{"follow":[{"rel":"corrects","direction":"incoming"},
                     {"rel":"verified_by","direction":"outgoing"}],
           "paths_per_target":2}}
```

Use exact refs returned by a write/read. `follow` replaces `direction` and
`relations`; do not combine them. It allows these moves wherever applicable,
not a required order of types. KMP preserves both stored arrows and their proof.
Starting at `search.from`, match each endpoint to reconstruct the walk. A
correction is not made true merely because a path reaches it.

`paths_per_target` defaults to1. With2, the reader can compare two routes to V
when stored alternatives exist. If only A←R→V exists, the result has one route,
V in `incomplete_targets`, no `unreached_targets`, and an exhausted selected
frontier. With2 discovered paths, `targets_reached` means the quota is met;
there may be more paths. A route through a target may continue to another one.
Cycles are excluded. Source equal to target needs just its zero-hop route.

Two routes to different destinations can share a useful prefix: selecting both
may require less unique material than selecting separately found routes. This
call returns candidates, not the best joint set. Quotas can hide a better set.
`max_states` (default4096) bounds the admitted root plus every eligible extension
attempt, including rejected cycles and visited nodes. Completed adjacency is
reused inside the same snapshot. N/E still count actual discovered refs/decoded
rows, not final context size. Compare those costs separately from material read.


## Select shared material before reading all alternatives

Suppose the stored paths are A→B→C, A→D→V, A→H→C and A→H→V.
The reader needs C and V and already has their exact returned refs. This call
can select the shared paths through H under four distinct entries:

```json
{"about":"about","from":"ref-A","to":["ref-C","ref-V"],
 "search":{"paths_per_target":2,"select":{"max_material_nodes":4}}}
```

Without groups, each target is one equally weighted requirement. Source A and
shared H count once; `search.max_nodes` still bounds all discovered refs.
`search.material` reports original candidate indexes and the selected material.
`routes` indexes only the selected relation table. All selected arrows, why and
evidence remain intact. The fixed beam evaluates combinations; it does not
promise the optimum or determine which sources are sufficient for your answer.

For “I need C AND V”, extend the select object with:

```json
{"max_material_nodes":4,"groups":[{"alternatives":[["ref-C","ref-V"]]}]}
```

For “either C AND V, OR replacement report R”, include returned `ref-R` in `to`
and use this select object:

```json
{"max_material_nodes":4,"groups":[{"weight":2,
 "alternatives":[["ref-C","ref-V"],["ref-R"]]}]}
```

These requirements are the reader's explicit choices. Do not invent replacement
or identity equivalence from matching words. Group indexes follow input order;
without explicit groups they follow sorted target refs. A group is complete only
when one whole alternative is in selected path material. Partial ANDs get no
reported benefit, even though the beam can use partial progress to explore.

If the AND example is limited to two material entries, neither complete route
fits. Expect `candidate_count` to remain positive, no selected routes and an
incomplete group. This does not say the evidence is absent. Execute the returned
`search.material.expand_candidates` only if you need the full bounded catalogue.
Follow its `next_actions` to finish that new read. Expansion retains about, time,
search limits and direction, removes select and the old page cursor, and incurs
additional context. It is optional, separate from selected-proof pagination, and
does not retain a snapshot across calls. Body reads still use Inspect; graph/body
snapshot consistency is not supplied by material selection.
