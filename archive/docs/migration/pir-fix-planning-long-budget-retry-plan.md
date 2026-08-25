# PIR Fix Planning Long-Budget Retry Plan

> **Historical (superseded).** Point-in-time record; the PIR repo was restructured (the `fix_planning` specialist program was decommissioned) and the PIR code/tooling paths referenced below are point-in-time, not live.

Status: Superseded (PIR `fix_planning` specialist program decommissioned; repo restructured)
Date: 2026-04-12
Scope: next iteration series for the first real `PIR` event-driven agent after
the initial `fix_planning` live smokes

Companion research note:

- [`pir-fix-planning-model-research-notes.md`](pir-fix-planning-model-research-notes.md)
- [`pir-fix-planning-experiment-matrix.md`](pir-fix-planning-experiment-matrix.md)

## Intent

Define the next reliability-oriented iteration series for `fix_planning`.

This plan changes one core assumption from the earlier slice:

- the system should no longer optimize first for short wall-clock latency
- it should optimize first for giving the LLM a real chance to understand and
  mitigate a production incident

Operational product stance for this series:

- a `15-20` minute wall-clock budget is acceptable for a productive incident if
  the agent can act autonomously and truthfully
- early prompt shrinking, token squeezing, and other premature latency
  optimizations are explicitly not the first move

## Why A New Plan Is Needed

The earlier slice plan in
[`pir-first-event-driven-agent-plan.md`](pir-first-event-driven-agent-plan.md)
set a conservative timing policy:

- runtime/session setup: `<= 5s`
- graph read: `<= 5s`
- primary LLM attempt: `<= 20s`
- optional second attempt: `<= 20s`
- publish: `<= 10s`
- total task budget: `<= 45s`

That was useful as a first safety-oriented proving slice. It is no longer the
right default for an autonomy-oriented incident workflow.

The live smokes showed a different reality:

- runtime and kernel graph read are not the dominant bottlenecks
- the first LLM JSON completion is the real limiting step
- the current retry count and sub-budgets are too small to give the model a
  serious chance

Current observed failure mode:

- runtime returns successfully
- kernel local read returns successfully
- the first `CompleteJSON` call times out under the current sub-budget
- later retries do not materially change the outcome
- the system escalates truthfully

That is better than false success, but it is still not a serious autonomy
budget.

## Grounded Starting Point

What is already true now:

- the `PIR -> runtime -> kernel` path is live
- `fix_planning` already runs as an event-driven task
- local graph read is already in use
- failure is now truthful at the stage boundary
- the smoke harness now reports the real `.failed` / `.escalated` outcome and
  applies the result through the same stage-result transition path used by the
  service
- the deployed `PIR` has now produced at least one real `fix_planning.completed`
  outcome under the long-budget `A0` policy, with valid kernel read-back
- internal planner sub-attempts are now exposed as first-class execution
  evidence in the deployed smoke output
- the deployed smoke can now load named YAML scenarios instead of relying on
  only one built-in hardcoded incident shape
- the first widened YAML scenario batch has now completed `3/3` truthful live
  runs on the deployed `A0` baseline

What is still open:

- fallback model support is not yet wired
- specialized graph roles are not yet showing meaningful divergence in the live
  graph
- wider scenario coverage has not yet been sampled enough to declare `A0`
  robust across incident families
- we still do not have a promotion decision between staying on `A0` longer and
  moving forward to `D1` or `A1`

## Latest Decision After First Valid `D1`

The first valid `D1` run did not change the baseline decision.

Observed outcome:

- `D1` executed correctly end-to-end
- it failed truthfully on proposal quality, not on infrastructure
- it added useful diagnostic signal, especially around weak evidence grounding
  and weak rollback quality
- it did not outperform `A0`

Operational conclusion:

- `A0` remains the baseline that should be protected
- `D1` remains experimental
- the next work should improve the common planner-quality layer before spending
  more effort on reranker-specific routing

## Delivery Hardening Update

`A0.2` is now closed for the two failure-path regressions that mattered most.

Evidence:

- the orchestration engine now ignores duplicate `.failed` and `.escalated`
  results once the task is already terminal
- a dedicated deployed smoke published duplicated
  `payments.incident.fix-planning.escalated` events for the same task
- the live result persisted only one terminal stage event, one
  `status.escalated`, one `escalated.to-human`, and produced `0` DLQ messages
- a second dedicated deployed smoke published duplicated
  `payments.incident.fix-planning.failed` events for the same task
