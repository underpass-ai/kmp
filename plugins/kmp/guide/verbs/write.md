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

Use `connect_to.ref: "source"` or `"@source"` to address exact local id `source`,
even if it appears later. No fuzzy matching or target creation occurs. An unknown
local name returns `UNKNOWN_LOCAL_REF` with declared ids in `allowed_values`.
Local targets count as `current_request` context. Rich links, including local
links, receive server neighborhood review before commit; cross-about
identity links retain the proposal rule. `read_context` is a caller-declared audit,
not a substitute for the served neighborhood. Do not claim to have inspected a new
local target. Independent facts may be unlinked: do not invent a relation.

The containing memory is the source of each link; `connect_to.ref` is its
target. Read the proposed triple aloud: approval **authorizes** action, outcome
**verified_by** check, exclusion **excluded_from** total. A relation name alone
does not describe its direction. See `guide:kmp-agent:advanced:relations` for
endpoint examples and partial corrections. `supersedes` marks the **whole**
target SUPERSEDED. If a report holds independent facts, isolate the relevant
fact or use `corrects` with the precise correction; preserve the other facts.

Omit `class` when `rel` has exactly one allowed class. For example,
`{"ref":"check","rel":"verified_by","why":"H verifies the deployed version.","evidence":"H reports the expected version."}`
compiles to `evidential`. An explicit class is checked, never overwritten.
`authorizes` and `chosen_because` require a choice of `motivational` or `causal`;
`confirms_selection` requires `evidential` or `motivational`. Decide from the
source. The canonical receipt and relation-quality detail retain the resolved
class. This completion changes neither proof requirements nor the relation's
direction and does not infer whether it is true.

With `context_id`, actor defaults to the persistent agent name; an explicit
actor overrides it. Without context, actor is required. Actor identifies the
author. Omit `observed_at` or set it to null: KMP assigns the exact ingestion
instant to this new observation and the packet's provenance. Supply an explicit
observation time only when the source establishes it, with the actual UTC offset.
KMP never chooses it from event dates or from a different record.

Root observed/occurred/valid clocks are defaults inherited by records. Each
record may override them. A record's `observed_at:null` clears the root default
and uses ingestion; `occurred_at:null` clears the root event date and remains
unknown. Without a root event date, omitted occurrence also stays unknown.
Ingestion is assigned by the kernel; the semantic writer cannot supply it.
Equal observed and occurred times are valid when the source supports both.
Several records may share an observation time without sharing an occurrence.
Each `connect_to` declaration also gets its own observed and ingested clocks.
Its observation comes from the packet, or exact ingestion if omitted; neither
endpoint's per-record dates can backdate the link. Its occurrence and validity
remain unknown. No extra writer arguments are needed. Separate execution and
verification records still carry their own event times.
If one report says an action ran at 11:00 and its check passed at 11:05, keep
the action and check in separate records with event-specific summaries and
evidence fragments. Do not put the later verified result into the earlier
action text. A relation does not move the check backward in time. See
`guide:kmp-agent:example:event-separation` for the native cutoff and viewer example.
Updating `search_summaries` preserves the original coordinates, including unknown
historical observation; only the new packet provenance receives a default date. KMP
resolves all names and validates the entire packet before one canonical ingest.
A rejected record writes none of the packet. The transaction covers one about.
Omit unknown validity boundaries too. Observing an existing state does not
establish when it began; a later report does not give an earlier writer knowledge
of when it will end.

`local_refs` maps your ids to canonical refs. A preview plans those refs;
`accepted=true` confirms persistence. Retry the unchanged logical packet with
the same idempotency key. Duplicate ids, duplicate targets, self-links and
undeclared local targets are errors. See
`guide:kmp-agent:example:semantic-batch` for a minimal packet, a forward proof
link, a rejected packet and temporal/ChronoLoom review. Accepted writes return a compact acknowledgment; recover the detailed receipt only when needed.

## Read the acknowledgment

`clock_defaults`, when present, reports `observed_at:"ingested_at"`, the count
of affected memories and whether packet provenance defaulted. It describes the
plan; `clocks` and the canonical receipt carry actual accepted values. A preview
does not reserve a timestamp. An unchanged replay returns the original clocks,
including after restart or import, without creating another observation.

`accepted=true` confirms the submitted packet was committed. Read `warnings`,
`diagnostics`, `feedback`, `labels.created` and `labels.resembling`; they can qualify success.
A lax write can carry `RELATION_CONTEXT_UNVERIFIED` with severity warning and
an audit action. Acceptance does not establish an unverified causal claim.
`relations` lists compact `{from, rel, to}` triples in canonical ingest order.
Endpoints from this packet use `@id`, resolved by `local_refs`; stored endpoints
keep their canonical refs. Read their direction and scope against the source.
The acknowledgment exposes what was compiled; it cannot certify that an
approval verifies execution or that a correction replaces an entire report.
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
`RELATION_CLASS_REQUIRED` means this relation has more than one possible class;
`RELATION_CLASS_MISMATCH` means the explicit choice is incompatible. Both identify
the field and include `allowed_values`. Do not invent evidence or weaken a
relation just to make validation pass. Missing class is different from null or
an empty string; only omission requests deterministic completion.

