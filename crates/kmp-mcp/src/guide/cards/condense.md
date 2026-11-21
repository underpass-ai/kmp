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
the illustrative values above. An omitted `actor` is the MCP client's name. Text
must be at most 4096 UTF-8 bytes and shorter than its body. On conflict, reread.
A search rendering of a stored memory (`summary_en`, `search_expansions`) is a
`kmp_write_memory` call with `search_summaries`; `kmp_summaries_audit` lists
what is still owed.

4. Reuse: start the same Trace with `proof:true,compact:{language:"en"}`.
   **Omit `proof_refs`, `expect_selection` and the old page cursor.**

A card stands only for the body version you declare: a later write makes it
stale, not wrong, and a body that moved under you is refused with both
versions named. Accepted revisions survive export/import and rebuild.
`kmp_summaries_audit` reads the event log and writes nothing: each memory is
`missing`, `refused` (with the lint's faults), `stands` (with weakness signals)
or `not_required`; render the text again and attach it unchanged. It judges
form, not meaning, unless the store opted into Jev, which flags `unfaithful`.
Page with `page.cursor` and the same arguments; totals cover the whole selection.

Card = derived orientation, never proof. Absent/stale/post-cut/wrong-language
cards return no prose; expand canonical bodies explicitly. Keep selected and
fetched refs under one manifest; finishing pages alone is not proof completion.
Complete call shapes: `guide:kmp-agent:example:reader-cards`.
