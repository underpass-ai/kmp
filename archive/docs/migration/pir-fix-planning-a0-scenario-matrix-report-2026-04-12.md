# PIR Fix Planning A0 Scenario Matrix Report 2026-04-12

> **Historical (superseded).** Point-in-time record; the PIR `fix_planning` specialist program was later restructured, so code paths and tooling referenced below (`cmd/fixplanning-deployed-smoke`, `fixplanning-deployed-smoke-scenarios.yaml`) no longer exist as cited.

Status: completed
Date: 2026-04-12
Scope: first multi-scenario deployed `A0` batch after `A0.3` execution
evidence and YAML scenario support

Companion report:

- [`pir-fix-planning-a0-repeatability-report-2026-04-12.md`](pir-fix-planning-a0-repeatability-report-2026-04-12.md)

## Intent

`A0.4` exists to answer a different question from the earlier five-run sample.

The repeatability report already showed that one deployed configuration could
complete `fix_planning` repeatedly on the original shape.

The next question was:

- does the same deployed `A0` baseline stay truthful and successful across
  multiple named incident families, not just one repeated fixture?

## Command Under Test

Batch command:

```bash
go run ./cmd/fixplanning-deployed-smoke \
  -scenario-file ./docs/fixplanning-deployed-smoke-scenarios.yaml \
  -all-scenarios \
  -repeats 1 \
  -timeout 90m
```

Scenario catalog:

- `cache-stampede`
- `connection-pool-exhaustion`
- `queue-backlog`

## Aggregate Result

Observed matrix summary:

- `scenario_count = 3`
- `repeats = 1`
- `total_runs = 3`
- `completed = 3`
- `truthful_failures = 0`
- `infrastructure_errors = 0`

This means the first widened `A0` batch achieved:

- `3/3` truthful completions
- `0/3` false `.completed`
- `0/3` kernel read-back regressions
- `0/3` transport or deployment failures

## Per-Scenario Fichas

### 1. `cache-stampede`

- `incident_run_id = 75dd71d8-62b9-48b3-a3ee-abc8d1b3c670`
- `incident_id = f2992854-e627-4931-89e3-b11467983c71`
- `task_id = b91130f4-c7c3-4907-a2a9-afae6b1757fa`
- `published_event = payments.incident.fix-planning.completed`
- `run_status = mitigating`
- `run_stage = patch_application`
- `relationships_ok = true`
- `internal_attempt_count = 1`
- `attempt_1_duration_ms = 48317`
- `generate_completion_tokens = 2028`
- `judge_completion_tokens = 48`

Decision title:

- `Implement staggered cache invalidation to prevent thundering herd effect.`

### 2. `connection-pool-exhaustion`

- `incident_run_id = f0fab6aa-b95a-44c8-9449-b62ed8690775`
- `incident_id = f06e8c2c-a906-49b8-a319-ecbe7d84f2da`
- `task_id = e8004172-211e-4eda-b66e-3009d56ec198`
- `published_event = payments.incident.fix-planning.completed`
- `run_status = mitigating`
- `run_stage = patch_application`
- `relationships_ok = true`
- `internal_attempt_count = 1`
- `attempt_1_duration_ms = 42195`
- `generate_completion_tokens = 1747`
- `judge_completion_tokens = 38`

Decision title:

- `Deploy fix to close checkout spans and recycle stale DB sessions immediately.`

### 3. `queue-backlog`

- `incident_run_id = d57d526d-8a08-494b-b85a-f3909bcbeb11`
- `incident_id = 4b1926f3-ab07-4c68-8821-6d5bc02013c1`
- `task_id = b03b160a-3e44-4d25-b7f3-92253415014c`
- `published_event = payments.incident.fix-planning.completed`
- `run_status = mitigating`
- `run_stage = patch_application`
- `relationships_ok = true`
- `internal_attempt_count = 1`
- `attempt_1_duration_ms = 33621`
- `generate_completion_tokens = 1326`
- `judge_completion_tokens = 51`

Decision title:

- `Deploy hotfix to reduce retry amplification timeout thresholds immediately.`

## Main Findings

1. `A0` still holds outside the original repeated fixture.

The current deployed `Qwen3.5-9B` baseline completed truthfully on three
different incident shapes without needing larger planners or support-model
routing.

2. The widened coverage did not require workflow retries.

All three runs completed with:

- `task_attempt = 1`
- `latest_stage_task_attempt = 1`
- `internal_attempt_count = 1`

So this batch did not need either workflow-level retry or internal planning
sub-attempt loops.

3. The completion-token envelope is now much clearer.

Across the three scenarios:

- planner `generate` completion tokens landed in the `1326-2028` range
- judge completion tokens landed in the `38-51` range
- all successful planner calls finished with `finish_reason = stop`

That is consistent with the earlier conclusion that the old `1024` ceiling was
artificially low for successful planner output.

4. Kernel read-back semantics remain stable across scenario shapes.

All three runs preserved:

- `decision` node materialization
- `ADDRESSES`
- `BASED_ON`
- downstream advancement to `patch_application`

5. `A0.4` changes the promotion discussion.

The next decision is no longer driven by a lack of successful breadth evidence.

The current question becomes:

- whether to keep widening `A0` with more scenarios and repeats
- or whether the marginal return is now higher in `D1` or `A1`

## Updated Interpretation

After `A0.1`, `A0.2`, `A0.3`, and now `A0.4`, the live baseline is materially
stronger than the original intent of the experiment:

- `A0` completes truthfully
- `A0` is observable
- `A0` is delivery-safe on validated duplicate paths
- `A0` exposes internal planner execution evidence
- `A0` has now passed a first widened multi-scenario live batch

That does not prove universal robustness, but it does prove that `A0` is now a
serious baseline, not just a one-off happy-path demo.
