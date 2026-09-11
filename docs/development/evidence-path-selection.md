# Native evidence path selection

This library increment implements seed-based discovery under #538. It does not
add an MCP tool, expose a public pattern language, or claim that an agent can
translate a question into complete obligations. MCP projection and progressive
guidance are a subsequent part of the same integration track.

`MemoryApplicationService::evidence_paths` validates owned input refs and calls
the graph port. The embedded adapter opens one `ReadTx` for the whole operation.
The domain solver consumes `TraceSnapshotReader` and has no storage, transport,
model or text-generation dependency. Unsupported graph adapters return an
explicit error; there is no unbounded or live-read fallback.

## Choice and proof

Each explicit role has a finite ordered sequence of existing relation types
and directions. Obligations bind a name at a position to a label key's admitted
values or to the exact stored ref. These are internal application policy types,
not a replacement for the native verbs. A shared label never creates an alias;
an identity task needs its justified `same_entity_as` path.

A state is `(role, position, node, present domains, missing witnesses)`.
Equal states share expansion while preserving every predecessor. Different
domains or missing witnesses remain distinct even at the same stored node.
Indexed adjacency is cached independently by node/type/direction, so those
distinct states do not reread identical edge pages.

Every present witness intersects its binding's domain. Empty intersection
rejects that state. Missing label membership records the role, position, node
and key; another route cannot fill it. After discovery, a depth-first walk of
the predecessor DAG reconstructs each complete role path. This uses one stack
and one mutable path rather than copying a suffix for every pending branch.

Joining one candidate per role intersects ALL witnesses incrementally. Known
incompatibility is irreversible and can be pruned; pairwise compatibility is
insufficient. All compatible groups survive, including groups needing review.
Duplicate proofs of the same assignment do not create ambiguity. Multiple
binding assignments do. Independent per-key domains do not encode correlations
between values of different dimensions in a compound record.

Each candidate retains original directed relation-table indexes, node refs and
bindings. `groups` identify complete compatible alternatives; ungrouped
candidates remain available for diagnosis. Edges beyond a locally incompatible
state are not expanded, so those rejected suffixes are not claimed as read.
Reconstruction and group formation can still be exponential in the worst case;
shared states do not remove the cost of enumerating distinct proofs.

## Clocks, completeness and work

The existing `TraceTemporalAdmission` is reused with an explicit selection and
unfiltered dimensional context. It bounds ownership, node coordinates and
relation clocks on the same snapshot. Label obligations read only admitted
coordinates. A budget-stopped coordinate read is not an absent label.

Undated relations can be retained under a temporal cut, following existing
Trace admission. Such paths and groups explicitly require review. Known and
unknown groups can coexist; known proof existence does not resolve the others.

N includes discovered refs and coordinate label endpoints; E includes decoded
coordinate and adjacency rows. S covers root/extension attempts, predecessor
reconstruction visits and candidate-join attempts. Depth limits apply to the
role path. No token or material packing policy belongs to this solver.

Every N/E/S/depth cutoff reports `Partial`, even if a role or candidate was
already found. `known_complete` is false for partial searches. No frontier
exhaustion or complete group establishes semantic truth, global absence, or
that every obligation was supplied correctly. The current experiment uses N/E/S
and depth high enough that none binds; choosing paths remains the priority.

## Verification and next surface

Small SQLite tests cover joint event contradictions, absent witnesses, distinct
states at a convergent node, shared expansions with two preserved proofs,
100-hop discovery, work cuts, temporal identity admission, unknown clocks,
incoming direction and about ownership. The developer example
`evidence_paths_control` maps frozen investigator JSON into the library request;
it is not a supported API and does not read expected answers.

Before the capability becomes agent-facing, extend the existing native verb
surface, preserve full continuations and proof provenance, provide actionable
missing-obligation signals, map gRPC and embedded behavior consistently and
update the canonical guide sources under `agent-surface.md`. Do not advertise
the investigator schema as a new user query language or load a second manual.
Compare with the frozen joint-selection oracle, then validate how agents select
or discover obligations. No additional writer work is required by this layer.
