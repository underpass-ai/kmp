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

This mode reads current same-about entries and rich links with why/evidence.
It does not apply an as-of cut, discover destinations from a question, return
several alternative paths to one target, or infer an AND/OR proof group.
For “what was known yesterday?”, first use the temporal verbs with the correct
clock. A recent link returned by this current-state trace cannot prove that
historical claim.
