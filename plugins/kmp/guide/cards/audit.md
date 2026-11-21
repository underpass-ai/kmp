Inspect one returned ref:

```json
{"about":"<owning-about>","ref":"<returned-ref>"}
```

Replace placeholders with refs memory returned. Inspect returns the object and
its proof; a receipt's `object.text` is the immutable accepted command, not
current memory. Evidence, links and raw records page under the byte ceiling,
and a continuation can reuse the first page's object. Finish `next_actions`
pages before concluding.

Trace the connection between two refs with `about`, `from` and `to`. The search
modes and their options are topic `trace`; this topic covers the call, clocks,
budget and proof options. For known-by questions add `axis:"observed"` and an
`as_of` cut explicitly. A `to` array, `search` or a temporal selection runs a
bounded search through same-about entries, one shortest route per destination
by default. A single `to` without them traces over every stored relation with
a bounded bidirectional search; a work cutoff returns no path, the stop in
`search` and an optional `search.widen`.

With `search.proof:true`, `proof_refs:[]` shows descriptors without bodies;
execute the body expansion actions and join fetched refs under one manifest
(`expect_selection`). `max_body_record_bytes` bounds fetched bodies. Paths and
fetched groups do not establish that an answer is complete. A changed selection
needs a fresh read. For short reusable cards after reading bodies, open topic
`condense`.

More: `guide:kmp-agent:example:bounded-trace`.
