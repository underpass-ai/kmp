Use **Condense** after reading a canonical body that will be useful again.
It stores your phrase for one entry or evidence source, in one language.
It does not summarize automatically or verify the phrase's faithfulness.
Accepted cards append an event and update the card projection in one transaction.
Their own revisions survive export/import and projection rebuild; replacing a
card preserves its previous text and does not advance the canonical body's
revision. Historical reads select the latest card authored at or before their
cutoff, then check that it still describes the selected body.

1. Trace with `search:{proof:true,compact:{language:"en"}}` to obtain
   `proof.condense_candidates` and `proof.manifest_id`. Its `items` rank shared
   sources first, then larger bodies, then ref. The list is unchanged on every
   page; finish the selected pages before assessing the selected proof.
2. Choose a candidate and read its canonical body with a named expansion bound
   to the manifest, using its `record_bytes` as the one-body ceiling. Copy the
   candidate's complete `source` and `expect` objects into Condense. No digest
   calculation or descriptor transcription is needed. Descriptors remain the
   path for a deliberately chosen body outside the recommendations.
3. Write `about`, `ref`, `language`, `scope:"node_body"`, `card`, `source` and
   `expect`. The candidate supplies `expect:{absent:true}` or the exact stored
   `card_revision` for a stale card. The text must be nonempty, at most 4096 UTF-8
   bytes, and strictly shorter in bytes than its body. Include `actor` without
   `context_id`; with a valid context, an omitted actor uses the persistent name.
4. Start the same Trace selection afresh with
   `search.proof:true,compact:{language:...}`. **Remove `proof_refs`,
   `expect_selection` and the old `page.cursor`.** Otherwise named refs request
   canonical bodies, taking precedence over compact delivery.

The card represents only that body, not a neighborhood or path. Changed source
or card revision refuses the write; read the current state before revising it.
Valid text appears with `body_state:compact`. An absent, stale, wrong-language
or post-cut card supplies no prose and never silently loads the canonical body.
Expand it explicitly when needed. Cards never close proof groups.

Complete calls, status meanings and recovery:
`guide:kmp-agent:example:reader-cards`.

The list offers at most eight absent/stale bodies of at least 1024 UTF-8 bytes.
`omitted_count` counts the remaining eligible targets. `valid`, `after_cut`
and `below_floor` are disjoint exclusions, card status taking precedence over
size; missing bodies are outside these counts. `shared_by` counts returned
routes or seek groups with known clocks and all binding witnesses, once per
route/group. Groups retained for review do not count; canonical body delivery
is independent.
The floor is a targeting heuristic, not a savings guarantee. A card written now
cannot appear in a read whose cutoff is already past. The list is orientation,
not a required action or permission to skip canonical source review.
