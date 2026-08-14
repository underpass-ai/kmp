# PIR Kernel Blind Structural Evidence

Status: completed live runs  
Date: 2026-04-11  
Scope: `Slice 4` evidence for the `PIR -> kernel` real integration plan

This document records the current live evidence for the blind extraction slice.
The goal of this slice is narrower than end-to-end incident solving: it checks
whether the primary model can still emit a kernel-friendly `GraphBatch` under a
weaker fixture, and whether the semantic reranker materially changes the
structural usefulness of the emitted relations.

## Method

Tests exercised:

- [`vllm_blind_graph_prompt_smoke_returns_valid_bounded_batch`](../../crates/kmp-testkit/tests/vllm_graph_blind_prompt_smoke.rs)
- [`vllm_blind_structural_smoke_reports_primary_and_reranked_scorecard`](../../crates/kmp-testkit/tests/vllm_graph_blind_structural_smoke.rs)

Fixture used:

- [`vllm-graph-materialization.blind.request.json`](../../api/examples/inference-prompts/vllm-graph-materialization.blind.request.json)

Runtime path:

1. The primary model receives the blind extraction fixture.
2. The test validates only a bounded local `GraphBatch` shape:
   - `4` nodes
   - `3` outward relations from the root
   - `2` non-root `node_details`
3. The test scores the emitted graph structurally:
   - root present
   - graph connected from the root
   - at least one finding candidate node
   - at least one evidence candidate node
   - at least one action candidate node
   - relation `semantic_class` compatibility against those inferred roles
4. The same batch is then passed through the semantic reranker over `/score`.
5. The test recomputes the same structural scorecard after reranking.

Cluster endpoints used during the run:

- primary LLM: `https://qwen35-9b.llm.underpassai.com/v1/chat/completions`
- semantic reranker: `https://vllm-semantic-reranker.underpassai.com/score`

Operational note:

- `LLM_ENABLE_THINKING=false` was used for the primary model on this run
- this avoids short-request ambiguity on the public DNS path
- it means this slice validates structure, not public-DNS reasoning mode

Prompt smoke outcome:

- the blind prompt smoke also passed against the same public DNS path
- this confirms that the weaker fixture still yields a valid bounded batch over
  the real public endpoint
- that smoke only asserts contract shape; it does not emit a structural
  scorecard

Structural run id:

- `vllm-blind-structural-1775921845`

## Observations

### Primary model output

Observed graph shape:

- `node_count`: `4`
- `relation_count`: `3`
- `detail_count`: `2`
- `root_present`: `true`
- `connected_from_root`: `true`

Observed node roles:

- finding candidate: `node-config-change`
- finding/evidence/action candidate: `node-env-inspection`
- action candidate: `node-rollback-action`

Observed relation classes before reranking:

- root -> `node-config-change`: `causal`
- root -> `node-env-inspection`: `evidential`
- root -> `node-rollback-action`: `procedural`

Observed scorecard summary:

- acceptable relations: `3 / 3`
- primary attempts: `3`
- prompt tokens: `1344`
- completion tokens: `1444`

### Reranked output

Observed reranker result:

- semantic-classifier attempts: `1`
- changed relations: `1`

Observed relation classes after reranking:

- root -> `node-config-change`: `causal`
- root -> `node-env-inspection`: `causal`
- root -> `node-rollback-action`: `procedural`

Observed scorecard summary:

- acceptable relations: `3 / 3`

Important note:

- the reranker changed one relation, but it did **not** increase the accepted
  structural coverage in this run
- before and after reranking, the batch already satisfied the explicit
  acceptance rubric used by this slice
- what changed was the interpretation of the inspection node from
  `evidential` to `causal`

## Minimal Inferences

These inferences are supported by the observed run:

- the primary model can emit a valid, local, connected `GraphBatch` under a
  weaker fixture than the earlier contract-heavy prompts
- the emitted graph still contains the minimum structure expected by the PIR
  plan: one incident root, one causal/evidential branch, one supporting
  inspection branch, and one action branch
- the semantic reranker is still active on the blind path and can change
  relation interpretation
- in this specific run, the reranker improved semantic consistency for one
  relation, but it did not change the scorecard pass/fail outcome

## What This Does Not Prove

This run does **not** prove the following:

- that the primary model diagnosed the incident autonomously
- that the primary model selected the mitigation autonomously
- that the reranker produced the one true semantic ground truth
- that public-DNS reasoning mode is stable with thinking enabled
- that one blind fixture is enough to generalize runtime reliability

The blind fixture is weaker than the earlier one, but it still constrains root
identity and graph cardinality. This remains a contract-and-structure slice,
not an autonomy slice.

## PIR Design Implications

For `PIR`, the most relevant design implications from this run are:

- a stable root plus non-deterministic non-root node ids is still workable for
  local incident extraction
- `finding` and `evidence` remain close in practice; PIR should expect overlap
  and avoid overfitting downstream logic to a brittle distinction
- the reranker should still be treated as useful but not magical; it can change
  relation interpretation even when the primary output is already structurally
  acceptable
- the next question is no longer whether the model can emit a bounded graph;
  it is whether the kernel-rendered context from that weaker graph supports
  correct downstream answers

## Next Honest Step

The next methodologically correct slice is `Slice 5`:

1. publish the blind extraction output
2. rehydrate it through the kernel with the intended query profile
3. ask non-literal questions about the incident
4. verify that the answer uses the rehydrated context without inventing absent
   causes or mitigations
