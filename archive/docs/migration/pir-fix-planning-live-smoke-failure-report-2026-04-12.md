# PIR Fix Planning Live Smoke Failure Report

> **Historical (superseded).** Point-in-time record; the PIR `fix_planning` program was decommissioned and the `internal/specialists/*` and `cmd/fixplanning-*-smoke` paths linked below no longer exist.

Date: `2026-04-12`

Scope: live validation of the hardened `fix_planning` path after adding:

- strict JSON parsing and schema validation
- in-attempt repair of malformed JSON
- judge validation before accepting the plan
- bounded retry feedback with prior rejection reason

This report covers the latest smoke run executed with:

```bash
go run ./cmd/fixplanning-real-smoke -service-name ledger-api -environment staging
```

Primary code under test:

- [`underpass-payments-incident-response/cmd/fixplanning-real-smoke/main.go`](../../../../underpass-payments-incident-response/cmd/fixplanning-real-smoke/main.go)
- [`underpass-payments-incident-response/internal/specialists/executor.go`](../../../../underpass-payments-incident-response/internal/specialists/executor.go)
- [`underpass-payments-incident-response/internal/specialists/strategies.go`](../../../../underpass-payments-incident-response/internal/specialists/strategies.go)
- [`underpass-payments-incident-response/internal/adapters/llm/client.go`](../../../../underpass-payments-incident-response/internal/adapters/llm/client.go)

## Identifiers

| Field | Value |
| --- | --- |
| incident_run_id | `2e41e681-db3e-412f-a232-139fa29ac6ef` |
| incident_id | `f01b006d-1af7-44f4-96fd-59e097723c31` |
| task_id | `767c8783-7679-4b36-9840-d018a5047240` |
| source_alert_id | `manual-fixplanning-smoke-1775988881263455519` |
| service_name | `ledger-api` |
| environment | `staging` |

## Evidence Sources

The findings below are grounded in four sources:

1. live smoke stdout from the run above
2. live `PIR` PostgreSQL state read back after the run
3. live kernel `GetContext` and `GetNodeDetail` reads after the run
4. code inspection of the current `PIR` executor and smoke harness

The raw structured snapshot used for this report was collected at `2026-04-12T10:47:15Z` from the live cluster and local port-forwards.

## Timeline

Observed from the smoke run:

| UTC+2 time | Event |
| --- | --- |
| `12:14:41.278` | run and task seeded |
| `12:14:41.587` | kernel seed confirmed ready for `fix_planning` |
| `12:14:41.589` | specialist execution started |
| `12:14:44.608` | runtime recommendation returned |
| `12:14:44.679` | kernel `GetContext` succeeded |
| `12:15:26.604` | first attempt rejected: `LOW_CONFIDENCE: fix_planning llm output rejected: parse llm analysis: unexpected end of JSON input` |
| `12:15:26.605` | stage failed with `TIMEOUT: task-level budget exceeded for fix_planning: context deadline exceeded` |
| `12:15:26.609` | smoke command ended with secondary `GetNodeDetail(NotFound)` on the missing decision node |

## Outcome Summary

The hardening did the right thing at the safety boundary:

- the malformed LLM output was rejected
- no `decision:{incident_id}:fix-planning` node was materialized
- no false `.completed` result was produced

The run still failed operationally:

- attempt 1 consumed almost the entire `45s` stage budget
- the second attempt had no practical time left
- the smoke harness then tried to read a decision node that was never published

## Persisted PIR State

### Incident Run

PostgreSQL state after the smoke:

| Field | Value |
| --- | --- |
| `status` | `investigating` |
| `current_stage` | `fix_planning` |
| `service_name` | `ledger-api` |
| `environment` | `staging` |
| `severity` | `SEV1` |

### Specialist Task

PostgreSQL state after the smoke:

| Field | Value |
| --- | --- |
| `stage` | `fix_planning` |
| `status` | `running` |
| `attempt` | `1` |
| `started_at` | `2026-04-12T12:14:44.610283+02:00` |
| `finished_at` | _null_ |
| `output_ref` | empty |
| `error_ref` | empty |
| `escalation_ref` | empty |

This is a real bug. The executor persists `Start()` at [`executor.go`](../../../../underpass-payments-incident-response/internal/specialists/executor.go), but it does not persist terminal task state on success, failure, or escalation. The smoke therefore leaves the DB saying `running` even after the stage has already failed.

## Full Graph Snapshot

The graph was read back with:

- `role = incident-commander`
- `role = fix-planning-agent`
- `depth = 8`
- `token_budget = 65536`
- `rehydration_mode = REHYDRATION_MODE_REASON_PRESERVING`

Both roles returned the same bundle:

| Metric | Value |
| --- | --- |
| nodes | `3` |
| relationships | `3` |
| node_details | `2` |
| rendered_hash | `render:341e38bcb146c0a8e100dedaf92136ff6825929ae74fcbd06d5a8c8076cf944c` |
| truncated | `false` |
| causal_density | `1` |
| detail_coverage | `0.6666666666666666` |

### Nodes

| Node ID | Kind | Title | Status | Key observation |
| --- | --- | --- | --- | --- |
| `incident:f01b006d-1af7-44f4-96fd-59e097723c31` | `incident` | `Ledger-Api incident from alert manual-fixplanning-smoke-1775988881263455519` | `INVESTIGATING` | root still reflects the last successful wave |
| `finding:f01b006d-1af7-44f4-96fd-59e097723c31:triage` | `finding` | `Initial hypothesis: Connection pool exhaustion from unrecycled sessions` | `HYPOTHESIZED` | triage output present and detailed |
| `evidence:f01b006d-1af7-44f4-96fd-59e097723c31:rehydration` | `evidence` | `Rehydrated evidence package` | `ASSEMBLED` | rehydration output present and detailed |

