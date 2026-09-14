# Optional consolidated views

Use a consolidated view when repeated source reports compete for context. The
writer supplies the interpretation; KMP groups exact compatible declarations,
checks literal quotes and source versions, and never generates prose. A matching
claim key is not independent proof that two reports mean the same thing.

This workflow is available through the embedded application and local CLI,
using the same `KMP_MCP_DATA_DIR` selection as other local commands. It adds no
MCP tool or remote gRPC operation. Normal writing, Ask and temporal navigation
continue to operate on canonical sources.

1. Run `kmp-mcp consolidation sources sources.json`, with
   `{"about":"project:sample","refs":["source ref returned by memory"]}`.
   Read the captured bodies, provenance, status and direct relation evidence.
   Capture every source whose interpretation the view depends on; the kernel
   cannot recover an unstored source or infer omitted semantic dependencies.
2. Author `write.json`. Declare `about`, a stable `view` name, `author`, one
   `idempotency_key`, `expect_revision:0` for creation, and `sources` mapping each
   exact returned ref to its returned `stamp`. Each `assertions` member contains
   `source_ref`, a literal `quote`, `why`, and `claim` with `referent`, `predicate`,
   `value`, `temporal_scope`, `polarity` (`affirmed` or `negated`),
   `epistemic_status` (`established`, `reported`, `tentative`, `unknown`), and an
   explicit array of `qualifiers`. A referent must identify the actual entity,
   not just a shared name. Preserve event identity, ownership periods, conditions,
   negation and uncertainty. Unknown assertions remain separate.
3. Run `kmp-mcp consolidation write write.json`. Source admission and view CAS
   share one transaction. A source that moved or a competing view revision
   causes a conflict. Refresh the sources and review the proposal; do not swap
   stamps into old interpretations automatically. An exact retry returns the
   original compact receipt (revision, counts and exact `expansion` read);
   acceptance is not a fresh read. Use that expansion to inspect the full audit.
4. Run `kmp-mcp consolidation project request.json`, with
   `{"about":"project:sample","view":"owners","max_bytes":10000}`.
   Only `current` describes unchanged dependencies. `stale` carries changed
   refs and no derived claims: return to the original sources and refresh only
   the affected view using its previous revision as `expect_revision`.
5. The projection's `expansion` is the complete JSON input for
   `kmp-mcp consolidation read`. It returns the immutable revision with source
   bodies, quotes, why and direct relations. `historical_audit` is a saved
   derivation, never a certification that its sources remain current.

For a source-version clock filter on a current projection, supply
`selection:{"axis":"observed","as_of":"2026-09-15T00:00:00Z"}`. The other
axes are `occurred`, `ingested` and `validity`. All declared dependencies,
including ungrouped sources, need the
selected stored clock, every coordinate must pass, and the view must already
have been authored by that cut. Validity uses inclusive start/exclusive end.
No missing event time is inferred from observation or view authorship. A filter
does not reconstruct earlier source versions: use native temporal navigation
for that. `read` accepts an explicit revision for audit and rejects `as_of`;
`project` rejects a revision, intervals and dimension filters. An audit expansion
is not a time-filtered proof packet.

`omitted_claims` counts complete groups left out by the clock or byte limit.
Source refs and explicit lifecycle links remain associated with each admitted
claim; a count of supporting reports never establishes independent corroboration.
Budget fractions change delivery, not whether an omitted statement is false.
Compare full preparation, write, read and expansion costs before claiming savings.

Views and their retry receipts persist in separate SQLite tables. They preserve
all canonical records and view revisions, but are not part of the canonical
event-log export/import bundle. Back up the SQLite store to retain these optional
views; an event-only restoration retains original memory and needs views authored
again. Read captures are bounded to 64 sources, 256 direct relations per source,
1 MiB per node/body record and 8 MiB aggregate source-capture input. Oversized
captures fail explicitly; these limits are not process-RAM guarantees.
