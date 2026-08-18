# Graph-aware `kernel_ask` reranker

Status: implemented for the release following `0.1.10`.

## Contract

`kernel_ask` ranks canonical evidence once, before response budgeting. The
pipeline is deterministic and embedded:

```text
evidence -> direct lexical gate -> typed graph features -> relevance order
         -> bounded compound-query rerank -> distinct-claim prefix
         -> confidence and audit fields
```

Evidence text remains the primary signal. A candidate must cross the direct
lexical gate using its own text, source, refs, supports, or metadata. Only then
may direct explanatory graph context improve its rank.

The graph feature retains relation type, semantic class, direction, opposite
endpoint, `why` terms, and relation-evidence terms. Structural and procedural
edges are excluded, as is the evidential `supports` edge that would lead from
a popular claim to sibling evidence. A relation rationale contributes only
when that relation also carries evidence. At most 16 relation features are
considered per candidate.

Superseded claims are excluded from current-advice queries. They remain
available when the question explicitly asks about replacement or prior state.

The first 64 relevant candidates enter the coverage reranker; the remaining
tail keeps its stable relevance order. This bounds the greedy step while all
canonical evidence remains available for proof expansion. Stable evidence
refs break final ties, so bundle traversal order cannot change the result.

## Frozen evaluation cases

The executable suite covers:

| Case | Assertion |
| --- | --- |
| #90 paraphrase recall | Two reordered/morphological paraphrases pass through the real embedded MCP path, each repeated three times; an unrelated control remains `UNKNOWN`. |
| #100 / ADR-equivalent regression | The SQLite-WAL decision remains in the five-citation core ahead of `earlier`, `more`, `one`, and `same` distractors; `chosen_because` remains in `proof.matched_relations`. |
| Compound questions | Novelty is computed from the same evidence, claim, and typed-relation features as initial relevance. |
| Duplicate claims | Best representatives of distinct supported claims precede repeat citations. |
| High-degree safety | Graph-only matches and `supports` sibling vocabulary cannot cross the direct eligibility gate. |
| Lifecycle | Current advice excludes a superseded claim; an explicit historical/replacement query can still audit it. |
| Determinism | Candidate permutations produce the same order; the MCP regression compares three identical structured responses. |
| Confidence | Confidence is recomputed from only the retained answer-core evidence. |

The post-#90 partial-amendment scenario in #91 remains a release evaluation on
the frozen real store; it is not reclassified as a new product mutation.

## Cost note

The `301`-candidate debug regression exposed the reason to bound reranking. In
the same local debug profile, the package suite with an unbounded greedy pass
finished in approximately `1.70 s`; after limiting the greedy window to 64,
the expanded suite finished in approximately `0.30 s`, and the isolated
301-candidate test in approximately `0.15 s`. These are developer-machine
regression timings, not a production benchmark. The useful complexity change
is from a greedy pass over all candidates to sorting plus a fixed-size greedy
window; direct relation collection never walks evidence siblings.

No embeddings, global IDF, model call, network service, or public v1beta1
schema change is involved.
