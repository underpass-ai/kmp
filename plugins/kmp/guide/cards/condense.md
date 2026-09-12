Condense a canonical body you have read; reuse its short phrase on later paths.

1. Obtain descriptors: Trace the selected paths with
   `search:{proof:true,proof_refs:[]}`. Keep the manifest and finish page actions.
2. Execute the returned body expansion calls; finish their pages. Read the body
   before writing, and copy its descriptor `revision` and `record_digest`.
3. Replace every placeholder below with observed values:

```json
{"about":"<owning-about>","ref":"<read-ref>","language":"en",
 "scope":"node_body","card":"<your phrase about this body only>",
 "source":{"revision":1,"record_digest":"<copied digest>"},
 "expect":{"absent":true}}
```

Call `kmp_condense`; copy the actual source revision instead of the illustrative
`1`. For an existing card use its observed `expect:{card_revision:...}`. Text
must be at most 4096 UTF-8 bytes and shorter than its body. On conflict, reread.

4. Reuse: start the same Trace with `proof:true,compact:{language:"en"}`.
   **Omit `proof_refs`, `expect_selection` and the old page cursor.**

Card = derived orientation, never proof. Absent/stale/post-cut/wrong-language
cards return no prose; expand canonical bodies explicitly. Keep selected and
fetched refs under one manifest; finishing pages alone is not proof completion.
Complete call shapes: `guide:kmp-agent:example:reader-cards`.