- the live result persisted only one `.failed`, created only one retry task,
  and produced `0` DLQ messages

Implication:

- terminal delivery semantics are now materially cleaner on the validated
  `fix_planning` failure and escalation paths
- the next iteration should focus on widening experiment coverage or moving the
  planner stack forward, not on re-litigating these specific duplicate-delivery
  regressions

## Execution Evidence Update

`A0.3` is now materially implemented.

Evidence:

- deployed smokes can now emit a compact `execution_evidence` block for the
  exact `task_id`
- that block exposes internal sub-attempt count, durations, operations, token
  usage, finish reasons, and failure reasons
- the smoke catalog is now backed by named YAML scenarios
- a live `cache-stampede` run completed truthfully while surfacing four internal
  planner sub-attempts inside one workflow-visible task attempt

Implication:

- one of the most important interpretability gaps in `A0` is now closed
- the next iteration should use this evidence to widen coverage, not just add
  more infrastructure

## Scenario Matrix Update

`A0.4` is now also completed.

Evidence:

- the deployed smoke ran the full YAML scenario catalog in one batch
- `cache-stampede`, `connection-pool-exhaustion`, and `queue-backlog` all ended
  in truthful `.completed`
- all three preserved kernel read-back semantics
- all three completed in `task_attempt = 1`
- all three also completed in `internal_attempt_count = 1`

Implication:

- the current `A0` baseline now has first breadth evidence, not only repeated
  evidence on one fixture
- the next iteration no longer needs to prove that `A0` works across more than
  one scenario family at all
- the next decision is whether to deepen `A0` further or spend the next slice
  on a support-sidecar or stronger planner experiment

## Queue Topology Update

The queue-topology hypothesis is now partially validated in live execution.

Evidence:

- `PIR` now runs separate JetStream durables for:
  - alerts
  - stage results
  - stage requests
- those consumers now use `DeliverPolicy = New`, so a freshly created durable
  no longer replays historical request backlog by default
- a deployed smoke with two concurrent `fix_planning.requested` runs proved
  that the old replay bug is gone
- the same concurrent smoke also proved that the current `request` lane is
  still effectively single-threaded at the worker level:
  - both requests were delivered into the lane
  - one run moved to `running`
  - the other remained in `requested`

Implication:

- separating `request` and `result` traffic was the correct first fix
- it removes the cross-lane head-of-line blocking that was delaying terminal
  transitions
- the next architectural question became how to preserve FIFO semantics per
  `incident_run_id` while allowing different incidents to advance concurrently

Follow-up result:

- `PIR` now dispatches `request`-lane work by `incident_run_id`
- ordering is preserved per key inside the worker
- unrelated incident runs can now start specialist execution concurrently on
  the same pod
- a repeated live smoke with two `fix_planning.requested` events confirmed that
  both runs now move from `requested` to `running` within the same short window

Remaining note:

- this closes the immediate single-pod head-of-line blocking problem on the
  `request` lane
- any future horizontal-scale semantics across multiple `PIR` pods should be
  evaluated separately if and when replica count increases

## `A0.v3` Budget Update

The current deployed baseline is now:

- `agent_version = fix-planning/2026-04-13.generate-budget.a0.v3`
- `stageBudget = 24m`
- `attemptPhaseBudget = 6m`
- `llmGenerateBudget = 5m`

Latest confirmed behavior:

- the widened `generate` budget materially improved the baseline
- `fix_planning` no longer fails only because the planner never answers
- live runs now show mixed behavior:
  - some attempts still time out in `generate`
  - other attempts complete `generate -> repair -> judge`
  - successful runs can now complete `fix_planning` and advance the workflow

Operational conclusion:

- `A0` remains the correct baseline
- the next hardening work should focus on planner-quality and contract quality,
  not on replacing the planner family yet

## Planner Hardening Candidates To Try Next

The latest live evidence narrows the common failure mode to rollback quality and
planner variability, not queueing or hook mechanics.

Recommended hardening candidates:

1. require rollback steps to name the exact change being reverted
2. require rollback steps to include an explicit verification step
3. require rollback preconditions to include an abort condition
4. require evidence basis to reference the concrete finding or evidence signal
5. reject plans that introduce ungrounded mechanisms such as cache warming
6. require the rollback to be the inverse of the proposed patch, not a generic
   recovery action
7. reject plans whose rollback language is reversible in prose but not
   operationally actionable
