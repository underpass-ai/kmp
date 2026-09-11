# Native evidence path selection

Seed-based discovery under #538 is exposed through the existing `kmp_trace`
verb's `search.seek` arguments. It does not add a tool or a text query language,
nor claim that an agent can translate a question into complete obligations.
The domain types remain internal; proto mapping compiles relation roles and
witness constraints for both embedded and gRPC dispatch.

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

## Native agent contract

A simple call supplies `from` and `search.seek:["verified_by"]`, without `to`.
Each role can name `rel`, `direction`, ordered `via` moves before the relation,
and ordered `after` moves after it. The main relation's traversal endpoint is
its witness. No caller variables or numeric witness positions are necessary.
`labels` constrains only that witness; `same_labels` joins listed keys across
all role witnesses; `same_ref` joins disjoint groups of main-relation endpoints; role strings name witnesses and {role,at:"anchor"} names traversal starts.
Shared explicit label constants are intersected before execution; contradictory
constants are an invalid request, unlike incompatible stored candidates.

For role path sets P1...Pk, a group chooses one path from each Pi. Every shared
known domain must have nonempty intersection across the entire group; missing
witness values remain recorded, not filled from known peers. An incoming move
changes traversal direction, never the stored relation statement. This is exact
compatibility for declared paths, without token utility or score-based pruning.

Response `seek` carries status, declared-obligation completeness, missing roles,
clock resolution and work counters. Whole `trace`, `candidates` and `groups`
items share one positional page. Candidate/edge indexes are global. The cursor
hashes every complete table, including unreturned bindings and groups, before
pagination; changing either arguments or proof rejects continuation. A larger
work allowance starts a new selection, never resumes an old search state.

`seek.review_context` optionally offers a Goto focused on the seed at the same
explicit `as_of` and axis. It is separate from `next_actions` page completion.
No implicit widening or unbounded Inspect replaces a historical read. Other
cuts require the reader to choose its next temporal move deliberately.
The audit card, verb, error lesson and example:evidence-seek share this contract.

Remaining scope: automatically discovering the task's obligations, alternative
relation sequences for one role, variable-length path patterns, correlations
between several dimension values, native-agent understanding, and body/source
batching. Fixed ordered paths can be deep but are not arbitrary graph programs.
The existing work ceilings still apply; the frozen correctness controls set
allowances above every observed requirement. No writer burden is added.

## Contextual sequence discovery

A role with `via:"context"` (typed proto `context:true`, no explicit via) adds
contextual prefix discovery before its main relation and optional fixed suffix.
The goal relation/direction and witness constraints remain explicit. There is
no NLP classifier or universal implication rule over semantic links.

Let H be the same-about, temporally admitted graph of justified rich relations,
with two traversal directions preserving each stored statement. Set d(seed)=0
and d(v)=minimum number of context hops from the seed. Keep the predecessor DAG
P(v)={(u,e): d(u)+1=d(v)}. For each reachable anchor a with the requested directed
relation to witness w, enumerate P(a), then that main relation and the declared
suffix. All equal-length prefixes survive; no token ranking or route quota is
applied. No discarded longer prefix is claimed to be semantically redundant.

The product state is (role, declared position, node, bindings, context_hops).
Its total depth strictly increases. Prefix distances are shared across roles;
adjacency by node/direction is read once and reused for typed main steps. The
existing state DAG retains alternative prefixes and fixed suffixes; the existing
joint solver applies all witness constraints. Witness projection offsets its
position by the actual context depth, without adding a hidden identity binding.

Context expansion is linear in admitted nodes/edges per role before path and
joint enumeration; the number of tied shortest paths or compatible groups can
still be exponential. Work is bounded by the same N/E/S/depth controls, charged
before reads/expansion/reconstruction. A cutoff remains partial. Zero-token
penalties do not remove these explicit work ceilings.

Outputs add context_discovery and per-candidate context_hops, included in the
complete selection fingerprint. Contextual leads never set declared obligations
complete. Compatible becomes review_required; ambiguity, incompatibility,
missing roles, clock unknowns and partial work remain distinct. All pages use
the same global proof indexes and the original relation arrows/why/evidence.

This closes automatic *intermediate sequence* discovery for an explicit goal
relation, with a documented minimum-hop policy. Choosing obligations from a
question, preferring semantically useful longer paths, correlations between
keys, and independent-agent understanding remain separate claims. Compression
is the subsequent reader-written navigation projection; it is not implemented
or required by this change.

## Equality at relation anchors

A main relation has two traversal endpoints: anchor immediately before it and
witness immediately after it. Incoming changes their traversal roles, not the
stored arrow: P --authorizes--> R has anchor R when read incoming. Neither a
shared event value nor a common authorizer makes two action refs identical.

`same_ref` accepts role strings (witness shorthand) and `{role,at}` members, with
at equal to anchor or witness. Groups partition endpoints, not whole roles; a
role can join its anchor in one group and its witness in another. Proto uses
TraceReferenceEndpoint. Mapping compiles each point into the existing reference
binding at via.len() or via.len()+1. There is no new variable language or solver.

For contextual prefixes, position-zero bindings are captured at each discovered
main-relation anchor, just before that main transition. They are not captured
at the seed or propagated through arbitrary bridges. Context expansion therefore
remains shared and unfiltered. Different anchor assignments remain distinct after
converging on the same witness. Fixed via bindings retain their existing position
semantics. Joining all groups intersects exact refs and preserves missing values.

For candidate c in role r, let A(c) be its main anchor and W(c) its witness. A
complete group G is retained only if all required endpoint equalities hold, e.g.
A(verification)=A(permission)=A(constraint), and W(identity)=W(permission).
This chooses a compatible set of discovered paths, including a longer route to
an authorized action when a shorter route reaches a different action. It does
not enumerate longer prefixes to the same anchor merely to repeat the same
obligations, nor infer unstated requirements. Source relevance still needs review.

Candidates expose anchor alongside witness and context_hops. Reference bindings
expose their endpoint members when an anchor participates; witness-only groups
keep their compact roles representation. Proto fingerprints include all anchor
refs and binding endpoints before pagination. Validate reversed edges, explicit
prefixes, mixed anchor/witness equality, convergence, separate groups for one
role's endpoints, unknown role/endpoint/duplicate refusal, transport parity and
changing an unreturned anchor assignment. No canonical memory or writer change.
