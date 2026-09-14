Condense a canonical body you have read; reuse its short phrase on later paths.

1. Trace with `search:{proof:true,compact:{language:"en"}}`. Read
   `proof.condense_candidates.items`: shared sources and large bodies come first.
   The same bounded list appears on every page. Keep the manifest and finish pages.
2. Read a chosen candidate's canonical body with a manifest-bound named expansion
   priced by its `record_bytes`. Copy its `source` and `expect` objects directly.
3. Replace every placeholder below with observed values:

```json
{"about":"<owning-about>","ref":"<read-ref>","language":"en","actor":"<reader-name>",
 "scope":"node_body","card":"<your phrase about this body only>",
 "source":{"revision":1,"record_digest":"<copied digest>"},
 "expect":{"absent":true}}
```

Call `kmp_condense` with the candidate's actual `source` and `expect`, replacing
the illustrative values above. Without `context_id`, `actor` is required; with
a valid context, an omitted actor defaults to the persistent agent name. Text
must be at most 4096 UTF-8 bytes and shorter than its body. On conflict, reread.

4. Reuse: start the same Trace with `proof:true,compact:{language:"en"}`.
   **Omit `proof_refs`, `expect_selection` and the old page cursor.**

Card = derived orientation, never proof. Absent/stale/post-cut/wrong-language
cards return no prose; expand canonical bodies explicitly. Keep selected and
fetched refs under one manifest; finishing pages alone is not proof completion.
Complete call shapes: `guide:kmp-agent:example:reader-cards`.