8. feed the precise local rejection reason back into the next retry, not only a
   generic malformed-or-low-confidence message

More concrete examples for the next slice:

- if `patch_description` says to stagger invalidation, `rollback_steps` should
  say how to restore the prior invalidation behavior
- if a rollback mentions cache safety, it should also mention a verification
  signal such as miss rate, latency, or consistency
- if a plan implies a capability like an admin endpoint, cache warmer, or
  feature flag, that capability must be supported by the evidence or it should
  move into `assumptions`

What not to confuse with hardening:

- bigger planner models
- more graph roles
- reranker-specific routing
- transport proxy work such as `Envoy`

Those may still matter later, but they are not the next common-layer fix.

## Latest Live Outcome

The latest deployed `A0` smoke materially changed the state of this plan.

Observed live result:

- long-budget `A0` on deployed `PIR` produced a truthful
  `payments.incident.fix-planning.completed`
- `run_status` advanced to `mitigating`
- `task_status` advanced to `completed`
- a real `decision:{incident_id}:fix-planning` node was materialized
- `ADDRESSES` and `BASED_ON` were both confirmed on kernel read-back

Important inference:

- the current `Qwen3.5-9B` planner is not categorically incapable of this task
- the previous dominant blocker was not just reasoning quality
- the earlier live failures were materially influenced by output-budget and
  response-shape constraints

That means the next iteration should not immediately jump to a larger planner.
It should first use the now-improved evidence layer to test breadth.

## Key Decision For The Next Series

The next series should prioritize these changes first:

1. more task-level wall-clock budget
2. more agent iteration retries
3. explicit operational support for long-running stage execution

It should not prioritize these first:

1. prompt reduction
2. token-budget reduction
3. more graph roles
4. aggressive output simplification just to fit the old time envelope

Reason:

- the current evidence says the agent is under-timed, not over-contextualized
- we should first find out what the current local graph path can do when given a
  realistic autonomy budget

Updated reading after `A0.3`:

- budget, retries, delivery hardening, and execution evidence are now all good
  enough to treat `A0` as a real baseline
- the next uncertainty is no longer "what happened inside one run?"
- the next uncertainty is "how broad is the working envelope across incident
  families?"

## Important Operational Constraint

Longer LLM budgets require an accompanying transport decision.

Current `PIR` JetStream consumer configuration uses:

- `AckWait = 5m`

That means a `15-20` minute `fix_planning` budget cannot simply be enabled in
the executor while keeping the current consumer contract unchanged.

One of these must happen in the same iteration series:

1. raise the consumer `AckWait` above the maximum expected stage duration
2. move long-running specialist execution behind a decoupled async worker model

For the next practical slice, the simpler path is:

- keep the current consumer architecture
- raise `AckWait` to match the new stage budget with safety margin

## New Concurrency Constraint From Live Smokes

The latest in-cluster smoke exposed a different class of bottleneck:

- a long-running `payments.incident.*.requested` message can keep the only
  active consumer busy for minutes
- while that happens, a fast terminal event like
  `payments.incident.fix-planning.escalated` from another incident can remain
  pending even though it is already in the stream

This is not primarily an LLM-quality problem.
It is a consumer-topology problem.

Operational reading:

- global FIFO across all incident messages is not acceptable
- FIFO should hold per `incident_run_id`, not across unrelated incidents
- `request` and `result` traffic should not share the same critical lane

Near-term architectural direction:

1. split consumers by event class:
   - `alerts`
   - `stage requests`
   - `stage results`
2. preserve ordering semantics per `incident_run_id`
3. keep stage-result handling on a fast lane so terminal transitions do not sit
   behind old long-running planner work

## `Envoy` Note

`Envoy` is now recorded as a possible later hardening layer for:

- transport retries
- rate limiting and quotas
- circuit breaking
- mTLS normalization across runtime, kernel, and model endpoints

But it is not the next fix for the current live blockage.

Reason:

- `Envoy` can improve transport policy
- it does not solve event-lane contention or global FIFO between incidents

So the intended order remains:

1. fix consumer topology and per-incident ordering first
2. only then consider whether `Envoy` adds enough value to justify its
   operational complexity

## Proposed Policy Revision

### Retry Count

Replace the current provisional policy:

- `max_iterations_per_task = 2`

With a new starting policy:

- `max_iterations_per_task = 4`

Interpretation:

- one primary attempt
- up to three bounded retries before escalation

