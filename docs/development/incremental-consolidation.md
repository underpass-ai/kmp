# Incremental consolidated views (#540)

Repeated reports now have an optional native view distinct from canonical memory.
A writer captures source versions, supplies source-backed structured claims, and
writes one named view. Equal declared referent, predicate, value, temporal scope,
polarity, epistemic status and qualifiers share a claim group. Different values,
people, periods, negations and qualifiers remain separate. Unknown interpretations
are never grouped. Quotes are checked byte-for-byte; semantic fidelity remains the
writer's responsibility. No embedding, language model, identity inference or new
relation type is added to the kernel.

## Contract and ownership

The domain owns admission, grouping and source-clock filtering. The proto-mapping
adapter owns JSON projection and byte budgeting.
The application exposes `ConsolidationApplicationService`; the object-safe
`ConsolidationStore` port is optional. SQLite captures source bodies, node
properties/provenance and both directions of direct relations in a single
transaction. A SHA-256 stamp binds these together with the exact body descriptor.
The write transaction re-captures dependencies, checks all stamps and the view
revision, inserts an immutable revision and retry receipt, then advances the head.
A refused write commits nothing. A repeated logical command returns its original
acceptance even after the head moves; a fresh read checks current dependencies.

The CLI is `kmp-mcp consolidation sources|write|read|project [FILE|-]`. JSON and
file handling belong at this boundary, not in storage policy. The workflow and
field semantics are in the installed guide's `advanced:consolidation` entry.
No new MCP/gRPC operation is advertised. Existing canonical ingest and lossless
presentation deduplication (#537) are unchanged: they cannot by themselves provide
atomic dependency validation or immutable independently addressed view revisions.

## Incremental behavior

A write refreshes one explicit view, not every view in the store. Current reads
validate only that view's bounded source set; unrelated writes do not invalidate
it. A source body changing under the same public revision/hash still invalidates
the view. Adding, removing or changing a directly incident relation also changes
the stamp. Failure to recapture a dependency makes the view stale, with no current
derived prose. Infrastructure errors propagate. The writer must re-read changed
sources and regenerate their interpretations; the kernel cannot re-author meaning.

This is bounded lazy invalidation, not a background model job or a full-store scan
after each write. Undeclared semantic dependencies are not discovered automatically.
Sources, including dependency records with no grouped assertion, remain in the
immutable audit. All original sources and prior view revisions remain unchanged.
Direct lifecycle relations preserve corrections, supersession and contradiction;
they do not automatically turn repeated reports into independent confirmation.

## Time and budgets

Current projection can filter its selected source versions by occurred, observed,
ingested or validity clocks. It requires every declared dependency and its
relevant stored coordinates to pass;
unknown clocks are not inferred. Cutoff equality is admitted, validity end equality
is excluded, and a view authored after the cut contributes no claims. No interval,
dimension-filter or historical source-version reconstruction is silently emulated.
An explicit revision read is labelled `historical_audit`; it gives the saved
sources and actual clocks, not current advice. Native temporal navigation remains
the route for reconstructing canonical event/observation/ingestion/validity history.

Projection admits whole groups in deterministic order under a JSON byte ceiling.
Its exact revision expansion recovers the original evidence. It reports omitted
groups instead of pretending the context is complete. The metadata floor is an
explicit error when it cannot fit. Byte counts are not token or RAM measurements.
Capture limits are 64 refs, 256 incident relations/ref, 1 MiB/node or detail record,
8 MiB aggregate capture input; CLI writes additionally bound input to 2 MiB.

## Durability boundary

Three additive SQLite tables contain heads, immutable views and retry receipts.
They survive restart and projection rebuilds. Like reader cards, these optional
views are outside the canonical event-log bundle; an event-only export/import
retains canonical memory but does not transfer consolidated views. A full SQLite
backup retains them. This limitation is visible in the guide, not an implicit
promise of bundle portability.

## Verification and evaluation

Behavioral controls cover paraphrases with distinct quoted sources; homonyms;
ownership periods; changed values; negations; tentative and unknown claims;
qualifiers; changed bodies with unchanged public version/hash; direct relation
changes; unrelated writes; CAS races between real sessions; failed writes;
idempotent restart/retry; immutable history; and all four clock boundaries.
CLI tests execute capture → write → compact projection → immutable expansion,
then mutation → stale result against a real store. Invalid boundary requests do
not open a store.

The [development comparison](incremental-consolidation-evaluation.md) records
source-only, formed, presentation-deduplicated and consolidated readers at
100%, 75% and 50% budgets, whole CLI costs, a failed writer interpretation and
a short-source control where consolidation costs more context. It separates
packet grounding from fidelity to original prose and from complete fact recovery.
The native contract does not establish semantic correctness; independent writer
evaluation on held-out histories and provider-billed end-to-end costs remain
unmeasured. These are limits of the evidence, not guarantees inferred from byte
savings or deterministic grouping.
