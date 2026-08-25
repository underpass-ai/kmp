# PIR Fix Planning A0 Repeatability Report 2026-04-12

> **Historical (superseded).** Point-in-time record; the PIR `fix_planning` program was decommissioned and the PIR code paths/tooling cited below (e.g. `cmd/fixplanning-*-smoke`, `internal/specialists/*`, `fixplanning-deployed-smoke-scenarios.yaml`) were restructured after this was written.

Status: completed
Date: 2026-04-12
Scope: first repeatability pass for deployed `A0` after adding first-class LLM
observability metrics

## Deployment Under Test

- deployment: `pir`
- namespace: `underpass-runtime`
- image:
  `ghcr.io/underpass-ai/payments-incident-response:v0.4.14-llm-observability-20260412T182127Z`
- policy:
  - `fix_planning stageBudget = 20m`
  - `maxIterations = 4`
  - `AckWait = 25m`
  - `planner generate max_tokens = 8192`
  - `planner thinking = enabled`
  - `repair thinking = disabled`
  - `judge thinking = disabled`

## What Changed In This Iteration

This iteration implemented `A0.1`:

- first-class LLM metrics exposed on `/metrics`
- matching JSON snapshot exposed on `/api/v1/metrics`
- labels available at minimum for:
  - `provider`
  - `model`
  - `stage`
  - `operation`
- raw `llm_traces` retained for forensic request and response inspection

Metric families added:

- `pir_llm_calls_total`
- `pir_llm_prompt_tokens_total`
- `pir_llm_completion_tokens_total`
- `pir_llm_finish_reason_total`
- `pir_llm_latency_seconds`

## Repeatability Runs

Five deployed smokes were executed against the live `pir` consumer using
`go run ./cmd/fixplanning-deployed-smoke -service-name ledger-api -environment staging -timeout 25m`.

### Run Fichas

| Run | incident_run_id | task_id | published_event | run_status | task_status | task_attempt | read-back |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `024c82e8-c6f4-41fa-94bd-1796f57428ab` | `2337088f-99c6-49f5-9f24-950c1ad6b80e` | `payments.incident.fix-planning.completed` | `mitigating` | `completed` | `1` | valid |
| 2 | `8f0a370d-43c6-4cb0-a61e-e33819c10b97` | `19d238eb-2d1c-4c10-aa6e-bd98ef695553` | `payments.incident.fix-planning.completed` | `mitigating` | `completed` | `1` | valid |
| 3 | `aec6dbd7-ed0f-4281-b073-1cf5dc03400d` | `a428cd00-ceb5-4379-95a2-c730aa7a5eac` | `payments.incident.fix-planning.completed` | `mitigating` | `completed` | `1` | valid |
| 4 | `8b2a008b-d828-49ef-bb9e-7be0a1a57682` | `96b24dd4-978a-438c-b387-7a743592fbb2` | `payments.incident.fix-planning.completed` | `mitigating` | `completed` | `1` | valid |
| 5 | `2ed76842-39f5-40c0-b270-65aea8316b7c` | `067697e8-c651-4cfc-acd6-c870391baf01` | `payments.incident.fix-planning.completed` | `mitigating` | `completed` | `1` | valid |

Result:

- `5/5` truthful live completions
- `0/5` false `.completed`
- `0/5` read-back failures
- `0/5` escalations in this sample

## LLM Observability Result

The new metrics were visible live on the deployed service.

Relevant series after the five-run sample:

- `pir_llm_calls_total{operation="fix-planning.generate", ...} = 7`
- `pir_llm_calls_total{operation="fix-planning.judge", ...} = 6`
- `pir_llm_completion_tokens_total{operation="fix-planning.generate", ...} = 13119`
- `pir_llm_completion_tokens_total{operation="fix-planning.judge", ...} = 272`
- `pir_llm_finish_reason_total{operation="fix-planning.generate", finish_reason="stop", ...} = 6`
- `pir_llm_finish_reason_total{operation="fix-planning.generate", finish_reason="error", ...} = 1`
- `pir_llm_finish_reason_total{operation="fix-planning.judge", finish_reason="stop", ...} = 6`

This confirms that token usage and finish reasons are now observable without
reading raw `llm_traces`.

## Per-Run LLM Trace Findings

Using `llm_traces` grouped by `incident_run_id`:

### Run 1: `024c82e8-c6f4-41fa-94bd-1796f57428ab`

- `fix-planning.generate`
  - calls: `2`
  - prompt tokens: `1670`
  - completion tokens: `2118`
  - finish reasons: `error`, `stop`
- `fix-planning.judge`
  - calls: `1`
  - prompt tokens: `842`
  - completion tokens: `40`
  - finish reason: `stop`

