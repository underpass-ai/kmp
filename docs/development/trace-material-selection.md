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

## Native validation, 2026-09-11

Implementation a7d10e02, binary SHA256 c891069bb3b9713b933880f6225a890722ed026299562b994a65f731cc051dbb.
The isolated kmp-eval run `trace-material-20260911-v4` made 81 RPCs, including
16 expected errors, at zero API cost. Native choices, material, benefit and
selection work exactly match frozen external M2 in all three candidate catalogues.
No parameters were fitted. Selected relation objects match raw candidates exactly;
N/E/S and temporal omissions remain unchanged. Selected paging (3/6/2 pages),
optional expansion, infeasible/unknown requirements and changed-hidden-proof
cursor rejection pass. The deep case preserves a 100-hop route plus both AND
supports, at 103 material nodes. Small exhaustive domain controls check 35
budget/weight combinations without assuming global beam optimality.

| Control | Raw → selected JSON tokens | Selected group coverage | RPC median ms |
| --- | ---: | ---: | ---: |
| Shared paths | 1,112 → 842 | 2/2 | 1.13 → 1.18 |
| Deep AND | 23,953 → 12,479 | 1/1 | 24.24 → 21.61 |
| One route at a temporal cut | 531 → 640 | 1/1 | 2.06 → 1.83 |

Tokens count the whole sorted compact structured response with tiktoken 0.14.0,
o200k_base, including material metadata and the optional expansion call. They are
not host context or billing. Five measured pairs per case, alternating order after
one warmup pair; local latency is exploratory. Reading every candidate afterwards
costs 1,954 tokens shared and 36,432 deep in total, more than a direct raw read.
Keep selection optional, and do not automatically follow candidate expansion.
The reviewed Trace schema increases from 2,094 to 2,442 tokens; no size CI gate.

V1-V3 preserve investigator fixture errors: full reingest changed clocks; then
canonical relation-only writes lacked the required array/entry envelope. V4
restates the unchanged unselected entry and its dimension and changes only the
unselected semantic relation. Exact selected-proof equality remains asserted.
The engine did not change between these runs. Operational report, raw calls,
CSV and PNG/SVG/PDF live in kmp-eval `reports/TRACE-MATERIAL-20260911.md` and the
versioned artifact directories. This result does not close #538 or #539.
