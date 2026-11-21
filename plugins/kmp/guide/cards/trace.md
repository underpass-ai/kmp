Trace with ONE search mode:

| Need | Call shape |
| --- | --- |
| Known destination | `about`, `from`, `to`; optional destination policies |
| Evidence from a seed | `about`, `from`, `search.seek` roles; no `to` |

Destination policies (`direction` or `follow`, `relations`, `dimensions`,
`prefer_dimensions`, `paths_per_target`, `select`) apply only with `to`. For
example, discover verification from a seed:

```json
{"about":"<about>","from":"<seed-ref>","search":{
 "seek":[{"name":"verification","rel":"verified_by","via":"context"}],
 "proof":true,"proof_refs":[]}}
```

`via:"context"` gives contextual leads: a neighbor's verification does not
verify this seed. Review candidates, bindings, groups and original sources.
Preserve `review_required`, unknown clocks and missing obligations. The work
limits (`max_nodes`, `max_edges`, `max_states`) bound the search, not the
answer. Proof options are topic `audit`.

More: `guide:kmp-agent:example:evidence-seek`.
