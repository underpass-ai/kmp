# Native MCP simplification

Decision: 2026-09-09. Evolve the native MCP contract (solution A), preserving
evidence-backed writing and navigation through dimensions, time and relations.
SQL, GraphQL and a new query language are outside this implementation track.

The integration branch is `integration/mcp-simplification`, created from `main`
at `48bc5de36aaf83cb4b44096b0be69714d93220c9`. Feature branches start from integration
and their PRs explicitly target integration. The repository default stays `main`.
Use a separate integration-to-main PR when the combined change is ready.

## Delivery order

| Block | Scope | Acceptance |
| --- | --- | --- |
| 1. Dimensions | Identity by about/key/value; multiple values per key; ingest, relabel, selectors and projections | Same value under different keys coexists; multiple aliases work; results and aggregates do not duplicate memories; new refs identify the exact key/value pair; unsupported old formats are rejected |
| 2. Writing and feedback | Semantic batches, local refs, evidence, compact receipts and actionable errors | One declaration materializes labels/relations; invalid batches leave no partial writes; acceptance and required repairs are explicit |
| 3. Reading | Direct intervals, field projection, coverage and complete continuations | The reader reconstructs the selected evidence under small budgets without guessing filters, clocks or omitted detail |
| 4. Agent evaluation | Independent writer/reader using the revised guide and new sources | Measure omissions, false relations, navigation, total tokens and latency; keep human corrections visible |

## First block: dimensions

Treat `(about, key, value)` as the logical identity of a label. Distinguish this
identity from its physical reference and from an entity's identity. Two aliases
may help find one entity; the string alone does not prove equivalence.

Implement the whole path: write and relabel inputs, dimensional identity,
catalogue, selectors, sequences, export/import, backend mappings and ChronoLoom.
Do not merely remove the input validator while persisting ambiguous dimensions.
Keep the public representation small; resolve internal refs in the server.

The user explicitly removed backward compatibility from scope. Change the public
contract and storage format where needed; do not build legacy-ref resolution or
old-store migration layers. Reject old formats explicitly and use fresh stores
for verification. Test round trips of the new format. Adding labels later must
preserve the event clock and retain the provenance of the label change.

Acceptance examples include two aliases under one key, the same text under two
keys, intersecting selectors, relabel of one membership, a homonym in another
about, and a late label shown correctly in ChronoLoom. Counts refer to unique
memories; pagination and work limits report incomplete coverage explicitly.

## Working and verification

Use cohesive PRs with behavioral evidence appropriate to their changes. Bugs in
released behavior follow the normal route to main; integration-only bugs target
integration. Bring required main fixes into integration without rewriting its
shared history. Existing repository protections and workflows remain unchanged.

Follow [agent surface maintenance](agent-surface.md) for actual contract changes,
including guide examples and discoverable extended help. Editorial guidance is
informational. Do not add a documentation bounded context or editorial CI gates.

Agent evaluation follows source-only and human-review requirements of its run.
Completing a development block does not automatically start a paid model run or
the reader of an experiment awaiting review.

## Deferred second phase: recoverable context compression

After the four blocks above, evaluate compact context views whose omitted detail
remains recoverable through native memory refs. Keep source text, evidence,
relations, clocks and validity intact in storage. Compare compact native output
with the completed uncompressed contract using answer quality, proof retention,
whole-session tokens (including expansions) and total latency. This is a later
experiment, not a dependency on SuperCompress or part of this integration goal.

## Writing increments on integration

Store-validated previews are integrated in #580. The semantic packet increment
adds `memories`: one about, one canonical ingest, local ids resolved before any
member is committed. Shared labels union with per-record memberships; the kind
selects the ordinary operation. Independent source facts can be unlinked, while
all claimed rich relations retain proof requirements. Per-record observation,
occurrence and validity remain distinct from the packet's provenance and the
kernel's actual ingestion clock.

The unified public writer accepts `memories` or the separate `search_summaries`
operation. The former `current`/`intent`/`scope` fields are rejected; semantic deltas
are explicit packet members with justified relations. Compact recoverable
receipts, declared coverage and structured repair signals complete the native writer contract.
A native packet replay is not independent agent evaluation and does not resume a reader awaiting human review.

