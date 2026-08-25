# MemoryAgentBench Benchmark Adapter

Date: 2026-05-06
Status: feasibility adapter v1 and live runner v1 available

## Positioning

MemoryAgentBench is the second external benchmark candidate for the
agentic-memory track. It evaluates memory in incremental multi-turn agent
interactions and groups tasks into four competencies:

- Accurate Retrieval (AR);
- Test-Time Learning (TTL);
- Long-Range Understanding (LRU);
- Conflict Resolution (CR).

Sources:

- Project code: `https://github.com/HUST-AI-HYZ/MemoryAgentBench`
- Dataset: `https://huggingface.co/datasets/ai-hyz/MemoryAgentBench`
- Paper: `https://arxiv.org/abs/2507.05257`

The first kernel slice targets Conflict Resolution because it stresses
temporal order, stale facts, conflict handling, and known-at-time evidence
without requiring domain-specific aggregate operators in core.

## Official Source Check

Checked on 2026-05-06:

| Source | Observed status | Impact |
| --- | --- | --- |
| GitHub repository | Public Python code for MemoryAgentBench, linked to an ICLR 2026 paper. | We can align adapter behavior with official data loading and benchmark framing. |
| README | Describes an "inject once, query multiple times" design. | KMP artifacts should ingest context once, then emit multiple asks against the same about namespace. |
| Hugging Face dataset | Publishes one default config with four splits: `Accurate_Retrieval`, `Test_Time_Learning`, `Long_Range_Understanding`, and `Conflict_Resolution`. | The adapter takes `--split` explicitly and records it as a benchmark dimension. |
| Dataset features | Rows contain `context`, `questions[]`, `answers[]`, and `metadata` including source, ids, types, and dates. | The adapter preserves question metadata in ask and expected artifacts. |
| Official code | Data utilities filter examples by `metadata.source`; some evaluation paths use GPT-4o as judge. | Source filtering is first-class. Official scoring remains separate from kernel retrieval diagnostics. |

Conclusion: MemoryAgentBench is usable now for kernel feasibility, but official
end-to-end scoring should stay clearly separated from substrate diagnostics.

## Adapter

The adapter lives in `rehydration-testkit`:

```bash
cargo run -p rehydration-testkit --bin memoryagentbench_kmp_adapter --locked -- \
  --input /path/to/memoryagentbench/Conflict_Resolution.jsonl \
  --output archive/artifacts/memoryagentbench-kmp/conflict-smoke \
  --split Conflict_Resolution \
  --source factconsolidation_mh_32k \
  --limit 10 \
  --limit-queries 10 \
  --force
```

Supported input shape:

- JSON array or JSONL;
- `context`;
- `questions[]`;
- `answers[]`;
- `metadata.source`;
- optional `metadata.qa_pair_ids[]`, `question_ids[]`, `question_types[]`, and
  `question_dates[]`;
- extra fields are preserved during parsing but not mapped into core KMP
  semantics yet.

The adapter is intentionally "inject once, query many":

```text
inject_context ingest -> all context entries
query ask             -> question 1 over current about
query ask             -> question 2 over current about
...
```

This differs from MemoryArena. MemoryArena represents staged environment
feedback after each subtask. MemoryAgentBench gives one knowledge pool and asks
multiple questions against that pool.

## Memory Mapping

For each item:

- `about`: `memoryagentbench:split:<split>:source:<source>:item:<item>`, or
  run-scoped when `--run-id` is provided;
- `benchmark_split` dimension scopes the official split and source;
- `benchmark_item` dimension scopes the dataset row;
- `memory_context` dimension scopes the injected context sequence;
- numbered fact lines such as `12. ...` become stable
  `context:fact:12` refs;
- generic context becomes ordered `context:chunk:<n>` refs;
- adjacent context entries are linked with procedural `follows` relations;
- ask events use `dimensions.scope=current_about` and do not ingest the
  question as memory.

The adapter does not infer conflict supersession. For FactConsolidation, serial
numbers are preserved as metadata and temporal coordinates, but deciding which
fact supersedes another belongs in a reader/plugin above deterministic memory.

## Generated Artifacts

| File | Purpose |
| --- | --- |
| `events.jsonl` | Ordered mixed ingest/ask stream for the live runner. |
| `ingest.jsonl` | KMP `kmp_ingest` events only. |
| `ask.jsonl` | KMP `kmp_ask` events only. |
| `expected.jsonl` | Expected answers and all refs available after context injection. |
| `replay.jsonl` | Context timeline and known-at snapshots per query. |
| `summary.json` | Aggregate counts for rows, queries, context entries, and filters. |
| `manifest.json` | Run metadata and artifact paths. |

## Runner

The live runner replays `events.jsonl` in order:

```bash
cargo run -p rehydration-testkit --bin memoryagentbench_kmp_runner --locked -- \
  --artifacts archive/artifacts/memoryagentbench-kmp/conflict-smoke \
  --output archive/artifacts/memoryagentbench-kmp/conflict-smoke-run \
  --endpoint http://rehydration-kernel.underpassai.com \
  --force
```

