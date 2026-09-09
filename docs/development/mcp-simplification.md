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
receipts, declared coverage and structured repair signals still belong to block 2.
A native packet replay is not independent agent evaluation and does not resume a reader awaiting human review.

Writer validation now carries stable feedback codes and member/field paths.
A missing prior read can return an executable Inspect action with about and ref.
No message parsing or evidence synthesis chooses the code or action. Receipts
and declared coverage remain pending; this is not completion of block 2.