Writer validation now carries stable feedback codes and member/field paths.
A missing prior read can return an executable Inspect action with about and ref.
No message parsing or evidence synthesis chooses the code or action. Receipts
now return compact acceptance with coverage of the submitted packet. Full normalized
memory, proof, coordinates and diagnostics are stored in the accepted command and
recovered by the returned Inspect action, without adding graph memories. A preview
never creates a receipt; a lax accepted relation with unverified prior context
still warns. Receipt inspection is historical, not a current-memory read.

The writer acceptance checks exercise the returned action, restart, later search
summary changes, bundle import and the native/gRPC boundary. They do not establish
independent agent understanding. The requested Luna/Terra/Sol writer check follows
writer integration while the next block implements reading and continuations.

## Reading increment: bounded temporal response pages

The independent writer check exposed a stored packet whose complete proof kept
all three selected entries out of a 30,000-byte response. Temporal verbs now page
entries and proof as complete items. The reader appends each section and executes
the returned `next_actions` until `page.has_more` is false. `selection.has_more`
then distinguishes history outside the kernel's selected packet; its actions
navigate that history without guessing a new clock, selector or boundary.

The opaque response cursor binds the selected content and query arguments. A
changed selection returns a conflict and a fresh-read action. Byte allowance and
response item count may change; an indivisible item returns the minimum budget
and a retry that makes progress. Stable selection metadata can itself exceed an
unreasonably small byte budget, which remains explicit in the response.

This is an MCP projection over the same typed kernel selection, shared by embedded
and gRPC transports. It adds no reader session or stored pagination state; each
continuation recomputes the bounded selection. It does not promise a snapshot
across subsequent temporal moves or reduced total traversal tokens. Native tests
reconstruct complete sections and execute the actions, including transport parity.

Direct intervals are integrated in #600, including the shared relation-clock
fix #599; #602 synchronizes the canonical protobuf source after the delivery
omission. Forward/Rewind accept an interval without an initial cursor, include
all start ties, exclude the end and preserve earlier explanatory proof. An
open end explicitly leaves expiry unassessed.

Temporal entry projection adds `fields`, explicit included/omitted fields and an
executable scoped Goto action on each reduced entry. Identity always remains;
proof and raw audit selection remain separate. A page cursor binds hidden
content too. Expansions are fresh reads, not retained snapshots. This is selective
retrieval, not a claim of lower total tokens when every body is expanded.

Trace and Relate now follow the same complete-continuation principle. Their
selected facts and relation proof remain whole under the byte budget. The kernel
computes a digest before positional transport paging; MCP binds it to the query
and returns executable continuation or restart actions. A later-page change is
detected even when the first page is unchanged. Only page size and byte allowance
may vary. These are the read fixes #608 and #609 adapted to the native
writer, receipt and temporal contract; no compatibility layer is introduced.

A complete action costs response tokens. Measure the full traversal with equal
proof coverage before claiming any compression. These fixes do not change the
separate Wake section-scope review or establish independent agent understanding.

Wake now declares response paths derived from selected proof separately from
about context. Its context is not time-bounded; applied dimensions and their
predicates are reported for both groups. This metadata is counted before byte
projection and preserved by the typed transport. A historical window can have
empty proof and a null resume cursor while the about still has context and labels.
The native lesson and transport checks exercise that distinction; the change does
not reinterpret current-state prose as a historical answer.

Inspect now returns complete continuation calls while preserving about, ref,
include and object reuse. A stalled page supplies a sufficient next-item byte
allowance; required_bytes continues to measure the complete inspection. Changed
objects or selected proof return conflict with a fresh read that restores the
full object. Native checks execute the calls and reconstruct all selected
sections; the same calls run across embedded, gRPC, stdio and HTTP.

Ask and Wake now return executable calls through the shared typed projection.
The calls retain question, original wording, temporal selection and dimensions.
They negotiate enough bytes when stalled and restart a shortened core to restore
its full content. Typed cursor failures carry a fresh call instead of requiring
argument reconstruction. Native walks compare complete selected sections with a
full same-store reference. Independent agent evaluation remains a separate block;
these increments do not authorize a benchmark reader awaiting human review.
