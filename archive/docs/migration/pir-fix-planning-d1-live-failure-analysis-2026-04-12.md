# PIR Fix Planning D1 Live Failure Analysis

> **Historical (superseded).** Point-in-time record; the PIR `fix_planning` specialist program was decommissioned and the `internal/specialists/*`, `cmd/fixplanning-*-smoke`, and `fixplanning-deployed-smoke-scenarios.yaml` code paths/links below were refactored/removed after this was written.

Date: `2026-04-12`

Status: failed truthfully

Scope: first valid live `D1` smoke against the deployed `pir` consumer after
adding the advisory grounding reranker sidecar on top of the successful `A0`
planner baseline.

Primary deployment under test:

- deployment: `pir`
- namespace: `underpass-runtime`
- image:
  `ghcr.io/underpass-ai/payments-incident-response:v0.4.16-d1-reranker-20260412T202020Z`
- planner model: `Qwen/Qwen3.5-9B`
- reranker model: `Qwen/Qwen3-Reranker-0.6B`
- reranker threshold: `0.2`

Primary code under test:

- [`underpass-payments-incident-response/internal/specialists/strategies.go`](../../../../underpass-payments-incident-response/internal/specialists/strategies.go)
- [`underpass-payments-incident-response/internal/specialists/executor.go`](../../../../underpass-payments-incident-response/internal/specialists/executor.go)
- [`underpass-payments-incident-response/internal/adapters/persistence/postgres/llm_trace_repo.go`](../../../../underpass-payments-incident-response/internal/adapters/persistence/postgres/llm_trace_repo.go)
- [`underpass-payments-incident-response/cmd/fixplanning-deployed-smoke/main.go`](../../../../underpass-payments-incident-response/cmd/fixplanning-deployed-smoke/main.go)

## Intent

`D1` keeps the successful long-budget `A0` planner path and adds a cheap
grounding reranker as an advisory signal for `fix_planning`.

The test question for this run was:

- does the reranker improve acceptance quality or stability on a real deployed
  `fix_planning` execution, without changing the planner?

## Identifiers

| Field | Value |
| --- | --- |
| incident_run_id | `a38e2cb4-1d54-4f97-b6fb-2a128b57bd32` |
| incident_id | `be24e25e-3496-4b8d-b066-52eb8d73897e` |
| task_id | `3ee028fd-9fa7-407e-9647-d9cb08a1f7f5` |
| scenario_id | `cache-stampede` |
| service_name | `ledger-api` |
| environment | `staging` |
| published_event | `payments.incident.fix-planning.escalated` |
| run_status | `escalated` |
| task_status | `escalated` |

Scenario source:

- [`underpass-payments-incident-response/docs/fixplanning-deployed-smoke-scenarios.yaml`](../../../../underpass-payments-incident-response/docs/fixplanning-deployed-smoke-scenarios.yaml)

Relevant scenario facts:

- triage hypothesis: `Hot-key expiry triggered synchronized cold fetches`
- likely component: `redis cache layer`
- rehydration evidence:
  `Cache rebuild contention inflated downstream request latency`

## Evidence Sources

This report is grounded in four sources:

1. deployed smoke output for the run above
2. `pir` pod logs from the live deployment
3. raw `llm_traces` rows stored in live `PIR` PostgreSQL
4. code inspection of the `generate -> reranker -> judge` path

Important operational note:

- `PIR` stores full LLM request and response payloads in PostgreSQL
- the `llm_traces` table records `request_body`, `response_body`,
  `response_code`, `error_text`, `incident_run_id`, `task_id`, `attempt`, and
  `operation`
- this is implemented in
  [`llm_trace_repo.go`](../../../../underpass-payments-incident-response/internal/adapters/persistence/postgres/llm_trace_repo.go)

## Timeline

Observed from the deployed smoke and pod logs:

| UTC | Event |
| --- | --- |
| `21:01:18` | `specialist execution starting` |
| `21:01:21` | runtime recommendation returned |
| `21:01:21` | kernel `GetContext` succeeded |
| `21:02:17` | internal attempt `1` rejected by judge |
| `21:03:17` | internal attempt `2` rejected by judge |
| `21:03:42` | internal attempt `3` rejected by judge |
| `21:04:22` | internal attempt `4` rejected by judge |
| `21:04:22` | executor published `payments.incident.fix-planning.escalated` |

## Execution Summary

The smoke summary for this run reported:

- `internal_attempt_count = 4`
- operations per attempt:
  - `fix-planning.generate`
  - `fix-planning.reranker`
  - `fix-planning.judge`

Per-attempt token and timing summary:

| Attempt | Duration | Generate prompt | Generate completion | Judge prompt | Judge completion | Reranker prompt |
| --- | --- | --- | --- | --- | --- | --- |
| `1` | `56.2s` | `1616` | `2349` | `1023` | `50` | `1003` |
| `2` | `60.1s` | `1834` | `2514` | `1027` | `55` | `1011` |
| `3` | `24.5s` | `1843` | `964` | `1026` | `57` | `1009` |
| `4` | `40.1s` | `1844` | `1641` | `1051` | `49` | `1059` |

Critical observation:

- all `12` LLM-side calls returned `response_code = 200`
- all planner and judge completions ended with `finish_reason = stop`
- no `repair` step was needed
- no JSON truncation happened in this run

So this failure was not caused by:

- transport
- timeout before first token
- malformed JSON
- truncation by `max_tokens`

It was a semantic rejection path.

## What The Planner Actually Proposed

Raw planner outputs from `llm_traces` show a repeated pattern.

### Attempt 1

- decision:
  `Implement staggered cache invalidation to prevent synchronized cold fetches.`
- patch:
  `Update cache invalidation logic to randomize keys or stagger expiration to avoid thundering herd.`
