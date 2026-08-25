# PIR Kernel Blind Context Consumption Evidence

Status: completed live run  
Date: 2026-04-11  
Scope: `Slice 5` evidence for the `PIR -> kernel` real integration plan

This document records one live in-cluster run of blind extraction followed by
kernel rehydration and LLM consumption of the resulting `rendered_content`.

The point of this slice is not to prove autonomous diagnosis. The point is
narrower and more important for integration:

- can a weaker extraction fixture still produce a bounded graph
- can the kernel rehydrate that weaker graph coherently
- can a downstream LLM answer correctly from the rehydrated context rather than
  returning `NOT_FOUND` or inventing unsupported causes

## Method

Runner test exercised:

- [`async-vllm-blind-context-consumption.sh`](../../../e2e/kernel-runner/tests/async-vllm-blind-context-consumption.sh)

Underlying assets:

- blind extraction fixture:
  [`vllm-graph-materialization.blind.request.json`](../../../api/examples/inference-prompts/vllm-graph-materialization.blind.request.json)
- kernel consumption prompt:
  [`kernel-context-consumption.txt`](../../../api/examples/inference-prompts/kernel-context-consumption.txt)

Runtime path:

1. Primary `vLLM` receives the blind extraction fixture.
2. The semantic reranker reclassifies relation `semantic_class` values.
3. The resulting `GraphBatch` is published to the kernel over NATS.
4. The kernel projects and rehydrates the graph through gRPC with
   `rehydration_mode=reason_preserving`.
5. The returned `rendered_content` is fed back to the primary model with a
   non-literal question about cause and mitigation.

Cluster endpoints used during the run:

- primary LLM: `http://vllm-qwen35-9b:8000/v1/chat/completions`
- semantic reranker: `http://vllm-semantic-reranker:8000/score`
- kernel gRPC: `https://kmp:50054`
- kernel NATS: `nats://kmp-nats:4222`

Operational notes:

- `LLM_ENABLE_THINKING=false` was used for this run
- node ids were namespaced by `run_id`
- the run used internal cluster endpoints, not public DNS, to avoid transport
  ambiguity during this slice

Run id:

- `e2e-vllm-blind-context-1775922307`

## Observations

Extraction and publication summary:

- root node id:
  `incident-2026-04-08-payments-latency--e2e-vllm-blind-context-1775922307`
- nodes: `4`
- relations: `3`
- details: `2`
- primary attempts: `2`
- semantic-classifier attempts: `1`
- semantic-classifier changed relations: `1`
- published messages: `6`

Kernel rehydration summary:

- requested scopes: `graph`, `details`
- `rehydration_mode`: `reason_preserving`
- neighbor count: `3`
- relationship count: `3`
- detail count: `2`
- rendered chars: `2028`
- selected detail node:
  `node-2026-04-08-3-rollout-config--e2e-vllm-blind-context-1775922307`

Selected rendered excerpt:

> Node Payments API Latency Incident (incident): Payments API latency and
> error rate spike following config rollout.

Observed final answer:

> The operational change that most likely explains the latency spike was the
> rollout of config 2026.04.08.3 which reduced DB maxConnections from 50 to 5,
> and the action that reduced user impact while recovery progressed was the
> on-call team's initiation of a rollback and traffic shift to a secondary
> region.

## Minimal Inferences

These inferences are supported by the observed run:

- the weaker blind fixture still produced a bounded graph that the kernel could
  materialize and read back
- the kernel returned rehydrated context rich enough for a downstream LLM to
  identify both the likely cause and the mitigation action
- the final answer used facts present in the rehydrated context rather than
  falling back to `NOT_FOUND`
- the semantic reranker remained active on this path and changed one relation
  before publish

## What This Does Not Prove

This run does **not** prove the following:

- that the primary model diagnosed the incident autonomously from raw evidence
- that the blind fixture is free from all useful hints
- that one successful run is enough to establish runtime reliability
- that public-DNS reasoning mode with thinking enabled is stable
- that the reranker always improves the final answer

This remains an integration slice, not an autonomy claim.

## PIR Design Implications

For `PIR`, the key implications from this run are:

- the contract is now validated on the path that matters most for PIR:
  `model extraction -> kernel publish -> kernel rehydration -> model answer`
- a bounded graph with stable root semantics is sufficient for the kernel to
  preserve operationally relevant cause and mitigation information
- the reranker can stay in the path as a semantic clean-up step without changing
  the external kernel boundary
- the next honest design question is not contract viability but extraction
  difficulty: how far we can weaken the fixture before the downstream answer
  degrades materially

## Next Honest Step

The next methodologically correct step is to expand this from one blind fixture
to a small family of blind incidents and compare:

1. extraction quality before/after reranking
2. rehydrated answer quality
3. degradation points as the prompt becomes less guided
