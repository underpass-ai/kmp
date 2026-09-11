# Select route material before returning complete proofs

Status: implemented in this increment after PR704; validation and delivery below.
F124's native control shows that extra routes can enable shared material, but
returning a second 100-hop route almost doubles structured JSON without improving
coverage. Source: kmp-eval/reports/TRACE-ALTERNATIVES-20260911.md.

## Required behavior

Add optional `search.select` to bounded Trace. Candidate discovery stays under
its existing N/E/D/S bounds and shared snapshot. Select over that bounded catalogue
before serializing full relation evidence. Do not download the whole graph first.
Keep default calls unchanged and keep M 2's distinction between discovery and
material costs. Do not add work to the writer or infer required evidence from prose.

Selection policy data:
- max_material_nodes: required, distinct entry refs on selected paths, including
  source; separate from search.max_nodes discovered during exploration.
- max_paths: default 4, bounded 1..8.
- groups: optional, up to 8 groups of explicitly known target refs, each with
  alternatives (OR of at most 8 nonempty AND sets), optional positive integer
  weight (default 1). Every group ref must be in to. Without groups, each target
  is one atomic group of equal weight. These are caller-declared retrieval
  requirements, not kernel-certified semantic proof obligations.

The existing ProofDependencyGroup is a bounded undirected neighborhood
(max 2 hops/8 members), not an AND/OR requirement. Do not silently reinterpret it.
ContextGroup packages already returned evidence and native reads; it is also
not a requirement specification. Reuse their provenance/omission principles,
not their unrelated semantics. Keep new DTOs at transport boundaries and native
selection value types in the existing domain; no new bounded context/framework.

## Frozen mathematical policy

Use the existing external M 2 formula with B 4, alpha 1/2, lambda 0, no fitting:
F(U) = sum_j w_j * 1[an alternative of group j is entirely in U].
H(U) = sum_uncovered_j w_j * max_a |U intersect G_ja| / |G_ja|.
Beam priority F + H/2, with feasible material/path limits. Retain best real F,
then fewer material entries, fewer paths, lexicographic candidate indexes.
All route evidence is preserved for selected paths, deduplicated in one table.

Implement integer/rational comparisons rather than floating confidence scores:
all AND sets have at most 8 refs; LCM(1..8)=840 makes priority
1680*F + sum_uncovered w_j*matched*(840/|G_ja|) exact. Bound weights and arithmetic.
This is beam search, not MHA*, and offers no optimality guarantee beyond checked
finite catalogues. Shared node union counts once. Path catalogue can have up to
64 routes from 8 targets with 8 alternatives each. Parent discovery remains bounded;
selection evaluates at most a small explicit number of beam expansions under
max_paths. Expose selection work separately from discovery S.

## Response and continuations

`trace` and `routes` contain selected complete paths only, with reindexed shared
edges and unchanged stored arrows/why/evidence/clocks. Report candidate count,
selected original candidate indexes, material count, group coverage and incomplete
requirements. Discovery unreached/incomplete quotas remain distinct from material
omissions. Returning no selected route must not imply no candidate was found.
M 2 may choose empty when no group completes: retain visible availability and a
complete action to expand the bounded candidate catalogue without selection.

Keep the full-candidate selection digest in the cursor fingerprint so an unseen
candidate/evidence change cannot silently reuse an old paginated selection.
Pagination finishes selected proof. Expanding candidates starts a new selection,
not a promise to retain a snapshot across calls. Measure any such expansion as
part of whole-reader context cost. Do not reuse omitted relation indexes or claim
body consistency: graph/body snapshot #539 remains an explicit next requirement.

## Acceptance before delivery

- Port the frozen selector with finite exhaustive-oracle controls covering shared
  nodes, AND 3, OR alternatives, weights, zero benefit, infeasible material, ties,
  pruning and excluded refs. Preserve failures; no parameter tuning on old data.
- Native MCP controls compare raw candidates vs selected proof from the same
  store/cut, exact selected arrows and proof, N/E/D/S unchanged, material limit
  honored, serialized whole responses and total expansion cost measured.
- Include 100-hop alternative branches, partial/unknown candidates, stale cursor,
  transport pagination and executable full-candidate expansion.
- Update protobuf, maps, schema/projection, guide/examples/maintenance together.
  No editorial CI gate. Human review still precedes independent Cala readers.
- New run ID, inputs/source/binary frozen before calls, no concurrent build;
  static figures/tables. Native M 2 success remains conditional on supplied targets,
  declared groups and admitted candidate catalogue. Dimensions/intent-directed
  discovery and proof bodies in the same snapshot remain pending, not complete.
