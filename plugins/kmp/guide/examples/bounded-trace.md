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

If `search.stop_reason` is `node_budget`, `edge_budget` or `depth_budget`,
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

This mode does not discover destinations from a question, return several
alternative paths to one target, evaluate supersession, or infer an AND/OR proof
group. Use temporal verbs to discover entries and Inspect for their full sources.