### Run 2: `8f0a370d-43c6-4cb0-a61e-e33819c10b97`

- `fix-planning.generate`
  - calls: `1`
  - prompt tokens: `1613`
  - completion tokens: `2026`
  - finish reason: `stop`
- `fix-planning.judge`
  - calls: `1`
  - prompt tokens: `890`
  - completion tokens: `45`
  - finish reason: `stop`

### Run 3: `aec6dbd7-ed0f-4281-b073-1cf5dc03400d`

- `fix-planning.generate`
  - calls: `1`
  - prompt tokens: `1556`
  - completion tokens: `2624`
  - finish reason: `stop`
- `fix-planning.judge`
  - calls: `1`
  - prompt tokens: `863`
  - completion tokens: `49`
  - finish reason: `stop`

### Run 4: `8b2a008b-d828-49ef-bb9e-7be0a1a57682`

- `fix-planning.generate`
  - calls: `2`
  - prompt tokens: `3379`
  - completion tokens: `4462`
  - finish reason: `stop`
- `fix-planning.judge`
  - calls: `2`
  - prompt tokens: `1742`
  - completion tokens: `92`
  - finish reason: `stop`

### Run 5: `2ed76842-39f5-40c0-b270-65aea8316b7c`

- `fix-planning.generate`
  - calls: `1`
  - prompt tokens: `1610`
  - completion tokens: `1889`
  - finish reason: `stop`
- `fix-planning.judge`
  - calls: `1`
  - prompt tokens: `883`
  - completion tokens: `46`
  - finish reason: `stop`

## Main Findings

1. `A0` now satisfies Gate 2 on this sample.

The current deployed `Qwen3.5-9B` configuration produced `5/5` successful live
`fix_planning` runs with truthful kernel read-back.

2. The new observability layer is useful, not cosmetic.

`prompt_tokens`, `completion_tokens`, `finish_reason`, call counts, and latency
are now visible directly from the live service metrics.

3. Successful planner completions are not tiny.

Single successful `generate` calls landed roughly in the `1889-2624`
completion-token range for these runs. That is materially above the old `1024`
ceiling and supports the earlier decision to raise planner output headroom.

4. `repair` was not part of the happy path in this sample.

No `fix-planning.repair` calls appeared in the live metrics for these five
runs. The successful path was planner output plus judge acceptance.

5. The extra `generate` / `judge` calls are explained.

Two of the five runs showed more than one `generate` and/or `judge` call even
though the exposed `specialist_tasks.attempt` remained `1`.

This is not, by itself, evidence of duplicate NATS delivery.

The code path in `PIR` already performs bounded internal planning retries inside
`executeWithPolicy(...)`. Those sub-attempts:

- increment `ExecutionInput.Attempt`
- annotate `llm_traces` with that internal attempt value
- remain inside the same specialist task attempt from the workflow point of
  view

So the observed `2x generate` or `2x judge` runs are consistent with internal
planner retries, not necessarily with a duplicated stage task.

What still remains open is not "why were there two planner calls?" but:

- whether those internal sub-attempts should be surfaced more explicitly in
  reporting
- and whether terminal result redelivery is fully idempotent on failure paths

6. The smoke remains focused on `fix_planning`, but the live system continues
downstream.

`llm_traces` for the same incident runs also captured reranker activity in later
stages such as `patch_application`, `branch_push`, `merge`, `deploy`,
`verification`, and `recovery_confirmation`. That is expected once the live
workflow advances beyond `fix_planning`.

## A0.2 Delivery Idempotency Validation

`A0.2` was implemented and then verified with dedicated deployed smokes.

### Escalated Path

- smoke command:
  `go run ./cmd/fixplanning-duplicate-stage-result-smoke`
- live run:
  - `incident_run_id = bf81e5e5-adf7-4af5-b786-140c971dc6aa`
  - `incident_id = 8b9fa6cb-5cfe-496b-960c-666f55126e22`
  - `task_id = f1260cc3-ea87-45d2-b4e2-2382615c5570`
- duplicated external publishes:
  - `3d39a4a3-5df1-4ebe-b070-ac8ccccb59d3`
  - `3c99359f-15ed-44c2-857b-bd235cfb9516`

Observed result:

- `run_status = escalated`
- `task_status = escalated`
- `task_count = 1`
- `workflow_event_count = 3`
- event counts:
  - `payments.incident.fix-planning.escalated = 1`
  - `payments.incident.status.escalated = 1`
  - `payments.incident.escalated.to-human = 1`
- `dlq_message_count = 0`

Interpretation:

- terminal duplicate delivery is now idempotent on the deployed `pir`
- the historical `.escalated -> escalated` loop is no longer reproducible with
  the dedicated smoke

