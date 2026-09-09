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
| 1. Dimensions | Identity by about/key/value; multiple values per key; ingest, relabel, selectors and projections | Same value under different keys coexists; multiple aliases work; results and aggregates do not duplicate memories; old refs and stores remain usable under an explicit compatibility policy |
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

Before changing persistence, select and document how old refs resolve and how
older binaries handle the new format. Do not silently reinterpret stored history.
Test old-store reads and round trips in isolated stores. Adding labels later must
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
