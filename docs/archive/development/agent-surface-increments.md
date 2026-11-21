# Agent surface: increment notes (archived)

These are dated maintenance notes, one per increment of the Trace and read
surface (September 2026, mostly under #538 and #544). They were part of
[the agent surface procedure](../../development/agent-surface.md) until 1.0
and were moved here, translated where they were in Spanish. They are records,
not instructions: names, arguments and numbers held on the day each was
written. Check anything you reuse against the current code, the guide cards
and the live `tools/list`.

## Trace candidate routes

Candidate discovery lives in domain (`bounded_trace_search` and
`TraceCandidateFrontier`); storage admission stays in `TraceTemporalAdmission`
and `TraceReadBudget`. Change both protobuf copies, proto request/result mapping,
MCP request parser, trace projection and tool schema together. `follow` replaces
global filters; `paths_per_target` changes the route quota, not proof truth.
Preserve arrows and shared global indexes across pages. Count rejected cycle
attempts in max_states, cache only completely expanded adjacency, and retain
early routes at a later work cutoff. Test mixed direction, reconvergence, hubs,
deep paths, temporal exclusions, quotas, zero hops and cursor reconstruction.
See [candidate discovery design](trace-alternatives.md). Guidance and cost
accounting remain documentation, with no new CI gate.

## Trace material selection

Native `select_trace_material` selects over the bounded candidate catalogue;
`TraceMaterialSelection` validates explicit caller requirements. Change the two
protobufs, request maps, route/clock reindexing, full-candidate cursor digest,
MCP parser/schema/projection and optional expansion action together. The expansion
belongs in `search.material.expand_candidates`, never mandatory `next_actions`.
It must preserve all selection arguments except select and the old page cursor.

Update the audit card, audit verb and bounded-trace example together, then
regenerate the installed guides and surface fixtures. Verify AND/OR, shared
material, zero benefit, infeasible requirements, hidden-candidate changes and
pagination. Measure the entire response and any optional expansion separately;
material nodes are not tokens. Keep calibration frozen for comparisons and
retain failed runs. See [mathematical policy](trace-material-selection.md).
These maintenance instructions add no editorial CI gate.

## Trace dimensional routing

Reuse DimensionSelection/LabelSelector/EntryLabels and their parser. Hard
search.dimensions and soft search.prefer_dimensions are separate choices; do
not silently turn the latter into admission. Trace reads memberships only on
the selected clock inside the same bounded ReadTx. Keep scope current_about.

Maintain both protobuf copies, request maps, queue scheduling, stats projection,
material-selection reindexing and cursor binding together. Explain preference
counts as observations, not truth. Unconfigured calls keep their prior output
and BFS cost. Update audit card, extended verb and bounded-trace example, then
regenerate. Validate a useful hint, a misleading hint with a bridge, temporal
multivalue/negative selectors, hard exclusion, dense fanout, stale labels and
full response costs. Do not tune the fixed 3:1 schedule on those controls.
See [policy and known limits](trace-dimensional-routing.md). No editorial gate.

## Resumable focused Trace exploration

When changing discovery scheduling, keep the announced routing order, counters,
extended audit guide and bounded-trace example consistent. A resumed expansion
turn is not a new path state: S counts eligible extension attempts; N/E count
actual references/rows. Report coordinate work and empty storage end probes.
Share partial adjacency between alternatives in one ReadTx, retain per-path
offsets, and give resumed queue entries fresh tickets so stale heap/FIFO entries
cannot consume them twice. Leaf claims require full admitted adjacency exhaustion.
Compare exact default BFS responses, late edges, misleading bridges, reconvergence,
mixed directions/clocks and depth/work cuts. Freeze the page quantum before
measurement; document its tradeoffs without a new size or editorial CI gate.
See [policy and controls](trace-page-exploration.md).

## Seed-based joint evidence paths

`search.seek` compiles native relations and witness constraints in proto mapping.
Keep both protobuf copies, MCP parser/schema, embedded/gRPC dispatch, result
projection, cursor fingerprint and three-section pagination together. No caller
variables or destination gold refs. Every role has one main witness; via/after
retain its position. Shared labels are joint intersections, same_ref is exact
reference equality, and missing labels/clocks never become known by joining.
Do not add private witness bindings to the solver merely for display: that would
change ambiguity. Witness refs are projected from the validated seek request.

Update the audit card, extended verb, evidence-seek lesson and typed error help;
regenerate guide assets and review both tool catalog fixtures. Validate incoming
arrows, deep/unlabelled bridges, incompatible/unknown groups, hidden-group cursor
changes, all page sections, work cuts and old Trace. Context review is optional,
separate from continuations and only offered at an equivalent explicit cut.
Measure complete native calls against frozen controls; do not claim NLP or agent
understanding from a deterministic compiler replay. These are maintenance
instructions, not new editorial CI gates.

For contextual seek, keep `via:"context"`, typed `context`, `context_discovery`
and `context_hops` consistent through the same boundaries. The witness offset
comes from the candidate's actual prefix depth. Include discovery mode and
prefix lengths in the fingerprint. Document minimum-hop selection and retained
ties, temporal/ownership/proof admission, explicit obligation input and required
semantic review. Verify unknown/future links, mixed directions, joint conflicts,
cycles, deep chains, all pages and embedded/gRPC parity. A discovered contextual
prefix does not entail a claim about the seed. Compression is a later optional
reader projection; add no writer fields or editorial CI gate here.

For relation endpoint equality, maintain TraceReferenceEndpoint in both protobufs,
JSON same_ref members, zero-position contextual capture, candidate.anchor, binding
endpoints and their whole-selection fingerprint together. Anchor means traversal
start of the main relation, after via/context; it is the stored target when the
move is incoming. Preserve role-string witness shorthand. Groups partition
endpoints so one role can join an action and a person separately. Test different
actions with the same witnesses/labels, convergence, fixed prefixes, unknowns,
disjoint endpoint validation, paging and real gRPC. Update the reduced audit card,
extended audit, human guide and evidence-seek example. Contextual compatibility
remains a lead requiring original source review, not logical entailment.

## Trace: keeping the modes apart

When changing Trace options, assign each one to known destinations or to
evidence from a seed. Keep the `oneOf` and the allowed fields in the schema,
and the grouped refusal before interpreting incompatible bodies; do not add a
redundant selector. Shared definitions stay unique, to keep the recursive
validation of unknown fields. Verify both valid families and the repair of
mixes with `to`, destination options and common limits.

Update the card, the extended verb, the human guide and the examples in the
same change. Contrast the same action against a different copy/publication,
and write the explicit clock in historical examples. Compare complete valid
results and measure the schema, the card and the refusals, without inferring
agent understanding from the replay.
[Design and acceptance](trace-search-modes.md); informative instructions, with
no extra editorial CI.

## Main notice of an incomplete read

`tool_result::packet_is_partial` derives the delivery state from
page.has_more, projection.page.has_more and projection.core_text_shortened. The
first text of partial reads and the optional help use that same function. Do
not confuse it with selection.has_more, search limits or the writer's prior
neighbourhood. Keep the structured object and its budgets intact: the notice
belongs to the envelope, not to the source. Test a real continuation without
context, with context, a complete UNKNOWN, a shortened core and another
historical position after a complete page. Update the entry, the temporal
card, the verb and the budget example; measure the whole envelope as well as
the body. [Design](read-progress.md), no editorial CI.

## Joint Trace proof materialization

`search.proof` is shared by destination and seed modes. Keep both protobufs,
request mappings, shared object/support/gap projection and whole-selection
fingerprint together. Page order must agree between kernel slices and MCP byte
projection. Preserve source provenance, attachment admission, missing bodies,
unknown proof and contextual review state. A body change outside the current
page still invalidates the cursor. Do not count existing snapshots/source batches
as a new improvement. Validate both modes, shared/foreign/missing sources, clocks,
N/E cutoffs, concurrent revisions and transport parity before paired timings.
`budget.max_bytes` is a response ceiling, not an allocation guarantee. Update
audit, bounded-trace/evidence-seek lessons and both generated guides together.
See [contract and limits](joint-evidence-materialization.md).

## Reading, expanding and cards: checking the journey

Compact proof reads carry `proof.condense_candidates`: a bounded list of
identities and costs, computed in domain over the whole selected table before
paging. `shared_by` counts returned routes or seek groups with known clocks and
all their binding witnesses, including their admitted supports, once per
route/group. Groups pending review add no sharing. Delivery of canonical
evidence is independent of the count, which does not verify meaning either.
The selection fingerprint includes the list; the body manifest keeps its
identity. Keep the policy, both protobufs, the mappers and the schema next to
the Condense guides. The reader copies `source` and `expect`, reads the body
and writes its card.

Check priority, tie-break, quota, floor, valid cards and cards later than the
cut, the compare-and-set of stale cards, source changes and pages with no
objects of the list. Compare the bytes of the list in embedded and gRPC.
Measure the whole journey: repeating a recommendation on every page adds cost.
The reproducible trial and its limits are described in
[condense-candidates.md](condense-candidates.md); it does not imply a token
saving or reader learning, and adds no editorial gates.

Keep Trace, Condense, the `audit`/`condense` cards, the verbs, the
`reader-cards` example and the refusal help aligned. The audit verb serves as
a map; navigation detail is looked up in `advanced:audit-reference`. Do not
load both by default. Keep refs when moving prose and review the regenerated
links.

The descriptor for Condense comes from Trace with `proof:true` and a body
option active, for example `proof_refs:[]`; Inspect does not expose that
digest. The reader must read the matching canonical body before writing. The
step to compact is a new read: keep the query and clocks, remove
`proof_refs`, `expect_selection` and the old cursor. A missing or stale card
does not trigger a canonical fallback. Keep apart the end of the pages, the
inventory of refs read and semantic sufficiency; an empty expansion queue does
not prove that earlier refs left out by manual batches were read.

Before checking learning with agents, seed the guide and the case in an
isolated store; verify registration, card, verb and example with a different
preparation identity. Freeze the binary, the asset and the hashes.
`GUIDE_UNAVAILABLE` invalidates a guided usage trial, although it does document
the response to the schema. The driver runs the calls the reader chose,
without following actions or quietly correcting arguments. Keep errors and
recovery. Measure text, MCP envelope and catalogue separately with an
identified tokenizer; bytes or words are not presented as tokens, nor a local
capture as the cost of the whole host.

When publishing, check the generated package and its `tools/list`, as well as
the code. The candidate binary's tool does not appear automatically in a
session that keeps an earlier registration: document the installed version,
the update path and the restart needed. Do not claim activation because it
compiled. These checks are delivery evidence, with no new editorial gates.