### Relationships

| Source | Type | Target | Semantic class | Notes |
| --- | --- | --- | --- | --- |
| `incident:*` | `HAS_HYPOTHESIS` | `finding:*:triage` | `causal` | sequence `1` |
| `incident:*` | `HAS_EVIDENCE` | `evidence:*:rehydration` | `causal` | sequence `2` |
| `evidence:*:rehydration` | `SUPPORTS` | `finding:*:triage` | `causal` | sequence `201` |

### Node Details

Finding detail:

```text
stage: triage
action_type: diagnose
incident_run_id: 2e41e681-db3e-412f-a232-139fa29ac6ef
incident_id: f01b006d-1af7-44f4-96fd-59e097723c31
relation_type: HAS_HYPOTHESIS
summary: Leaked or stale database connections saturated the payments-api pool
hypothesis: Connection pool exhaustion from unrecycled sessions
likely_component: postgres connection pool
```

Evidence detail:

```text
stage: rehydration
action_type: retrieve-context
incident_run_id: 2e41e681-db3e-412f-a232-139fa29ac6ef
incident_id: f01b006d-1af7-44f4-96fd-59e097723c31
relation_type: HAS_EVIDENCE
summary: Causal graph links timeout spikes to unreleased DB sessions during surge traffic
evidence: Checkout spans stay open across retry loops and starve the pool
mode: full
```

### Missing Node

The kernel still returns:

```text
Node not found: decision:f01b006d-1af7-44f4-96fd-59e097723c31:fix-planning
```

That is the correct graph outcome for this failed run. No `fix_planning` decision wave was accepted.

## What The Graph Tells Us

### 1. The hardening worked

The most important result is negative-but-correct:

- the graph stopped at the last valid state
- the kernel contains only `triage + rehydration`
- the `fix_planning` decision node is absent

This is a real improvement over the earlier behavior where fallback findings could still produce a `.completed` wave after an unusable LLM answer.

### 2. The retry policy is logically correct but operationally weak

The timeline shows a subtle failure mode:

- attempt 1 ran long enough to consume almost the whole stage budget
- attempt 1 was rejected only at the end, after the malformed JSON was detected
- attempt 2 was announced, but the context expired immediately afterward

So the code now has a bounded retry policy, but in practice the retry has no budget left when the live model is slow or malformed.

### 3. The graph and the PIR run state diverge after failure

After the failed stage:

- PostgreSQL says the run is in `current_stage = fix_planning`
- the kernel root node still says `current_stage = rehydration`

This is expected under the current architecture because the graph root advances only when a stage successfully publishes a wave. It is not wrong, but it matters for debugging: the DB is the orchestration truth; the graph is the last successful materialized truth.

### 4. Role-specific retrieval did not change the failed-state graph

`incident-commander` and `fix-planning-agent` produced the same bundle and the same rendered hash for this incident.

That means the specialized `fix-planning-agent` profile is not the limiting factor in this failure. The limiting factor is upstream LLM output reliability and stage-budget consumption, not graph retrieval quality.

### 5. The graph-shape migration is still incomplete

The live graph still contains:

- `incident --HAS_EVIDENCE--> evidence`

The migration plan expected:

- `incident --HAS_CAUSE_CONTEXT--> rehydration_evidence`

This shape gap remains visible in the live failed graph and should still be considered open.

### 6. The smoke harness surfaces a secondary error, not the primary one

`Executor.Handle` currently returns the result of `publishFailure(...)`, not the original specialist error. In the smoke harness this means:

1. the specialist logs the real cause
2. the handler returns `nil`
3. the harness continues and tries to read the decision node
4. the command exits on `GetNodeDetail(NotFound)`

So the user-facing smoke error is secondary. The real first-order failure was the LLM path exhausting the stage budget after malformed output.

## Additional Bugs Confirmed By This Test

### Bug 1: terminal task status is never persisted

Code inspection shows only a best-effort `running` update:

- [`executor.go`](../../../../underpass-payments-incident-response/internal/specialists/executor.go)

There is no matching persistence for:

- `completed`
- `failed`
- `escalated`

This is why `specialist_tasks.status` remains `running` in the database after the smoke has already failed.

### Bug 2: smoke failure reporting is misleading

The smoke currently reports:

- missing decision node

when the primary cause was:

- malformed/truncated LLM JSON followed by budget exhaustion

The harness should detect a terminal `.failed` / `.escalated` event and stop there instead of always attempting decision-node read-back.

## Conclusions

The latest live smoke should be considered a safety success and an operational failure.

Safety success:

- no false `fix_planning.completed`
- no decision node written on unusable model output
- graph remains consistent with the last accepted stage

Operational failure:

- the live model still returns malformed or slow JSON often enough to exhaust the `45s` stage budget
- the bounded retry path is not effective under the current per-stage budget shape
- the current smoke UX hides the primary error behind a secondary read-back failure

## Recommended Next Actions

1. Add per-phase time slices inside `fix_planning`:
   generation, repair, judge, publish, read-back.
2. Persist terminal task state in `specialist_tasks` for both success and failure.
3. Make the smoke harness failure-aware:
   stop on `.failed` / `.escalated` and print the published error payload.
4. Keep the current hardening:
   it is preventing false success.
5. Decide whether to:
   shorten the prompt further,
   reduce judge cost,
   or move repair/judge to a smaller faster model.
6. Close the remaining graph-shape gap:
   `HAS_EVIDENCE` vs `HAS_CAUSE_CONTEXT`.