This is intentionally more permissive because the objective is now to give the
planner a real opportunity, not just a quick structured-output check.

### Task Wall-Clock Budget

Replace the current `45s` task budget with:

- `fix_planning total task wall-clock budget <= 20m`

This should be treated as the working baseline for the next series, not as the
final forever value.

### Proposed Phase Budget Baseline

Initial long-budget policy:

- runtime/session setup: `<= 30s`
- graph read and local detail fetch: `<= 30s`
- publish + read-back: `<= 2m`
- remaining budget reserved for planning attempts: `~17m`

Proposed attempt-level budget:

- attempt 1: `<= 4m`
- attempt 2: `<= 4m`
- attempt 3: `<= 4m`
- attempt 4: `<= 4m`

This gives:

- enough time for a real planner response
- enough room for multiple tries
- explicit reserve for publish/read-back and operational slack

### In-Attempt Budget Shape

Do not optimize this too aggressively yet.

For the next slice, the in-attempt policy should be generous:

- primary planner generation gets the majority of the attempt budget
- repair remains available, but should not starve generation
- judge should remain bounded and may be moved to advisory mode if it becomes a
  secondary bottleneck

The main rule is:

- do not keep an `8s` first-generation budget when the system claims to support
  autonomous production mitigation

## Role And Model Strategy

### Roles

Roles are not the first lever for this series.

Current live evidence showed that:

- `incident-commander`
- `fix-planning-agent`

returned essentially the same bundle in the failed-state graph.

So the next slice should not spend time inventing more read roles unless live
evidence later shows meaningful graph-shape divergence.

### Models

Model separation is valuable, but only after the long-budget baseline exists.

Recommended progression:

1. first prove the current planner with a realistic time and retry budget
2. only then add fallback model routing if needed

If a later iteration needs multi-model support, the recommended split is:

- primary planner model: strongest reasoning model, long budget
- repair/jsonizer model: smaller, faster, structure-focused model
- judge model: smaller or medium model, safety/grounding focused
- optional fallback planner: alternate strong model or deployment

This is materially more useful than adding more graph roles right now.

## Theoretical Fit By Model Type

For this use case, not all "helper models" solve the same problem.

The practical split is:

1. planner models

- generative causal LMs
- appropriate for:
  - understanding incident context
  - generating remediation hypotheses
  - producing a candidate mitigation plan
- these are the models that should receive the long budget

2. repair / jsonizer models

- generative models, usually smaller
- appropriate for:
  - converting a candidate plan into strict JSON
  - repairing malformed but semantically useful output
- these do not replace the planner; they reduce structure fragility

3. reward / judge models

- usually classification-style models rather than chat-completion models
- appropriate for:
  - scoring a completed candidate
  - ranking multiple candidate plans
  - gating acceptance offline or in a sidecar step
- important limitation:
  - many are not drop-in replacements for `OpenAI Chat Completions`
  - they often require sequence-classification style integration

4. rerankers

- ranking models, not planners
- appropriate for:
  - ranking candidate evidence chunks
  - ranking candidate retrieval outputs
  - ranking multiple remediation candidates after generation
- they should not be treated as mitigation planners

5. coder models

- generative code-specialized LMs
- appropriate for:
  - the later `patch_application` stage
  - code diffs, patch suggestions, and implementation review
- they are not the first model to optimize for the current `fix_planning`
  bottleneck

## Current Internet-Sourced Model Inventory

This shortlist is based on official Hugging Face model cards and metadata current
as of `2026-04-12`.

### Shortlist By Operational Function