For `needs_review`, nothing has been written. Review `neighborhood` and the
proposed `relations`; use each stored item's Inspect action to expand its text,
links and proof. `stored` and `proposed` are distinct. Long text is omitted
whole instead of clipped before a negation or condition. Unknown clocks remain
unknown. `links.from` and `links.to` index stored items in this packet only.
`partial`, `omitted` and `omitted_conflicts` disclose excluded items; the last
counts omitted facts participating in explicit conflicts, not unique disputes.
`expand_context` offers full-detail reads in the explicitly consulted abouts.
Those reads may include broader context and pages; finish relevant pages.

After reviewing, execute `next_actions[0]` to resume the unchanged packet, or
correct it and submit again. With `context_id` the existing continuation store
retains the packet; otherwise the action carries its full arguments and
`review_token`. Do not add arguments to a continuation. Tokens bind the exact
logical proposal and complete selected source material, including omissions.
A changed proposal or relevant context returns a fresh review without writing.
A race at commit is safely retryable under the same idempotency key. An accepted
retry returns its original receipt even if memory has changed afterwards.

The rule applies to rich relations in the kernel vocabulary (and unknown
non-strict relations), including same-batch links and `strict:false`. Simple
observations, structural links and honest `follows`/`answers`/`uses_background`
links need no review round. A prior declaration of read refs cannot bypass it.
This is the writer's review, not a new human approval step. A token records
acknowledgement of delivered context, never understanding or semantic truth.

Selection uses explicit endpoints, their direct links and shared key/value
labels in the current about. Explicit conflict facts and constraints precede
recent facts; old constraints retain their validity clocks rather than being
silently declared current. Foreign scope is limited to explicitly proposed
identity endpoints and their surroundings. It never scans all abouts. The
initial target is five items in roughly 2 KiB before MCP action metadata;
a large indivisible item may exceed that target. No model summarizes the text.
Local reads, serialization, agent context and another interaction still cost.

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
for search: `memories[].summary_en`, plain English a reader would ask with,
identifiers, amounts and acronyms kept exactly as written (`v0.7.0`, `#469`,
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

For normal writing omit `options.dry_run` or set it to false. Rich links first
return `needs_review`; resume after reviewing. Independent observations stay one call.
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
normal writing already validates and requests neighborhood review where required. Raw `kmp_ingest`
with `dry_run=true` also reaches the kernel and leaves memory uncommitted.

## Restricted HTTP clients

An HTTP bearer token may restrict abouts, label values and external refs. Rich-link
review also requires `kmp:read` and an about grant for every external endpoint,
including when resumed from a continuation. Those
grants apply to shared and per-record labels, each array member, explicit entry
refs, relation targets and search-summary refs. Relabel additions obey the same
label grants. A local @id is resolved inside its packet; it does not name an
external memory. A forbidden request must stop before any backend call. Do not
remove source-supported labels or refs just to evade a permission error.

### Read the outcome and stored clocks

`status=committed` appended the logical command; `replayed` returns its earlier
acceptance without another write. Both keep `accepted=true`. `validated` is a
dry-run, `needs_review` is an unapplied context review, and `rejected` is a validation refusal. `unconfirmed` cannot establish
persistence: retain the same idempotency key when resolving a transport error.
Do not turn an uncertain reply into a new logical write.

`clocks.scope=accepted_command` summarizes the clocks actually in that command,
including on replay. Each clock counts memories once across all label memberships:
`entries`, `distinct_values`, and an RFC3339 `single_value` when there is only one.
`clocks.relations` separately counts semantic links and how many carry each
clock; structural label memberships are excluded. For one ordinary link,
`relations=1, observed=1, ingested=1, occurred=0` is normal. Read responses expose
its own dates in `relation.clocks`; they describe the stored declaration, not
when its endpoints occurred or when the claim became true. An unchanged retry
preserves these dates. Old undated links are not assigned dates on read.

Zero occurred entries means no event time was recorded. One observed instant
across nine memories is worth comparing with the nine sources; KMP cannot decide
whether those sources were observed together. Missing occurrence is legitimate
when its source does not supply one. Do not invent clocks to fill a count.

Example: two memories with four label memberships can report two observations
at one instant, one occurrence, and one valid_from. Inspect receipt.action only
when the individual historical clocks or evidence are needed. Its receipt retains
those clocks even after a later write, restart or bundle import. The current graph
can have changed. Null clock coverage is unavailable, not zero; previews do not
report saved clocks. Packet coverage still cannot assess source completeness.

### Calendar dates in search renderings

`El 17 de agosto de 2026 se abrió la ruta.` may use
`The route opened on 2026-08-17.` as summary_en in strict mode. The lint compares
complete Spanish or English month-name dates with ISO YYYY-MM-DD dates, including
the month and valid Gregorian day. It leaves both stored strings unchanged.
English `August 17, 2026` and `17 August 2026` are equivalent forms here.

For `El parte llegó el 18 de agosto.`, use `The report arrived on August 18.`
when the year is unknown. A full rendering such as `2026-08-18` preserves the
known month/day too, but this lint neither supplies nor verifies its extra year:
the writer must justify it from evidence. A complete source date must keep its
year. A missing partial date is reported as `--08-18`; repair its month/day,
not by adding a redundant bare `18`. Changed months or days still fail.

Other formats, times, amounts, versions and acronyms remain literal. An amount
of 17 EUR still needs its own 17; the day in a date does not preserve that amount.
This check does not prove the meaning of the rest of the summary. A changed or
omitted recognized date is reported as its canonical ISO identifier.
