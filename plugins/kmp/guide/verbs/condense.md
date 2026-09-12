Use **Condense** after reading a canonical body that will be useful again.
It stores your phrase for one entry or evidence source, in one language.
It does not summarize automatically or verify the phrase's faithfulness.

1. Trace with `search.proof:true,proof_refs:[]` to obtain descriptors and
   `proof.manifest_id`. Inspect and legacy Trace do not expose the required
   record digest. Finish the selected pages, then execute body expansion calls.
2. Read the canonical body. Copy its descriptor `revision` and `record_digest`
   into `source`; never calculate or substitute `content_hash` for that digest.
3. Write `about`, `ref`, `language`, `scope:"node_body"`, `card`, `source` and
   `expect`. Use `expect:{absent:true}` first, or the observed `card_revision`
   to replace an existing card. The text must be nonempty, at most 4096 UTF-8
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