| Function | Candidate model | Why it matters here | Practical note | Source |
| --- | --- | --- | --- | --- |
| primary planner | `Qwen/Qwen3-30B-A3B-Instruct-2507` | strong general instruct planner candidate for long-budget reasoning | generative `AutoModelForCausalLM`, Apache-2.0, large MoE-style model | [card](https://hf.co/Qwen/Qwen3-30B-A3B-Instruct-2507) |
| primary planner | `deepseek-ai/DeepSeek-R1-Distill-Qwen-32B` | reasoning-oriented planner candidate if we want stronger chain-of-thought style behavior | generative `AutoModelForCausalLM`, MIT, likely heavier latency profile | [card](https://hf.co/deepseek-ai/DeepSeek-R1-Distill-Qwen-32B) |
| alternate planner | `mistralai/Mistral-Small-3.2-24B-Instruct-2506` | viable alternate planner family if we want family diversity and a vLLM-oriented distribution | model card is explicitly tagged for `vllm`; Apache-2.0 | [card](https://hf.co/mistralai/Mistral-Small-3.2-24B-Instruct-2506) |
| repair / jsonizer | `Qwen/Qwen3-4B-Instruct-2507` | smaller instruct model for JSON repair and structured retries | generative `AutoModelForCausalLM`, Apache-2.0, much lighter than the planner tier | [card](https://hf.co/Qwen/Qwen3-4B-Instruct-2507) |
| reranker | `Qwen/Qwen3-Reranker-4B` | heavier reranking option for evidence or candidate-plan ranking | ranking model, not planner; `text-ranking` task | [card](https://hf.co/Qwen/Qwen3-Reranker-4B) |
| reranker | `Qwen/Qwen3-Reranker-0.6B` | cheaper reranking option for low-latency candidate ordering | ranking model, not planner; smallest operational reranker in the shortlist | [card](https://hf.co/Qwen/Qwen3-Reranker-0.6B) |
| reward / judge | `Skywork/Skywork-Reward-V2-Qwen3-8B` | strong candidate for acceptance scoring or pairwise plan ranking | `AutoModelForSequenceClassification`; not a drop-in chat model | [card](https://hf.co/Skywork/Skywork-Reward-V2-Qwen3-8B) |
| code / patch | `Qwen/Qwen3-Coder-30B-A3B-Instruct` | strong patch-stage candidate once `fix_planning` is stable | reserve for `patch_application`, not first fix-planning bottleneck | [card](https://hf.co/Qwen/Qwen3-Coder-30B-A3B-Instruct) |
| code / patch | `Qwen/Qwen2.5-Coder-14B-Instruct` | lighter patch-stage candidate if `30B` is too expensive | generative code model with lower serving cost | [card](https://hf.co/Qwen/Qwen2.5-Coder-14B-Instruct) |

### Practical Fichas

#### Ficha 1: `Qwen/Qwen3-30B-A3B-Instruct-2507`

- class: `AutoModelForCausalLM`
- architecture: `qwen3_moe`
- role fit: best current shortlist candidate for the long-budget primary planner
- integration fit: high for the current `planner` slot
- risk: large model; budget and serving cost must be accepted
- source: [https://hf.co/Qwen/Qwen3-30B-A3B-Instruct-2507](https://hf.co/Qwen/Qwen3-30B-A3B-Instruct-2507)

#### Ficha 2: `deepseek-ai/DeepSeek-R1-Distill-Qwen-32B`

- class: `AutoModelForCausalLM`
- architecture: `qwen2`
- role fit: reasoning-heavy fallback or alternate planner
- integration fit: high for long-form planning experiments
- risk: likely slower than smaller instruct baselines; may increase latency further
- source: [https://hf.co/deepseek-ai/DeepSeek-R1-Distill-Qwen-32B](https://hf.co/deepseek-ai/DeepSeek-R1-Distill-Qwen-32B)

#### Ficha 3: `mistralai/Mistral-Small-3.2-24B-Instruct-2506`

- class: instruct generative model
- architecture: `mistral3`
- role fit: alternate planner family for model diversity
- integration fit: attractive because the official card is tagged for `vllm`
- risk: family switch means prompt behavior and structured-output behavior must be revalidated
- source: [https://hf.co/mistralai/Mistral-Small-3.2-24B-Instruct-2506](https://hf.co/mistralai/Mistral-Small-3.2-24B-Instruct-2506)

#### Ficha 4: `Qwen/Qwen3-4B-Instruct-2507`

- class: `AutoModelForCausalLM`
- architecture: `qwen3`
- role fit: repair/jsonizer or cheap bounded retry assistant
- integration fit: high for structure repair because it preserves the same family
- risk: should not be mistaken for the main remediation planner on complex incidents
- source: [https://hf.co/Qwen/Qwen3-4B-Instruct-2507](https://hf.co/Qwen/Qwen3-4B-Instruct-2507)

#### Ficha 5: `Skywork/Skywork-Reward-V2-Qwen3-8B`

- class: `AutoModelForSequenceClassification`
- architecture: `qwen3`
- role fit: acceptance scoring, pairwise ranking, or offline evaluation
- integration fit: medium, because it requires classifier-style integration
- risk: not a drop-in replacement for current chat-completions-based judge logic
- source: [https://hf.co/Skywork/Skywork-Reward-V2-Qwen3-8B](https://hf.co/Skywork/Skywork-Reward-V2-Qwen3-8B)

#### Ficha 6: `Qwen/Qwen3-Reranker-4B`

- class: ranking model
- architecture: `qwen3`
- role fit: ranking retrieval evidence or multiple candidate plans
- integration fit: useful sidecar, not primary planner
- risk: adds ranking quality, but will not solve the first `CompleteJSON` timeout directly
- source: [https://hf.co/Qwen/Qwen3-Reranker-4B](https://hf.co/Qwen/Qwen3-Reranker-4B)

#### Ficha 7: `Qwen/Qwen3-Reranker-0.6B`

- class: ranking model
- architecture: `qwen3`
- role fit: low-cost reranking where `4B` is too expensive
- integration fit: high for cheap ordering tasks
- risk: less likely than `4B` to help on nuanced remediation ranking
- source: [https://hf.co/Qwen/Qwen3-Reranker-0.6B](https://hf.co/Qwen/Qwen3-Reranker-0.6B)

#### Ficha 8: `Qwen/Qwen3-Coder-30B-A3B-Instruct`

- class: `AutoModelForCausalLM`
- architecture: `qwen3_moe`
- role fit: later `patch_application` worker
- integration fit: high for code-generation stages, low for current `fix_planning` bottleneck
- risk: using it now would optimize the wrong stage
- source: [https://hf.co/Qwen/Qwen3-Coder-30B-A3B-Instruct](https://hf.co/Qwen/Qwen3-Coder-30B-A3B-Instruct)

#### Ficha 9: `Qwen/Qwen2.5-Coder-14B-Instruct`

- class: `AutoModelForCausalLM`
- architecture: `qwen2`
- role fit: lighter patch-stage alternative
- integration fit: useful if `30B` coder serving cost is too high
- risk: still not the first model to optimize while `fix_planning` remains planner-bound
- source: [https://hf.co/Qwen/Qwen2.5-Coder-14B-Instruct](https://hf.co/Qwen/Qwen2.5-Coder-14B-Instruct)

### What This Means For Our Use Case

Immediate takeaway:

- the next `fix_planning` iteration should still be planner-first
- the best model candidates to test after budget expansion are:
  - `Qwen/Qwen3-30B-A3B-Instruct-2507`
  - `deepseek-ai/DeepSeek-R1-Distill-Qwen-32B`
  - `mistralai/Mistral-Small-3.2-24B-Instruct-2506`

Best support-model candidates after that:

- repair/jsonizer: `Qwen/Qwen3-4B-Instruct-2507`
- reranker: `Qwen/Qwen3-Reranker-0.6B` or `Qwen/Qwen3-Reranker-4B`
- reward/judge sidecar: `Skywork/Skywork-Reward-V2-Qwen3-8B`

Important practical conclusion:

- reward and reranker models are useful support components
- they are not substitutes for giving the planner enough time
- coder models belong to the next stage family, not the current planner bottleneck

## Iteration Series

### Iteration A: Long-Budget Single-Model Baseline

Goal:

- prove that `fix_planning` can use a serious wall-clock budget on the existing
  single-model path

Changes:

- raise `fix_planning` task budget to `20m`
- raise retries to `4`
- raise smoke timeout above the task budget
- raise JetStream `AckWait` with safety margin
- add per-attempt timing and outcome logging

Do not change yet:

- prompt structure
- token budget
- graph read shape
- model routing

Exit condition:

- one live smoke completes successfully, or
- one live smoke exhausts the full long-budget policy and escalates truthfully
  with clear per-attempt evidence

### Iteration A1: Observability And Repeatability

Trigger for this iteration:

- `A0` has already produced at least one truthful live `.completed`

Goal:

- convert one successful live smoke into a measurable, repeatable operating
  baseline for the current deployment

Required changes:

1. expose first-class LLM observability for:
   - `prompt_tokens`
   - `completion_tokens`
   - `finish_reason`
   - per-operation call count
   - per-operation latency
2. keep raw `llm_traces` as forensic evidence, but do not rely on them as the
   only observability surface
3. run repeated live smokes against the same deployed `A0` configuration and
   capture them with the same ficha
4. preserve the successful `generate` token ceiling and thinking-mode split that
   produced the first live completion

Why this now matters:

- one `.completed` proves feasibility
- it does not yet prove stability
- we cannot judge whether to stay on `Qwen3.5-9B` or move to `A1-A3` without
  seeing token usage, finish reasons, and repeatability patterns across runs

Exit condition:

- `3/5` consecutive truthful live smokes are captured for the deployed `A0`
  configuration, with token and finish-reason observability available for every
  LLM call

Current status:

- completed on the `2026-04-12` live sample
- first-class LLM observability is live
- deployed `A0` achieved `5/5` truthful `.completed` smokes with valid
  read-back in the first repeatability pass

### Iteration A2: Failure-Path Idempotency

Trigger for this iteration:

- any failure path still causes redelivery or invalid terminal state transitions

Goal:

- make `.failed` / `.escalated` terminal handling idempotent and operationally
  quiet

Required changes:

- stop `.escalated -> escalated` reconsumption from reaching DLQ
- make terminal stage-result handling safe under duplicate delivery

This is still part of the `A0` hardening series because truthful failure is a
core property of the first real agent.

### Iteration B: Fallback Model Expansion

Trigger for this iteration:

- only if Iteration A still fails after the model has been given a real time
  budget

Goal:

- preserve the same graph-local agent path, but allow alternate model help

Candidate policy:

- attempt 1-2: primary planner model
- attempt 3: alternate planner model or deployment
- attempt 4: final bounded retry with full prior rejection context

The point is not to add more reasoning layers. The point is to reduce single
deployment fragility.

### Iteration C: Structured Multi-Model Specialization

Trigger for this iteration:

- only if fallback routing still leaves reliability problems

Goal:

- separate heavy planning from structure repair and judgment

Candidate split:

- planner: long-budget reasoning
- repair/jsonizer: fast structured output repair
- judge: bounded acceptance review

This should happen only after the main planner has already been given enough
time. Otherwise we will still be optimizing the wrong bottleneck.

### Iteration D: Role Differentiation Only If Needed

Trigger for this iteration:

- only if live evidence shows that role-specific graph retrieval materially
  changes planner quality

Until then:

- keep the current `fix-planning-agent` role
- avoid role proliferation without evidence

## Required PIR Changes For Iteration A

Minimum implementation set:

1. raise `fix_planning` stage execution budget to the new long-budget baseline
2. raise `fix_planning` retry count to `4`
3. rebalance attempt reservation logic around the longer wall-clock budget
4. raise smoke default timeout above the new task budget
5. raise JetStream consumer `AckWait` above the new task budget
6. log per-attempt:
   - start time
   - end time
   - duration
   - failure class
   - rejection reason
   - model used
7. keep truthful completion / escalation semantics unchanged

## Required PIR Changes For Iteration A1

Minimum implementation set:

1. expose `llm_calls_total`
2. expose `llm_prompt_tokens_total`
3. expose `llm_completion_tokens_total`
4. expose `llm_finish_reason_total`
5. expose LLM latency by operation and model
6. include `operation`, `model`, and `stage` labels at minimum
7. document how these metrics map back to the `fix_planning` planner, repair,
   and judge calls
8. keep `llm_traces` as the source of truth for forensic payload inspection

## Required PIR Changes For Iteration A2

Minimum implementation set:

1. make terminal stage-result application idempotent
2. prevent duplicate `.escalated` deliveries from creating invalid
   `escalated -> escalated` transitions
3. keep the smoke harness terminal-outcome aware

## Success Criteria For This Series

This series is successful when all of these become true:

- `fix_planning` gets a real autonomy budget in live execution
- the system can tolerate a long-running stage without transport-level
  redelivery fighting the executor
- retries are numerous enough to be operationally meaningful
- truthful escalation remains intact when the planner still cannot complete
- one live completion is followed by observable and repeatable behavior, not
  just a one-off success
- the next evidence report shows either:
  - repeated real `.completed` decision waves with kernel read-back and LLM
    observability, or
  - a full multi-attempt failure with enough time spent to justify escalation

## Explicit Non-Goals

For this next series, do not frame the work as:

- “make it faster at any cost”
- “fit the planner into the old 45s envelope”
- “shrink context first”
- “cut prompt or token budget first”

Those may become later optimizations, but not before the agent has had a fair
chance to succeed.

## Recommended Immediate Next Step

Implement Iteration A first:

- longer task budget
- more retries
- longer consumer ack window
- longer smoke timeout

Then run a new live smoke and write a fresh evidence report against that policy
before deciding whether fallback-model routing is actually necessary.
