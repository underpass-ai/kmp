## A semantic packet

Use `memories` for one or more source-backed records in one about. Give each a
short local `id`, `kind`, `summary`, `evidence`, and its label arrays. A single
record is a one-element packet. No `intent`, `current` or process `scope` is
needed in this form. The former current/intent/scope input is rejected; there is no compatibility adapter.

Top-level labels are shared memberships, unioned with each record's labels.
Every record must have at least one membership; none is invented. Repeating a
pair in both shared and record labels is harmless; duplicates within either
array are rejected. Every supplied label is materialized before commit. Put only memberships that
apply to every record at the top level. If S1 supports one memory and S2 another,
put `source: ["S1"]` and `source: ["S2"]` on those respective memories. Shared
`source: ["S1", "S2"]` attaches both to both. Aliases mentioned only in prose do
not create alias memberships; declare each source-supported alias explicitly.
A source label helps navigation but never replaces its concrete evidence text.

Use `connect_to.ref: "@source"` to address local id `source`, even if it appears
later. Local targets count as `current_request` context. Stored rich targets
still require an actual prior read declared in `read_context`; cross-about
identity links retain the proposal rule. Do not claim to have inspected a new
local target. Independent facts may be unlinked: do not invent a relation.

Actor identifies the author; top-level observed_at is the packet's provenance
observation time. Actual ingestion is recorded separately by the kernel. Each
record may override its observed/occurred/valid clocks; omitted occurrence remains unknown. KMP
resolves all names and validates the entire packet before one canonical ingest.
A rejected record writes none of the packet. The transaction covers one about.

`local_refs` maps your ids to canonical refs. A preview plans those refs;
`accepted=true` confirms persistence. Retry the unchanged logical packet with
the same idempotency key. Duplicate ids, duplicate targets, self-links and
undeclared local targets are errors. See
`guide:kmp-agent:example:semantic-batch` for a minimal packet, a forward proof
link, a rejected packet and temporal/ChronoLoom review. Accepted writes return a compact acknowledgment; recover the detailed receipt only when needed.

## Read the acknowledgment

`accepted=true` confirms the submitted packet was committed. Read `warnings`,
`diagnostics`, `feedback`, `labels.created` and `labels.resembling`; they can qualify success.
A lax write can carry `RELATION_CONTEXT_UNVERIFIED` with severity warning and
an audit action. Acceptance does not establish an unverified causal claim.
`coverage` counts declared memories, relations, evidence objects and per-memory
label memberships after the shared-label union. `complete=true` applies only to
the submitted packet. `source_coverage=not_assessed` means KMP cannot tell whether
the writer omitted a source fact, alias or date. For `search_summaries`, the
coverage separates updated renderings from preserved memberships.

The accepted response omits repeated canonical data and per-relation diagnostics.
`receipt.ref` identifies immutable command detail. Execute `receipt.action`
verbatim when you need to audit the accepted write; it supplies `kmp_inspect`
with the about, ref and include options. No routine readback is required.
For example, after a packet with local ids `source` and `decision`, the action's
`object.text` is JSON: `receipt.writer.local_refs` maps both ids, and
`receipt.canonical_memory` preserves their actual coordinates, assigned sequences,
source evidence and relation proof. `receipt.writer.relation_quality` retains the
compiler's per-link checks. `command` gives the accepted revision and content hash.

This is the original accepted snapshot. A later relabel or search rendering does
not rewrite it; inspect the memory's own ref for its current state. Receipts survive
restart and bundle export/import and are not graph memories, label matches or
Wake evidence. Previews have no durable receipt and retain `ingest_preview` for
review. A simulated backend does not prove receipt persistence. Full receipt
inspection costs more context than the acknowledgment; the ordinary Inspect
stable-object floor and its warnings apply.

## Repair a refused packet

