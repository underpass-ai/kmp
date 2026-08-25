# PIR First Event-Driven Agent Plan

Status: planned for next session
Date: 2026-04-12
Scope: first real event-driven agent on top of the already-live `PIR -> kernel`
adapter

## Intent

Define the next implementation slice after the current stage-driven
`PIR -> kernel` integration.

This plan deliberately pivots away from squeezing more value out of the
root-centric smoke path. That path has already proven the adapter, the retry
discipline, the late operational waves, the reranker integration, and the
truthful root state.

The next useful proof is different:

- one agent
- triggered by real events
- reading a local graph neighborhood rather than the whole incident from the
  root
- using runtime in the same loop
- publishing one new wave back to the kernel

## Why Pivot Now

The current root-centric proof has reached diminishing returns.

What it already proved:

- stable incident identity
- async publish path
- gRPC read path
- reranker-assisted semantic classification
- truthful root state after late waves
- useful end-to-end graph materialization for a real PIR run

What it does not prove well anymore:

- graph-local reasoning by an agent
- event-driven task ownership
- per-agent query shape and token policy
- per-agent iteration and retry policy
- how the system behaves when an agent cannot resolve its task within budget

That is why the next slice should not be “more root-centric graph tests”.
It should be the first event-driven agent.

## Non-Goals For This Slice

This slice is intentionally narrow.

Do not attempt yet:

- a general frontier/resume framework
- multiple event-driven agents
- a complete event-driven rewrite of PIR
- autonomous diagnosis claims
- autonomous mitigation claims
- removing all root-direct edges
- solving token growth globally for every future agent

Those are later concerns. This slice is about proving one clean vertical path.

## Chosen First Agent

Recommended first agent:

- `fix_planning`

Reason:

- it sits at the right point in the causal chain
- it naturally depends on graph-local evidence
- it produces a semantically meaningful output:
  - a `decision`
  - `ADDRESSES`
  - `BASED_ON`
- it is easier to reason about than `triage`
- it is less operationally noisy than `deploy` or `recovery_confirmation`

## Trigger Shape

Entry event:

- `payments.incident.fix-planning.requested`

Trigger contract for the agent:

- `incident_run_id`
- `task_id`
- stage metadata already present in PIR

What changes in this slice:

- instead of reading context primarily from the root incident bundle, the agent
  should start from the local graph neighborhood relevant to the task

## Graph Read Strategy

The agent should not start by asking for the whole incident narrative.

Initial read target:

- `finding:{incident_id}:triage`
- `evidence:{incident_id}:rehydration`

Desired local graph shape consumed by the agent:

- node: triage finding
- node: rehydration evidence
- relation: `incident -HAS_FINDING-> finding`
- relation: `incident -HAS_CAUSE_CONTEXT-> rehydration_evidence`
- relation: `rehydration_evidence -SUPPORTS-> finding`

Practical read rule for this slice:

- keep the root incident id as the stable anchor for lookup
- request only the scopes and depth needed to assemble the local subgraph for
  `fix_planning`
- do not ask the LLM to consume the full incident render if a smaller local
  render can do the job

Initial proposed query profile:

- `role`: `fix-planning-agent`
- `requested_scopes`: `graph`, `details`
- `depth`: `2`
- `token_budget`: `1536`
- `rehydration_mode`: `reason_preserving`

Reason:

- enough for the current local chain
- smaller than the root-centric smoke
- still faithful to graph semantics

## Runtime In The Same Loop

The runtime should be part of this first agent slice, not a separate isolated
exercise.

Required runtime behavior to prove:

- create session
- request tool recommendation
- keep runtime metadata attached to the task execution
- complete the task with runtime and graph context in the same loop

This slice should produce evidence that one event-driven agent can do:

`event -> runtime session -> local graph read -> LLM reasoning -> publish wave`

That is much more meaningful than another runtime-only smoke.

## Output Shape

Expected wave output from the first agent:

- node: `decision:{incident_id}:fix-planning`
- root edge: `incident -MITIGATED_BY-> decision`
- non-root edge: `decision -ADDRESSES-> finding`
- non-root edge: `decision -BASED_ON-> rehydration_evidence`
- `node_detail` with:
  - decision summary
  - rationale
  - tradeoff or risk statement if present
  - concrete action proposal if present

## Iteration Policy For This Slice

This is the first place where we need an explicit iteration budget.

