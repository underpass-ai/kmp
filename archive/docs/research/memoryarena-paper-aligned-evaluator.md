# MemoryArena Paper-Aligned Evaluator

Date: 2026-05-06
Status: local evaluator v1 implemented in `memoryarena_kmp_scorecard`, with
domain answer scoring extracted into reusable `rehydration-testkit`
`memoryarena_scorecard` primitives. Candidate for extraction to a standalone
public repository.

## Sources

- Paper: `https://arxiv.org/abs/2602.16313`
- Project: `https://memoryarena.github.io/`
- Dataset: `https://huggingface.co/datasets/ZexueHe/memoryarena`

The project site describes the dataset as multi-session agentic tasks with
`questions`, `answers`, and optional background context. The paper defines the
evaluation around Memory-Agent-Environment loops where memory is initialized
empty, updated after completed subtasks, and reused by later subtasks.

No official evaluator repository is currently published from the project site.
The evaluator below is therefore paper-aligned, not official.

## Metric Contract

The evaluator emits the paper's core evaluation shape:

- `SR`: task success rate;
- `PS`: task progress score;
- `SR@depth`: success rate at subtask depth;
- `sPS`: only for group travel when a constraint-satisfaction evaluator is
  available.

For this local v1:

- `task_success_rate` is `SR`;
- `schema_version` is currently `memoryarena-score-summary-v1`;
- `process_score` is mean per-task hard progress;
- `micro_process_score` is hard-correct subtasks divided by all subtasks;
- `sr_at_depth` is keyed by numeric subtask index;
- `soft_process_score` is explicitly a local proxy, not official `sPS`.
- `failure_classes` is a local diagnostic summary, not an official metric. It
  is included to separate kernel recall/proof failures from answer-reader or
  task-agent failures.

Task success rules:

| Domain | Local task success rule | Paper rationale |
| --- | --- | --- |
| `progressive_search` | Final subtask hard success | Paper scores success by the concluding query. |
| `formal_reasoning_math` | Final subtask hard success | Paper scores success by the major/final problem. |
| `formal_reasoning_phys` | Final subtask hard success | Paper scores success by the major/final problem. |
| `bundled_shopping` | All subtasks hard success | The final bundle must satisfy the accumulated bundle state. |
| `group_travel_planner` | All subtasks hard success | The final group plan must satisfy all accumulated traveler state. |

## Domain Scorers

`progressive_search` and formal reasoning use labelled `Exact Answer:`
extraction where present, normalized fallback matching otherwise, and explicit
alias handling such as `also written as`.

`bundled_shopping` treats `target_asin` as the hard success key. Attribute text
coverage is reported only as a diagnostic soft score.

`group_travel_planner` currently checks expected itinerary slot values against
the observed answer text. Its soft score is labelled
`travel_expected_slot_text_coverage_proxy`. This must not be reported as the
paper's official `sPS` until we have the environment tables and constraint
evaluator needed to score actual constraint satisfaction.

## Input And Output

Current local CLI:

```bash
cargo run -p rehydration-testkit --bin memoryarena_kmp_scorecard --locked -- \
  --artifacts <adapter-output-dir> \
  --run <runner-output-dir> \
  --output <score-output-dir> \
  --force
```

Inputs:

- `<adapter-output-dir>/expected.jsonl`;
- `<runner-output-dir>/results.jsonl`;
- optional `<runner-output-dir>/summary.json`.

The evaluator is fail-fast for empty filtered inputs: an empty `expected.jsonl`
or `results.jsonl` is treated as invalid input, not as a valid zero-task score.
Rows are keyed by `(task_type, task_id, subtask_index)` so one evaluator run can
combine multiple MemoryArena configs whose raw task ids overlap.

Outputs:

- `subtask_results.jsonl`;
- `task_results.jsonl`;
- `hypotheses.jsonl`;
- `score_summary.json`.

Each subtask row includes `failure_class`. `score_summary.json` aggregates the
same labels under `failure_classes`; this lets benchmark reports say whether a
failed answer came from missing/invalid evidence or from the layer that consumes
correct evidence.

## Public Repo Extraction

The public repo should not depend on Underpass deployment or kernel internals.
Recommended layout:

```text
memoryarena-evaluator/
  README.md
  LICENSE
  CITATION.cff
  crates/memoryarena-evaluator/
  fixtures/
  docs/metric-contract.md
  docs/limitations.md
```

Public input should support two layers:

- generic `gold.jsonl` + `predictions.jsonl` for any memory system;
- optional KMP adapter for Underpass artifacts.

Release criteria before publication:

- fixture coverage for all five MemoryArena configs;
- multi-config fixture coverage with overlapping task ids;
- deterministic CLI output with stable schema version;
- paper-citation and non-official disclaimer in README;
- CI running parser and scorer tests;
- separate labels for official metrics, local proxies, and kernel diagnostics.
