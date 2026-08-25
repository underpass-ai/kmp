# PIR Fix Planning Experiment Matrix

> **Historical (superseded).** Point-in-time record from the PIR `fix_planning` specialist program, which has since been decommissioned and restructured (`internal/application/{saturation*,paymentintegrity}`). Code paths and tooling referenced below (`internal/specialists/*`, `cmd/fixplanning-*-smoke`, `fixplanning-deployed-smoke-scenarios.yaml`, `llm_trace_repo.go`) were refactored or removed after this was written.

Status: Superseded (PIR fix_planning program decommissioned; matrix never advanced past Phase A/D1)
Date: 2026-04-12
Scope: controlled experiment matrix for model, retry, and long-budget policy
selection around the first real `PIR` `fix_planning` agent

Companion documents:

- [`pir-fix-planning-long-budget-retry-plan.md`](pir-fix-planning-long-budget-retry-plan.md)
- [`pir-fix-planning-model-research-notes.md`](pir-fix-planning-model-research-notes.md)
- [`pir-fix-planning-live-smoke-failure-report-2026-04-12.md`](pir-fix-planning-live-smoke-failure-report-2026-04-12.md)

## Goal

Find the smallest practical model architecture and runtime policy that can make
`fix_planning` complete autonomously and truthfully under a production-grade
incident budget.

This matrix is not trying to optimize for:

- the lowest latency
- the smallest token count
- the smallest prompt

It is trying to optimize for:

- real completion probability
- truthful failure when autonomy still does not work
- operational clarity about what changed and why

## Current Status

`A0` has now achieved a real live pass on the deployed `PIR` path:

- `payments.incident.fix-planning.completed`
- valid kernel read-back
- stage advancement to `patch_application`

That means the matrix no longer starts from "can this work at all?".

It now starts from:

- "can the current deployed planner do this repeatably and observably?"

## Fixed Variables For Phase 1

These stay fixed across the first experiment wave unless explicitly noted:

- kernel query role: `fix-planning-agent`
- scopes: `graph`, `details`
- depth: `2`
- kernel token budget: `1536`
- rehydration mode: `reason_preserving`
- current output schema and acceptance rules
- current event-driven path:
  `requested -> runtime -> local graph read -> LLM -> publish -> stage result`
- truthful escalation behavior on unresolved outcomes

This matters because the next wave should answer:

- "which planner and budget work?"

not:

- "which combination of prompt, retrieval, graph shape, and model worked by
  accident?"

## Operational Baseline For This Matrix

All active experiments in this matrix assume the long-budget policy family:

- task wall-clock budget: `20m`
- retries: `4`
- JetStream `AckWait`: `>= 25m`
- smoke timeout: `>= 25m`
- publish/read-back reserve must remain intact

Unless explicitly overridden, the intended attempt shape is:

- attempt 1: planner-heavy
- attempt 2: planner-heavy with prior rejection context
- attempt 3: planner-heavy or fallback planner
- attempt 4: final bounded retry or fallback planner

## Metrics To Capture In Every Run

Every experiment run should capture the same evidence fields:

1. control fields

- experiment id
- planner model
- support models
- retry count
- task budget
- incident_run_id
- task_id

2. stage outcome

- published event type
- completed / failed / escalated
- total wall-clock
- run final status
- task final status

3. attempt evidence

- number of attempts actually used
- duration per attempt
- failure reason per attempt
- did generation return at all
- did JSON parse on first try
- did repair run
- did judge run

4. kernel truthfulness evidence

- was a decision node materialized
- were `ADDRESSES` and `BASED_ON` present
- if failed, was no false decision node published

5. operational evidence

- serving footprint class
- integration complexity
- whether the run hit transport constraints

6. LLM observability evidence

- prompt tokens per operation
- completion tokens per operation
- finish reason per operation
- latency per operation
- whether the values were visible as first-class metrics or only as raw traces

## Promotion Gates

### Gate 1: Lab Pass

An experiment is worth continuing only if it achieves at least one of:

- `1/1` successful live smoke
- or one truthful escalated live smoke that clearly used the intended long budget

If the run still fails quickly for the same reason as the old short-budget path,
the experiment has not tested anything meaningful.

### Gate 2: Candidate Pass

A candidate configuration is worth keeping only if:

- at least `3/5` consecutive live smokes complete successfully
- no false `.completed` is observed
- every failed run remains truthful

### Gate 3: Pre-Production Pass

A candidate is worth carrying forward only if:

- it remains truthful across all failures
- it shows stable successful read-back semantics
- it does not require hidden operator intervention during the run

## Cost Classes

This matrix uses coarse operational cost classes rather than fake precision:

- `L`: low serving footprint, low integration complexity
- `M`: moderate serving footprint or one additional sidecar
- `H`: large model or expensive sidecar topology
- `VH`: multi-large-model topology or complex multi-model routing

## Model-Role Shortlist

These are the current working candidates from the research note:

| Role | Primary candidate | Alternate candidate | Notes |
| --- | --- | --- | --- |
| planner | [Qwen/Qwen3-30B-A3B-Instruct-2507](https://hf.co/Qwen/Qwen3-30B-A3B-Instruct-2507) | [deepseek-ai/DeepSeek-R1-Distill-Qwen-32B](https://hf.co/deepseek-ai/DeepSeek-R1-Distill-Qwen-32B) | strongest planner-focused candidates in the current shortlist |
| planner alternate family | [mistralai/Mistral-Small-3.2-24B-Instruct-2506](https://hf.co/mistralai/Mistral-Small-3.2-24B-Instruct-2506) | none yet shortlisted | useful for family diversity and `vllm`-friendly serving |
| repair / jsonizer | [Qwen/Qwen3-4B-Instruct-2507](https://hf.co/Qwen/Qwen3-4B-Instruct-2507) | current planner in self-repair mode | use only after long-budget planner baseline exists |
| reranker | [Qwen/Qwen3-Reranker-0.6B](https://hf.co/Qwen/Qwen3-Reranker-0.6B) | [Qwen/Qwen3-Reranker-4B](https://hf.co/Qwen/Qwen3-Reranker-4B) | ranking support, not planner replacement |
| reward / judge | [Skywork/Skywork-Reward-V2-Qwen3-8B](https://hf.co/Skywork/Skywork-Reward-V2-Qwen3-8B) | none yet shortlisted | requires classifier-style integration |
| patch stage coder | [Qwen/Qwen3-Coder-30B-A3B-Instruct](https://hf.co/Qwen/Qwen3-Coder-30B-A3B-Instruct) | [Qwen/Qwen2.5-Coder-14B-Instruct](https://hf.co/Qwen/Qwen2.5-Coder-14B-Instruct) | belongs to `patch_application`, not first planner bottleneck |

## Experiment Matrix

### Phase A: Long-Budget Single-Planner Baseline

Goal:

- isolate the value of more time and more retries before adding sidecars

| ID | Planner | Support models | Policy | Cost | Hypothesis | Promote if | Reject if |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `A0` | current deployed `Qwen/Qwen3.5-9B` | none | `20m`, `4` retries | `M` | long budget alone may be enough for the current model | one real `.completed` with valid read-back | still dies early or times out on every attempt with no qualitative improvement |
| `A1` | [Qwen/Qwen3-30B-A3B-Instruct-2507](https://hf.co/Qwen/Qwen3-30B-A3B-Instruct-2507) | none | `20m`, `4` retries | `H` | stronger planner quality may unlock success without extra topology | `3/5` truthful runs with at least one successful completion early | no improvement over `A0` despite larger budget and stronger planner |
| `A2` | [deepseek-ai/DeepSeek-R1-Distill-Qwen-32B](https://hf.co/deepseek-ai/DeepSeek-R1-Distill-Qwen-32B) | none | `20m`, `4` retries | `H` | reasoning-oriented planner may outperform general instruct planner on remediation planning | better completion rate or better near-success quality than `A1` | slower but not better than `A1` |
| `A3` | [mistralai/Mistral-Small-3.2-24B-Instruct-2506](https://hf.co/mistralai/Mistral-Small-3.2-24B-Instruct-2506) | none | `20m`, `4` retries | `H` | planner-family diversity may beat Qwen-family failure modes | clear improvement in structure reliability or completion rate | no meaningful improvement over Qwen-family baselines |

Current note for `A0`:

- Gate 1 is now satisfied
- Gate 2 is now also satisfied on the first five-run live sample
- `A0.2` duplicate-delivery hardening is now complete for both `.escalated` and
  retryable `.failed`
- `A0.3` execution evidence is now exposed directly in deployed smoke output
- named YAML scenarios now exist for widening `A0` coverage
- `A0.4` has now produced `3/3` truthful completions across the first widened
  YAML scenario batch
- the next decision is still not "switch models now"
- the next decision is whether wider scenario coverage keeps `A0` convincing
  enough to delay `A1`
- after the first valid `D1` run, `A0` still remains the operational baseline
  because `D1` added diagnostic value but did not improve acceptance

### Phase B: Planner Plus Structure Repair

Goal:

- reduce structure failure without changing the planner's reasoning role

Run this phase only after choosing the best planner from Phase A.

| ID | Planner | Support models | Policy | Cost | Hypothesis | Promote if | Reject if |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `B1` | best of `A1-A3` | [Qwen/Qwen3-4B-Instruct-2507](https://hf.co/Qwen/Qwen3-4B-Instruct-2507) as repair-only sidecar | `20m`, `4` retries | `H` | a small same-family repair model can reduce malformed-output loss | malformed JSON rate drops materially while success rate rises | sidecar adds complexity without reducing escalation rate |
| `B2` | best of `A1-A3` | same repair sidecar only on retries `2-4` | `20m`, `4` retries | `M` | repair should help most after the first planner failure, not before | better cost/performance than `B1` | no better than single-planner baseline |

### Phase C: Planner Fallback Routing

Goal:

- reduce dependency on one planner deployment or one model family

Run this phase only if Phase A or B still leaves significant failure.

| ID | Planner | Support models | Policy | Cost | Hypothesis | Promote if | Reject if |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `C1` | best planner from `A/B` on attempts `1-2` | alternate-family planner on attempts `3-4` | `20m`, `4` retries | `VH` | failures may be family-specific, not just budget-specific | late attempts recover runs that the primary planner misses | extra routing adds latency and operational burden with no recovery |
| `C2` | best Qwen-family planner on attempts `1-2` | `DeepSeek-R1-Distill-Qwen-32B` on attempts `3-4` | `20m`, `4` retries | `VH` | reasoning-heavy fallback may rescue difficult causal plans | hard incidents complete more often than in `C1` or `A` | fallback is slower and still not useful |

### Phase D: Ranking And Reward Sidecars

Goal:

- improve acceptance quality only after planner success becomes plausible

Run this phase only after at least one planner configuration already produces
successful completions.

| ID | Planner | Support models | Policy | Cost | Hypothesis | Promote if | Reject if |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `D1` | best planner from earlier phases | [Qwen/Qwen3-Reranker-0.6B](https://hf.co/Qwen/Qwen3-Reranker-0.6B) | `20m`, `4` retries | `M` | cheap reranking may improve evidence or candidate ordering | acceptance quality rises with small operational overhead | no measurable difference in accepted plans |
| `D2` | best planner from earlier phases | [Qwen/Qwen3-Reranker-4B](https://hf.co/Qwen/Qwen3-Reranker-4B) | `20m`, `4` retries | `H` | heavier reranking may outperform the cheap reranker | materially better ranking signal than `D1` | more cost with no quality gain |
| `D3` | best planner from earlier phases | [Skywork/Skywork-Reward-V2-Qwen3-8B](https://hf.co/Skywork/Skywork-Reward-V2-Qwen3-8B) as acceptance sidecar | `20m`, `4` retries | `H` | reward scoring may outperform chat-judge heuristics | fewer false acceptances without harming completion rate | classifier integration cost exceeds quality gain |

Current note for `D1`:

- the first valid live `D1` run ended in truthful escalation, not completion
- raw `llm_traces` show valid JSON and `response_code=200` on all planner,
  reranker, and judge calls
- the failure surface was semantic:
  - rollback plan too vague or operationally unsafe
  - patch still too generic on one attempt
  - reranker showed strong `finding` alignment but weak `evidence` alignment
- the next best move is not another reranker experiment first
- the next best move is planner-contract hardening that benefits both `A0` and
  `D1`, then re-testing `A0` before retrying `D1`

## Deferred Or Explicitly Out-Of-Scope Experiments

These should not be run before the earlier phases:

| ID | Deferred experiment | Why deferred |
| --- | --- | --- |
| `X1` | extra graph roles beyond `fix-planning-agent` | current live evidence does not show role divergence as the main bottleneck |
| `X2` | aggressive prompt shrinking | we have not yet given the planner a fair time budget |
| `X3` | token-budget reduction | same reason as `X2` |
| `X4` | coder-model substitution inside `fix_planning` | coder models address later patch stages, not the current planner bottleneck |
| `X5` | reward-model-first architecture | reward models score candidates; they do not generate the mitigation plan |

## Recommended Execution Order

Run the first wave in this order:

1. `A0.2` duplicate-delivery and terminal-path hardening
2. `A0.3` execution evidence plus YAML scenario coverage
3. keep the current deployed planner fixed while validating the cleaner path
4. only if `A0` regresses or cannot hold the result, move to `A1`
5. then `A2`
6. then `A3`
7. choose best planner
8. `B2`
9. only if still needed, `C1` then `C2`
10. only after planner success exists, `D1` or `D3`

This ordering matters because it isolates variables:

- first budget
- then planner
- then structure repair
- then fallback routing
- then ranking or judging

## Run Ficha Template

Every live run should be captured in the same compact ficha:

```text
experiment_id:
date:
planner_model:
support_models:
task_budget:
retry_count:
ack_wait:
smoke_timeout:
incident_run_id:
task_id:
published_event:
run_status:
task_status:
attempts_used:
per_attempt_durations:
per_attempt_failures:
decision_node_materialized:
addresses_edge_present:
based_on_edge_present:
truthful_failure_preserved:
notes:
```

## Immediate Next Recommendation

Stay on `A0` for one more hardening iteration.

Immediate next task:

- harden the common planner-quality layer first:
  - stronger `fix_planning` schema
  - deterministic local rollback validation
  - regression fixture from the first valid `D1` failure
  - then re-test `A0`
  - only then re-test `D1`

Reason:

- `A0` has already shown `5/5` truthful `.completed` live runs in the first
  repeatability pass
- `A0` also passed the first widened YAML scenario batch
- `D1` produced diagnostic value but not better acceptance
- the next highest-return work is to strengthen planner output quality before
  changing model size or adding more support topology

Only if `A0` fails Gate 2 after that should we spend time deploying larger
planner candidates.