Provisional rule:

- `max_iterations_per_task = 2`

Interpretation:

- one primary attempt from the local graph context
- one bounded retry if the first attempt is malformed, incomplete, or fails a
  local acceptance check

Important safety rule:

- if the agent reaches max iterations without a valid outcome, the incident must
  not be marked `resolved`
- the task result is `unresolved`
- the run should move to `escalated` or `suspended`, depending on operational
  policy

For this first slice, the simpler policy is:

- unresolved after max iterations -> `escalated`

Reason:

- it is operationally safer than silently suspending by default
- it gives us a clear stop condition for the first event-driven path

## Retry Policy For This Slice

We already have separate transport retries in the system.

For this agent slice, keep retries separated into three domains:

1. Graph read retry

- budget: existing gRPC retry discipline
- errors: `NotFound`, `Unavailable`, `DeadlineExceeded`

2. Semantic reranker retry

- keep current bounded retry policy
- reranker remains advisory to the publish path

3. Agent iteration retry

- this is not transport retry
- this is one bounded second attempt to complete the task

Do not mix these three into one opaque retry loop.

## Timing Budget For This Slice

We need a task-level wall-clock budget now.

Provisional timing policy:

- runtime/session setup: `<= 5s`
- graph read: `<= 5s`
- primary LLM attempt: `<= 20s`
- optional second attempt: `<= 20s`
- reranker + publish: `<= 10s`
- total task wall-clock budget: `<= 45s`

If the task exceeds the total budget:

- mark the task `unresolved`
- escalate the incident
- do not emit a false success wave

This is intentionally conservative. We can tune later with evidence.

## Acceptance Checks

The agent result should be accepted only if:

- a decision node is produced
- the node validates as `GraphBatch`
- the graph remains connected from the incident root
- the decision has at least:
  - one `ADDRESSES` edge
  - one `BASED_ON` edge
- the detail payload is not empty
- the output does not require transport-level repair to become structurally
  valid

If any of those checks fail:

- consume one iteration
- try again if budget remains
- otherwise escalate

## Evidence Required

To consider this slice successful, capture:

1. event trace
   - requested event
   - task id
   - incident run id
2. runtime evidence
   - session id
   - recommendation id
   - selected tools if any
3. graph read evidence
   - node count
   - detail count
   - content hash
   - token budget used
4. publish evidence
   - relation types emitted
   - semantic classes emitted
   - reranker changes if any
5. kernel read-back evidence
   - decision node visible
   - `ADDRESSES` and `BASED_ON` visible
6. failure evidence if the task escalates
   - iteration count
   - why unresolved
   - wall-clock budget consumed

## Exit Criteria

This slice is complete when all of these are true:

- one real `fix_planning` task runs as an event-driven agent
- runtime is exercised in the same path
- the agent reads local graph context rather than relying on whole-incident
  root-centric rendering
- the agent publishes a valid decision wave back to the kernel
- the kernel read-back shows the expected decision semantics
- failure after max iterations leads to `unresolved + escalated`, not false
  resolution

## What This Slice Still Will Not Prove

Even if this slice passes, it still will not prove:

- autonomous diagnosis of the incident
- autonomous end-to-end resolution
- the final optimal graph shape
- the final frontier/resume abstraction
- that truncation has been replaced with a complete budget policy for all
  future agents

That is acceptable. This slice is about getting the first event-driven loop
correct.

## Implementation Checklist For Tomorrow

1. Define the exact event-driven `fix_planning` path in PIR.
2. Reuse runtime inside that path instead of adding a parallel runtime-only
   harness.
3. Implement local graph read for the agent with bounded scopes/depth/budget.
4. Implement `max_iterations_per_task = 2`.
5. Implement task-level wall-clock budget.
6. On max-iteration or timeout failure:
   - do not mark resolved
   - mark unresolved
   - escalate
7. Publish the decision wave back to the kernel.
8. Re-read the kernel and verify the decision graph shape.
9. Write one evidence report for the run.

## Session Handoff

If we resume tomorrow, the first question should not be architectural again.
The architecture choice is already made for the next slice:

- first event-driven agent
- runtime included
- root-centric whole-incident read no longer the main proof path

The first practical step tomorrow should be:

- implement the event-driven `fix_planning` slice in `PIR`

Everything else in this document is there to keep that implementation bounded,
honest, and testable.
