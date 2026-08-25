# PIR Kernel Live Context Consumption Evidence

Status: completed live run
Date: 2026-04-10
Scope: `Slice 2` evidence for the `PIR -> kernel` integration plan

This document records one live cluster run of the incremental `PIR` smoke that
publishes three waves into the kernel, rehydrates the resulting graph, and asks
the LLM to answer from `rendered.content`.

It is intentionally narrow. It describes what the run observed, what may be
inferred from those observations, and what remains unproven.

## Method

Test exercised:

- [`pir_graph_batch_incremental_context_consumption_smoke_succeeds_against_live_kernel`](../../../crates/kmp-testkit/tests/pir_graph_batch_roundtrip_smoke.rs)

Fixtures used:

- [`incident-graph-batch.json`](../../../api/examples/kernel/v1beta1/async/incident-graph-batch.json)
- [`incident-graph-batch.incremental-2.json`](../../../api/examples/kernel/v1beta1/async/incident-graph-batch.incremental-2.json)
- [`incident-graph-batch.incremental-3.json`](../../../api/examples/kernel/v1beta1/async/incident-graph-batch.incremental-3.json)

Runtime path:

1. `wave 1` publishes an incident root, one finding, and one mitigation
   decision.
2. `wave 2` reuses the same incident identity and adds one new finding plus one
   new task.
3. `wave 3` keeps the same graph shape, but corrects the current incident/task
   state and updates two `node_detail` revisions to `2`.
4. All three waves pass through the semantic reranker before publish.
5. The kernel materializes the graph and returns `graph + details`.
6. The third-wave `rendered.content` is passed back to the primary LLM for one
   question-answer turn.

Cluster endpoints used during the run:

- primary LLM: `http://vllm-qwen35-9b:8000/v1/chat/completions`
- semantic reranker: `http://vllm-semantic-reranker:8000/score`
- kernel gRPC: `https://kmp:50054`
- kernel NATS: `nats://kmp-nats:4222`

Run namespace:

- `rk-pir-context-1775853311`

## Observations

### Wave 1

Published graph:

- nodes: incident root, `db-pool-typo` finding, `reroute-secondary` decision
- relations:
  - `HAS_FINDING` from incident to `db-pool-typo`
  - `MITIGATED_BY` from incident to `reroute-secondary`
- detail nodes: `db-pool-typo`, `reroute-secondary`

Observed kernel summary:

- `root_node_id`: stable namespaced incident id
- `run_id`: `rk-pir-context-1775853311-wave-1`
- `published_messages`: `5`
- `neighbor_count`: `2`
- `relationship_count`: `2`
- `detail_count`: `2`
- `rendered_chars`: `1744`
- selected detail excerpt: rollout changed `DB maxConnections` from `50` to `5`

Observed semantic reranker result:

- attempts: `1`
- changed relations: `1`

Important note:

- the fixture declared the first relation as `evidential`
- the published graph after reranking carried that relation as `causal`

### Wave 2

Published graph:

- nodes: same incident root, `retry-storm` finding, `apply-retry-cap` task
- relations:
  - `HAS_FINDING` from incident to `retry-storm`
  - `REQUIRES_ACTION` from incident to `apply-retry-cap`
- detail nodes: `retry-storm`, `apply-retry-cap`

Observed kernel summary:

- `root_node_id`: unchanged from `wave 1`
- `run_id`: `rk-pir-context-1775853311-wave-2`
- `published_messages`: `5`
- `neighbor_count`: `4`
- `relationship_count`: `4`
- `detail_count`: `4`
- `rendered_chars`: `3249`
- selected detail excerpt: retries without jitter doubled concurrency and
  increased queueing pressure

Observed semantic reranker result:

- attempts: `1`
- changed relations: `0`

Observed rehydrated context:

- included both first-wave and second-wave graph elements
- included the second-wave finding `Retry storm amplified load`
- included the second-wave task `Apply retry cap`
- used `reason_preserving` rehydration mode

### Wave 3

Published graph:

- nodes: same incident root, same `retry-storm` finding, same
  `apply-retry-cap` task
- relations:
  - `HAS_FINDING` from incident to `retry-storm`
  - `REQUIRES_ACTION` from incident to `apply-retry-cap`
- detail nodes:
  - `retry-storm` with `revision=2`
  - `apply-retry-cap` with `revision=2`

Observed kernel summary:

- `root_node_id`: unchanged from earlier waves
- `run_id`: `rk-pir-context-1775853311-wave-3`
- `published_messages`: `5`
- `neighbor_count`: `4`
- `relationship_count`: `4`
- `detail_count`: `4`
- `detail_revision`: `2` for the selected task detail
- `rendered_chars`: `3457`
- selected detail excerpt: retry-cap change completed and DB wait time returned
  toward normal

Observed semantic reranker result:

- attempts: `1`
- changed relations: `0`

Observed rehydrated context:

- graph size stayed constant at `4` neighbors / `4` relationships / `4` details
- root incident summary updated to recovery language
- task summary updated from planned action to completed rollout
- detail revisions for `retry-storm` and `apply-retry-cap` advanced from `1`
  to `2`
- used `reason_preserving` rehydration mode

### Final LLM Answer

Prompt intent:

- answer in one sentence what happened after the retry-cap rollout, using the
  corrected rehydrated context

Observed answer:

```text
After the retry-cap rollout, incident latency returned below 1.1 seconds and
the retry-cap task reached a completed state with concurrency falling back
toward baseline.
```

## Minimal Inferences

These inferences are supported by the observed run:

- the kernel contract supports incremental incident materialization across
  multiple waves with stable incident identity and distinct `run_id` values
- the kernel can represent both graph expansion and graph correction
- the kernel can keep graph cardinality stable while still updating current node
  summaries, relation rationale, and `node_detail` revision
- the third-wave `rendered.content` preserved enough corrected context for the
  primary LLM to answer the targeted question correctly
- the semantic reranker was exercised on all three waves and was still useful
  on at least one relation in this run

## What This Does Not Prove

This run does **not** prove the following:

- that the primary model diagnosed the incident autonomously
- that the model selected the mitigation autonomously
- that the current fixture shape is free from prompt leakage
- that the primary model would emit correct `semantic_class` values without the
  reranker
- that one run on one incident is enough to generalize runtime reliability

The fixtures remain strong and intentionally shaped. This is still a contract
validation exercise, not an autonomy evaluation.

## PIR Design Implications

For `PIR`, the most relevant design implications from this run are:

- incremental waves are a viable representation for incident evolution
- stable node identity across waves is necessary and works as intended here
- `graph + details` with `reason_preserving` produced a usable downstream
  context for question answering
- corrective waves are viable: PIR does not need to model every update as graph
  growth
- the reranker should still be treated as operationally relevant, because the
  first wave required one semantic-class correction

## Next Honest Step

The next methodologically correct slice is not to claim higher autonomy. It is
to reduce fixture leakage and rerun the same structure:

1. build a weaker extraction fixture
2. evaluate graph structure before and after reranking
3. repeat the same context-consumption check