Generated runner artifacts:

| File | Purpose |
| --- | --- |
| `event_results.jsonl` | Per-event KMP success, elapsed time, and errors. |
| `results.jsonl` | Per-ask answer, proof, observed refs, missing refs, and known-at diagnostics. |
| `hypotheses.jsonl` | Compact answer stream for future evaluator integration. |
| `summary.json` | Aggregate event, known-at, lexical, and evidence-ref counts. |

The runner treats failed KMP events as a non-zero run. It writes diagnostic
artifacts before returning failure so ingestion, projection, retrieval, or
proof gaps remain inspectable.

## Current Scope

Implemented:

- parser for JSON array or JSONL MemoryAgentBench records;
- source filtering through `--source`;
- split/run scoped KMP refs and about namespaces;
- fact-line and generic context chunk mapping;
- inject-once/query-many artifact generation;
- replay and known-at snapshots;
- context truncation for smoke slices through `--max-context-entries`;
- query truncation for smoke slices through `--limit-queries`;
- live runner against a deployed kernel;
- per-query known-at cleanliness, missing-ref, unexpected-ref, and lexical
  answer diagnostics;
- fixture tests, adapter smoke, and live runner smoke.

Not implemented yet:

- official MemoryAgentBench scoring integration;
- GPT-4o judge integration for official long-answer tasks;
- specialized conflict reader/plugin that converts serial facts into
  supersession candidates.

## Verification

Fixture smoke:

```bash
cargo run -p rehydration-testkit --bin memoryagentbench_kmp_adapter --locked -- \
  --input crates/rehydration-testkit/tests/fixtures/memoryagentbench_minimal.jsonl \
  --output /tmp/memoryagentbench-kmp-adapter-smoke \
  --split Conflict_Resolution \
  --force
```

Expected summary for the fixture:

```text
dataset_items: 1
prepared_items: 1
questions: 2
ingest_events: 1
ask_events: 2
context_entries: 4
truncated_context_entries: 0
```

Live fixture run against `http://rehydration-kernel.underpassai.com` on
2026-05-06:

```text
total_events: 3
successful_events: 3
failed_events: 0
known_at_clean_asks: 2
lexical_answer_hits: 2
unexpected_ref_asks: 0
missing_allowed_ref_asks: 0
```

Real Conflict Resolution smoke from the official dataset on 2026-05-06:

```bash
curl -s 'https://datasets-server.huggingface.co/rows?dataset=ai-hyz%2FMemoryAgentBench&config=default&split=Conflict_Resolution&offset=0&length=8' \
  -o /tmp/memoryagentbench_conflict_rows.json
jq -c '.rows[].row' /tmp/memoryagentbench_conflict_rows.json \
  > /tmp/memoryagentbench_conflict_rows.jsonl

cargo run -p rehydration-testkit --bin memoryagentbench_kmp_adapter --locked -- \
  --input /tmp/memoryagentbench_conflict_rows.jsonl \
  --output /tmp/memoryagentbench-kmp-real-conflict-mh6k-q3-f64 \
  --split Conflict_Resolution \
  --source factconsolidation_mh_6k \
  --limit 1 \
  --limit-queries 3 \
  --max-context-entries 64 \
  --run-id memoryagentbench-real-mh6k-q3-f64-20260506 \
  --force

cargo run -p rehydration-testkit --bin memoryagentbench_kmp_runner --locked -- \
  --artifacts /tmp/memoryagentbench-kmp-real-conflict-mh6k-q3-f64 \
  --output /tmp/memoryagentbench-kmp-real-conflict-mh6k-q3-f64-run \
  --endpoint http://rehydration-kernel.underpassai.com \
  --limit-items 1 \
  --force
```

Observed summary:

```text
dataset_items: 8
prepared_items: 1
questions: 3
context_entries: 64
truncated_context_entries: 391
total_events: 4
successful_events: 4
failed_events: 0
known_at_clean_asks: 3
lexical_answer_hits: 0
unexpected_ref_asks: 0
missing_allowed_ref_asks: 0
elapsed_ms: 12610
```

Interpretation:

- KMP substrate path is stable for a real official row: ingest and asks
  complete, refs stay scoped to the current about, and no known-at leaks are
  detected.
- `kmp_ask` is not a MemoryAgentBench reader. For these questions it returns
  a generic deterministic evidence summary from the beginning of the context,
  not a question-directed answer.
- Full `factconsolidation_mh_6k` has 455 facts and 100 questions in this row.
  A 100-query live run over 256 facts was too slow for an interactive smoke
  because the runner currently emits only a final summary.
- Next benchmark work should add either a candidate reader/search layer or a
  specialized FactConsolidation plugin above KMP. It should not be implemented
  as benchmark-specific behavior in core.
