Inspect one returned ref:

```json
{"about":"<owning-about>","ref":"<returned-ref>"}
```

Replace placeholders with memory results. Inspect returns the object and its
proof; a receipt instead contains the historical accepted command.

Trace connections with ONE mode:

| Need | Call shape |
| --- | --- |
| Known destination | `about`, `from`, `to`; optional destination search policies |
| Evidence from seed | `about`, `from`, `search.seek` roles; no `to` or destination policies |

For example, discover verification from a seed:

```json
{"about":"<about>","from":"<seed-ref>","search":{
 "seek":[{"name":"verification","rel":"verified_by","via":"context"}],
 "proof":true,"proof_refs":[]}}
```

`via:"context"` gives contextual leads; a neighbor's verification does not verify
this seed. Review candidates, bindings, groups and original sources. Preserve
`review_required`, unknown clocks and missing obligations. For known-by
questions, add the task's `axis:"observed"` and `as_of` cut explicitly.

Finish returned page actions before resolving indexes. With `proof:true`,
`proof_refs:[]` shows descriptors without bodies; execute body expansion actions
and join fetched refs under one manifest. Paths and fetched declared groups do
not establish answer completeness. A changed selection requires a fresh read.

Read `guide:kmp-agent:example:evidence-seek` for roles/joins;
`guide:kmp-agent:example:bounded-trace` for destinations and dimensions.
For short reusable cards after reading canonical bodies, open topic `condense`;
`guide:kmp-agent:example:reader-cards` teaches the complete delivery workflow.