### Failed Path

- smoke command:
  `go run ./cmd/fixplanning-duplicate-failed-stage-result-smoke`
- live run:
  - `incident_run_id = 3bb9fe00-599a-4382-a2d3-74746a903d4f`
  - `incident_id = 3e01b85f-136e-42b2-aa7c-17eb796b4147`
  - `task_id = 6a5d2149-0a75-41b1-bf51-1c991685d10b`
- duplicated external publishes:
  - `cd0fc742-29d5-41a3-9f0e-44ee8f90c835`
  - `adc45f56-f863-47bc-8dd0-a18055855618`

Observed result:

- `run_status = investigating`
- `run_stage = fix_planning`
- original task:
  - `status = failed`
  - `attempt = 1`
- retry task:
  - `status = requested`
  - `attempt = 2`
- `task_count = 2`
- event counts:
  - `payments.incident.fix-planning.failed = 1`
  - `payments.incident.fix-planning.requested = 1`
- `dlq_message_count = 0`

Interpretation:

- duplicate delivery on the retryable `.failed` path is also idempotent on the
  deployed `pir`
- duplicate external `.failed` publishes do not create duplicate retry tasks
- `A0.2` is now closed for both paths that were operationally relevant:
  duplicate `.escalated` and duplicate retryable `.failed`

## A0.3 Execution Evidence And YAML Scenarios

`A0.3` was implemented after `A0.2` to expose internal planner sub-attempts as
first-class execution evidence and to move deployed smokes onto a named YAML
scenario catalog.

### What Changed

- `fixplanning-deployed-smoke` can now load named scenarios from
  `fixplanning-deployed-smoke-scenarios.yaml`
- the smoke summary now includes an `execution_evidence` block derived from
  `llm_traces` for the exact `task_id` under test
- that block exposes:
  - `internal_attempt_count`
  - `operation_totals`
  - per-sub-attempt duration
  - per-sub-attempt operation list
  - `prompt_tokens`, `completion_tokens`, and `finish_reasons`
  - failure reasons when a sub-attempt fails

This closes one of the main interpretability gaps left open by `A0.1`:
successful runs can now be explained without querying raw traces manually.

### Named Scenario Validation

The YAML catalog was validated locally with:

- `go run ./cmd/fixplanning-deployed-smoke -scenario-file ./docs/fixplanning-deployed-smoke-scenarios.yaml -list-scenarios`

Observed scenario ids:

- `cache-stampede`
- `connection-pool-exhaustion`
- `queue-backlog`

### Live Scenario Run

One deployed live smoke was then executed against the valid `pir` deployment:

- scenario id: `cache-stampede`
- incident_run_id: `3b0ec071-53f4-4cf7-9267-e0142ff34c18`
- incident_id: `4b2aeafd-153d-4d01-9606-78635977fabd`
- task_id: `352fd2c0-09e4-4b73-9161-477327f4c998`
- published_event: `payments.incident.fix-planning.completed`
- run_status: `mitigating`
- run_stage: `patch_application`
- relationships_ok: `true`

Kernel read-back remained valid:

- `decision:4b2aeafd-153d-4d01-9606-78635977fabd:fix-planning`
- `ADDRESSES` present
- `BASED_ON` present

### Execution Evidence Observed

The smoke output for that run now exposed:

- `internal_attempt_count = 4`
- `task_attempt = 1`
- `latest_stage_task_attempt = 1`

Interpretation:

- the workflow-visible specialist task still completed in its first workflow
  attempt
- inside that task, the planner used bounded internal sub-attempts exactly as
  designed by `executeWithPolicy(...)`
- those internal sub-attempts are now visible directly in the smoke output

Observed attempt pattern:

- attempt 1:
  - `generate` + `judge`
  - semantic classifier activity during publish
  - long duration (`331623ms`)
- attempt 2:
  - `generate`
  - failed with `context deadline exceeded`
- attempt 3:
  - `generate` + `judge`
  - successful continuation
- attempt 4:
  - `generate` + `judge`
  - final accepted output

Important consequence:

- the earlier ambiguity around "extra generate/judge calls" is no longer a
  forensic-only question
- the current deployed smoke can now distinguish:
  - workflow-level retries
  - planner sub-attempts inside one workflow task

### Resulting A0 Status

After `A0.1`, `A0.2`, and now `A0.3`, the current baseline is stronger than the
original experiment intent:

- deployed `A0` can complete truthfully
- deployed `A0` has passed an initial `5/5` repeatability sample
- duplicate `.escalated` and `.failed` delivery is validated as idempotent
- internal planner sub-attempts are now visible as first-class run evidence
- named YAML scenarios now exist for broader repeatability coverage