A refusal carries `feedback`: a stable `code`, `severity`, `field`, `reason`
and an optional `action` with `tool` and complete `arguments`. Use the field
path to locate the problem; do not parse the prose to choose a repair.
Read the whole feedback array. Once the packet shape and local identities are
usable, KMP reports the first compiler failure from each invalid record together,
for both memories and search_summaries. Correct all listed fields and resubmit
the whole packet. This is not an exhaustive source audit: another rule in the
same record may fail after its first error is repaired. An unusable packet shape,
ambiguous local id or failed store read can still stop the check earlier.
For example, `MEMORY_EVIDENCE_REQUIRED` at `memories[1].evidence` means the
second record needs its real source. `SEARCH_SUMMARY_REQUIRED` at
`memories[1].summary_en` asks for a faithful English search rendering.
`INVALID_KIND` includes `allowed_values` from the same vocabulary as the schema.
Choose the kind from the source's meaning; the server supplies no replacement.
`INVALID_TYPE` reports the exact field with `expected_type` and `received_type`.
For example, `memories: "[]"` is a string: send a JSON array of record objects.
KMP does not parse a string into memories. A non-object array member reports
`memories[index]`, expecting `object`. `EMPTY_MEMORIES` means the array is present
but contains no records. Omitting both `memories` and `search_summaries` returns
`WRITE_OPERATION_REQUIRED`; other absent required fields use `REQUIRED_FIELD`.
For example, an unapproved request may be recorded as an observation of the
request, without `updates_state` or `supersedes` on the current decision.
A later approval needs its own evidence. `request` and `outcome` are not kinds.
See the semantic-batch example for individual source labels and this distinction.
`RELATION_CLASS_MISMATCH` identifies the link's class. Do not invent evidence
or weaken a relation just to make validation pass.

For `PRIOR_CONTEXT_REQUIRED`, KMP can supply a complete `kmp_inspect` action
for the existing target. Execute the read, evaluate its evidence, then declare
the actual returned ref in `read_context` and resubmit the corrected packet.
The action is a supported reading move, not proof that the relation is true.
An absent action means no source-backed automatic correction is known.
Planner refusals commit none of the packet; store failures still carry their
backend error. A failed pre-read retains unavailable, not_found, conflict or
backend_error; do not treat it as a malformed source packet. Malformed successful
store data is a backend_error. A normal accepted write does not require routine readback.

## What to write, and what never to write

Write when something is **decided, constrained, or concluded**. Decisions,
constraints, outcomes — each with coordinates and evidence.

Never write transcripts. Memory is not a log of the conversation; it is the
durable shape of the work. A transcript makes later traversal worthless.

Label what you write. Every label is a `(key, value)` membership within its
about. Use keys such as agentic_process, task and agentic_episode when the source
supports those memberships. `labels` uses arrays,
even for a single value, and may add further values under any key:

```json
"labels": {
  "alias": ["neb", "Nébula Cache"],
  "component": ["neb"],
  "environment": ["production"]
}
```

Each pair becomes its own coordinate with the memory's clocks. Two aliases
under `alias` are valid, as is `neb` under both `alias` and `component`.
The key distinguishes them. Repeating a value inside an array is an error. Shared and record labels are unioned. Values may contain spaces, punctuation and
Unicode. The server returns canonical refs: copy them when an exact graph
address is needed. Do not construct refs or infer entity identity from labels.

Read the `labels` catalogue in `kmp_wake` to reuse existing vocabulary.
Responses report labels written and created. A new spelling under the same
key, such as `component=kmp_viewer` when `component=kmp-viewer` exists, is
refused in strict mode or reported in `labels.resembling` in lax mode.
Sharing a value across different keys is valid and produces no resemblance
warning. `options.labels_new: ["component"]` confirms intentional new values
under that key; nothing is silently renamed or merged.