- rollback:
  `Revert invalidation logic change and restart service to restore default cache behavior.`

Judge result:

- `accepted = false`
- reason:
  `The rollback plan is too vague and risky as it suggests restarting the service to restore cache behavior, which may not revert the specific logic change or could cause further instability.`

### Attempt 2

- decision:
  `Implement staggered cache invalidation to prevent synchronized cold fetches.`
- patch:
  unchanged in substance
- rollback:
  `Revert the invalidation logic patch and flush Redis cache to clear stale entries without service restart.`

Judge result:

- `accepted = false`
- reason:
  `The rollback plan is flawed because flushing the Redis cache to clear stale entries contradicts the goal of resolving a cache miss surge and may exacerbate the issue by forcing immediate cold fetches again.`

### Attempt 3

- decision:
  still the same high-level plan
- patch:
  still high-level and generic
- rollback:
  `Revert the invalidation logic patch and restore previous cache invalidation behavior without flushing cache.`

Judge result:

- `accepted = false`
- reason:
  `The proposed fix is too vague and lacks specific implementation details required for an actionable patch, while the rollback plan fails to address the specific risk of cache inconsistency or data loss associated with invalidation logic changes.`

### Attempt 4

- decision:
  `Implement jittered cache invalidation with exponential backoff for cold fetches.`
- patch:
  `Update cache_invalidator.py to add 50ms random jitter to TTL expiry and implement circuit breaker for cold fetch retries.`
- rollback:
  `Revert code changes and trigger manual cache flush via admin endpoint to restore consistency before redeploying previous invalidation logic.`

Judge result:

- `accepted = false`
- reason:
  `The rollback plan is not concrete because it relies on a manual cache flush via an admin endpoint that is not confirmed to exist or be functional in the current environment.`

## What The Reranker Saw

The reranker path did run on every attempt.

The code orders documents as:

1. `finding`
2. `evidence`

That ordering comes from
[`rerankFixPlanningGrounding(...)`](../../../../underpass-payments-incident-response/internal/specialists/strategies.go).

Reranker scores:

| Attempt | finding_score | evidence_score | passes `min_score >= 0.2` |
| --- | --- | --- | --- |
| `1` | `0.999433` | `0.117347` | no |
| `2` | `0.999417` | `0.062838` | no |
| `3` | `0.999416` | `0.102088` | no |
| `4` | `0.999477` | `0.186442` | no |

Interpretation:

- the candidate plans were always strongly aligned with the triage finding
- they were never sufficiently aligned with the rehydration evidence under the
  configured threshold
- `D1` therefore surfaced a real mismatch: the planner kept talking in the
  general language of a cache stampede, but not with enough grounding in the
  specific evidence package

Important detail:

- in `D1`, the reranker signal is advisory
- it does not reject the plan by itself
- the final rejection still came from the judge

## Why This Run Failed

The failure mechanism was:

1. the deployed `pir` path executed correctly end-to-end
2. the planner produced valid JSON on every attempt
3. the reranker consistently signaled weak evidence grounding
4. the judge rejected every proposal on rollback concreteness and actionable
   specificity
5. the bounded retry loop exhausted all four internal attempts
6. the executor escalated truthfully

This is the key conclusion:

- `D1` failed because proposal quality remained insufficient
- not because the reranker or deployment path was broken

## Main Findings

### 1. The smoke was valid

This run is a valid `D1` result.

The live deployment was correctly wired:

- runtime path worked
- kernel query path worked
- reranker path worked
- no NATS or TLS issue blocked execution

### 2. `D1` did not improve acceptance for this scenario

`cache-stampede` still ended in truthful escalation.

The reranker added signal, but the overall plan remained unaccepted.

### 3. Retry feedback is reaching the model, but not changing the plan enough

`executeWithPolicy(...)` passes both:

- `PreviousRejectionReason`
- `PreviousLLMOutput`

into the next attempt.

Even so, the planner stayed in the same remediation family across attempts and
only changed rollback language superficially. The feedback loop exists, but it
did not produce a materially better plan here.

### 4. The weakest field is rollback quality

All four judge rejections mention rollback directly.

The pattern is:

- vague rollback
- rollback that worsens cache misses
- rollback that ignores consistency risk
- rollback that assumes a non-confirmed admin endpoint

So the most consistent failure surface is not `hypothesis` and not even the
top-level `decision`. It is the operational quality of `rollback_plan`, plus
patch specificity on attempt `3`.

### 5. The reranker signal is useful, but not yet decisive

`D1` exposed a meaningful asymmetry:

- `finding` score is almost perfect on every attempt
- `evidence` score stays below threshold on every attempt

That means the sidecar is already telling us something actionable:

- the planner is matching the incident label
- but not grounding deeply enough in the rehydrated evidence package

### 6. This was a truthful failure, not a regression to false success

The good news is architectural:

- no false `.completed`
- no malformed output
- no ghost success wave
- no hidden duplicate delivery issue

The system failed honestly after exhausting the bounded policy.

## Practical Conclusion

`D1` currently provides observability value more than acceptance value.

For this scenario it helped explain the failure:

- the plan aligns with the triage headline
- the plan does not align strongly enough with rehydration evidence
- the rollback plan remains too weak for acceptance

That is useful, but it is not yet enough to claim `D1` improved the baseline.

## Recommended Next Step

The next iteration should not treat this as a transport or infra bug.

The right follow-up is planner-quality work focused on the failure surface that
the run actually exposed:

1. tighten the planner contract around `rollback_plan`
2. force stronger use of the rehydration evidence in the generated patch and
   rollback
3. decide whether low reranker evidence score should remain advisory or become
   a stronger retry/judge signal

If we re-run `D1` after changes, we should compare against this exact report,
not against `A0` in the abstract.