Use `kmp_relabel` to change memberships later. Its `add` and `remove` use the
same arrays. For example, `add: {"alias": ["Nébula Cache", "NC"]}` adds two
aliases, and `remove: {"alias": ["NC"]}` later removes only that membership.
Give the memory's returned `ref` and a source-backed `why`. The kernel refuses
adding an existing pair, removing an absent pair, placing a pair on both
sides, and removing the last label. Other values under the key remain.
Added memberships inherit the memory's clocks; the relabel's actor, receipt
time and rationale are recorded separately. Read back with `kmp_inspect`
and `include.raw` to verify the memberships and evidence.

Choose keys for the routes you will need: project/source alone cannot select a
person, account or component. Reuse the same key for the same route over time;
`alias: ["@north"]` and `account: ["@north"]` are distinct selectors. Separate
facts when one can expire or be replaced without changing the others. A person's
role, an account assignment and a permanent constraint need not share a lifecycle.
See `guide:kmp-agent:example:alias-ownership` for separate assignments and
`guide:kmp-agent:example:semantic-batch` for a source-to-memory self-check.

Filter a facet with `dimensions.selectors`, for example
`{"key":"alias","op":"in","values":["neb"]}`. To require both an alias and
an environment, supply one selector for each key. Shared labels locate
candidates; only justified relations declare that two memories concern the
same entity or event.

Write in the language of the work, and give the memory an English rendering
for search: `memories[].summary_en`, plain English a reader would ask with, every
number, identifier and acronym kept exactly as written (`v0.7.0`, `#469`,
`kmp-mcp`, `ADR`). `kmp_ask` searches it and never cites it — the citation is
`memories[].summary`, byte for byte — so an English question reaches a memory
written in Spanish, and jargon (`rollout slipped`) is found by the words
people ask with (`launch postponed`). A strict write requires it when the
memory is not written in English and refuses one that fails the lint: wrong
language, too thin, a copy of the text, a dropped identifier. Fix the summary
the error names; never alter the text to fit it. A cited item that was reached
through its summary says so with `matched_via: summary` and `summary_terms`.

Memories written before summaries existed still owe one. `kmp-mcp summaries
pending [<about>] [--json]` lists them, with the text to render and, for a
summary the lint refuses, what is wrong with it; `kmp-mcp doctor` counts them.
Attach one or more with `kmp_write_memory`, using `search_summaries` records
containing only `ref` and `summary_en`: the text, kind and coordinates are read from
the store and cannot be supplied, no relation is written, and a rendering
that fails the lint is refused with every fault named. Each target must belong to the declared about; all renderings are validated
before one commit. An exact retry is a no-op.

Normal writes are one call: omit `options.dry_run` or set it to false.
The relation guide documents explicit previews and validation before commit.

Use one `idempotency_key` per logical write. Replaying the same accepted write
returns its success rather than duplicating memory. A `conflict` can also mean
the store advanced before this attempt committed: follow the error message,
rebase, and retry the same logical write with the same key when it says the
conflict is retryable. A key already accepted with different content must not
be reused.

For a relation, read `guide:kmp-agent:advanced:relations`. For event and observation timestamps read `guide:kmp-agent:verb:time`; for English search summaries read `guide:kmp-agent:advanced:summary`. Read the lifecycle and cross-about topics when the write uses those features. These are exact refs in `guide:kmp-agent`; reuse bodies already in context.

## A preview checks the selected store

Use `options.dry_run=true` only when a preview is useful. Embedded and gRPC
previews run the same kernel validation as a commit, including existing refs,
label policy and coordinate ownership. A missing target or backend failure is
an error; a syntactically valid packet is not enough. A successful preview has
`accepted=false`, `dry_run=true` and `validation.scope=current_store`. It writes
no event. The fixture backend reports `scope=fixture` and proves no live state.

The returned `ingest_preview` is the proposed canonical packet. Previewing
reserves neither labels nor sequences; a later commit is validated again against
the then-current store. Do not repeat previews before every ordinary write:
the normal write already validates and commits in one call. Raw `kmp_ingest`
with `dry_run=true` also reaches the kernel and leaves memory uncommitted.
