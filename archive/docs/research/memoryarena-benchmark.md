# MemoryArena Benchmark Adapter

Date: 2026-05-06
Status: feasibility adapter v1, stage-aware runner v1, and paper-aligned local scorecard v1 available

## Positioning

MemoryArena is the primary external benchmark candidate for the agentic-memory
track. It is a better product fit than pure chat QA because each task contains
multiple interdependent subtasks where later actions should use memory acquired
from earlier actions and feedback.

Sources:

- Project: `https://memoryarena.github.io/`
- Dataset: `https://huggingface.co/datasets/ZexueHe/memoryarena`
- Paper: `https://arxiv.org/abs/2602.16313`

LongMemEval remains the secondary conversational-memory regression. MemoryArena
is the first benchmark track intended to validate replayable, temporal,
multidimensional process memory.

## Official Source Check

Checked on 2026-05-06:

| Source | Observed status | Impact |
| --- | --- | --- |
| Project site | Describes MemoryArena as a multi-session Memory-Agent-Environment benchmark and links paper, data, and code. | Confirms this benchmark matches the kernel's agentic-memory direction. |
| Project `Code` link | Points to `https://github.com/`, not to a concrete repository. | There is no official evaluator repository available from the site in this cut. |
| Hugging Face dataset | Publishes five configs: `bundled_shopping`, `progressive_search`, `group_travel_planner`, `formal_reasoning_math`, and `formal_reasoning_phys`, each with `test` split. | The adapter should treat HF as the source of benchmark records. |
| Hugging Face files | Contains README plus JSONL files under the five config directories. | No official scoring script is published in the dataset repo. |
| Paper | Frames MemoryArena as multi-session Memory-Agent-Environment loops where agents learn from earlier feedback and reuse memory in later subtasks. | Our scorecard must remain clearly labelled as a local baseline until official code is published. |

Conclusion: official evaluator integration is currently blocked by missing
published evaluator code. The next implementable step is a paper-aligned
agentic reader/evaluator over KMP artifacts, while keeping the hook open to
swap in the official evaluator if the authors publish it.

## Adapter

The first adapter lives in `rehydration-testkit`:

```bash
cargo run -p rehydration-testkit --bin memoryarena_kmp_adapter --locked -- \
  --input /path/to/memoryarena/progressive_search/data.jsonl \
  --output archive/artifacts/memoryarena-kmp/progressive-smoke \
  --task-type progressive_search \
  --limit 10
```

Supported input shape:

- JSONL, one MemoryArena task per line;
- `id`;
- `questions[]`;
- `answers[]`;
- optional `backgrounds` as string or list;
- optional `category`;
- optional `paper_name`;
- extra fields are preserved in parsing but not mapped into core KMP semantics
  yet.

The adapter is intentionally stage-aware. It does not ingest all answers before
all asks. For each task it emits:

```text
initial ingest        -> global background, when present
pre_subtask ingest    -> current question and subtask background
ask                   -> current subtask query
post_subtask ingest   -> answer feedback after the subtask is completed
```

This keeps MemoryArena aligned with agentic memory: an answer only becomes
memory for later subtasks after the environment has produced that feedback.

Dimension declarations are append-safe:

- the initial ingest declares the task and process dimensions;
- each episode dimension is declared only once;
- later appends can use `memory.dimensions: []` through the gRPC/MCP boundary
  once prior dimensions exist;
- redeclaring an existing namespaced dimension for the same `about` is
  idempotent and does not emit a duplicate dimension node;
- sequential `follows` links use `class="procedural"`, because `temporal` is a
  traversal concept, not a valid relation semantic class.

Idempotency keys include the run scope so two benchmark runs with the same task
id do not collide when their `about` namespace changes.

## Runner

The live runner replays `events.jsonl` in order:

```bash
cargo run -p rehydration-testkit --bin memoryarena_kmp_runner --locked -- \
  --artifacts archive/artifacts/memoryarena-kmp/progressive-smoke \
  --output archive/artifacts/memoryarena-kmp/progressive-smoke-run \
  --endpoint http://rehydration-kernel.underpassai.com \
  --force
```

Generated runner artifacts:

| File | Purpose |
| --- | --- |
| `event_results.jsonl` | Per-event KMP success, elapsed time, and errors. |
| `results.jsonl` | Per-ask answer, proof, observed refs, missing refs, and known-at diagnostics. |
| `hypotheses.jsonl` | Compact answer stream for future evaluator integration. |
| `summary.json` | Aggregate event, known-at, leak, lexical, and evidence-ref counts. |

The runner treats failed KMP events as a non-zero run. It still writes the
diagnostic artifacts before returning the failure so ingestion, projection,
retrieval, or proof gaps remain inspectable.

## Scorecard

The deterministic scorecard consumes adapter and runner artifacts:

```bash
cargo run -p rehydration-testkit --bin memoryarena_kmp_scorecard --locked -- \
  --artifacts archive/artifacts/memoryarena-kmp/progressive-smoke \
  --run archive/artifacts/memoryarena-kmp/progressive-smoke-run \
  --output archive/artifacts/memoryarena-kmp/progressive-smoke-scorecard \
  --force
```

Generated scorecard artifacts:

| File | Purpose |
| --- | --- |
| `subtask_results.jsonl` | Per-subtask hard correctness, domain-specific expected-answer parsing, optional soft score, known-at diagnostics, and ref recall. |
| `task_results.jsonl` | Per-task SR decision, PS, final-subtask score, extracted answers, known-at diagnostics, and ref recall. |
| `hypotheses.jsonl` | Compact chosen answer stream for evaluator or dashboard ingestion. |
| `score_summary.json` | Aggregate SR, PS, micro-PS, SR@depth, task-type summaries, candidate hit, known-at, leak, and runner metrics. |

## Interpretation Probe

`memoryarena_kmp_interpretation_probe` tests reusable domain plugins against
real MemoryArena runner evidence. It is not an official MemoryArena score and it
does not use gold answers. The probe reads `results.jsonl`, runs reusable value
plugins over the current question plus recovered evidence refs, and writes typed
mentions and any safe derivation attempts.

```bash
cargo run -p rehydration-testkit --bin memoryarena_kmp_interpretation_probe -- \
  --run /tmp/memoryarena-realistic-short-20260506152937/combined-run \
  --output /tmp/memoryarena-realistic-short-20260506152937/combined-interpretation-probe \
  --force
```

Use this to validate plugin behavior on benchmark artifacts before wiring the
same domain plugins into readers or writer-side enrichment.

## Composed Plugin Reader

`memoryarena_kmp_plugin_reader` is the benchmark adapter for the generic kernel
reader. It consumes `ComposedEvidenceReader::kernel_default()` from
`rehydration-interpretation`, applies all registered value plugins to the same
retrieved evidence, and writes the aggregated reader output into every result
row under `plugin_reader`.

The row output includes both configured order and actual execution order:
`plugin_reader.plugin_configuration.plugin_order` records the reader
configuration, while `plugin_reader.execution_order` records what ran for that
specific ask. This matters because plugin order is an explicit reader policy,
not an implementation detail.

The adapter does not add benchmark heuristics. It only replaces `ask_answer`
when an explicit deterministic derivation step returns an answer. If a run has
no derivation steps, the scorecard should remain unchanged while the artifacts
gain typed, auditable value mentions.

```bash
cargo run -p rehydration-testkit --bin memoryarena_kmp_plugin_reader -- \
  --run /tmp/memoryarena-benchmark-20260506-plugins/combined-run \
  --output /tmp/memoryarena-benchmark-20260506-plugins/combined-plugin-reader-v3 \
  --force

cargo run -p rehydration-testkit --bin memoryarena_kmp_scorecard -- \
  --artifacts /tmp/memoryarena-benchmark-20260506-plugins/combined-artifacts \
  --run /tmp/memoryarena-benchmark-20260506-plugins/combined-plugin-reader-v3 \
  --output /tmp/memoryarena-benchmark-20260506-plugins/combined-plugin-reader-v3-scorecard \
  --force
```

Current composed-reader result on the 2x/domain combined slice
`/tmp/memoryarena-benchmark-20260506-plugins/combined-run`:

- asks: 73;
- changed answers: 0;
- value plugins: `source-code-value-v1`, `math-expression-value-v1`,
  `url-value-v1`, `money-value-v1`, `date-value-v1`;
- derivation plugins registered: `value-operation-v1`,
  `currency-derivation-v1`, `date-derivation-v1`;
- configured order is serialized in the summary and every row;
- per-row execution order currently contains the five value plugins, because
  this run has no explicit derivation steps;
- typed value mentions: 4,567;
- derivation results: 0;
- scorecard unchanged: SR 0.3000, PS 0.3622, micro-PS 0.3973.

Current probe result on the 2x/domain combined slice
`/tmp/memoryarena-benchmark-20260506-plugins/combined-run`:

- asks: 73;
- currency mentions: 35 across 20 asks;
- date mentions: 292 across 19 asks;
- math mentions: 4,226 across 25 asks;
- source-code mentions: 14 across 8 asks;
- URL mentions: 0;
- currency mentions in `formal_reasoning_math` and `formal_reasoning_phys`: 0
  after hardening against LaTeX `$...$` math delimiters;
- derivation attempts: 0, intentionally, because the probe only derives when at
  least two safe operands are available and the question asks for a supported
  operation.

## LLM Evidence Reader

`memoryarena_kmp_reader` is the first MemoryArena reader layer above the
deterministic kernel run. It consumes the current subtask question plus recovered
KMP evidence, calls an LLM, and writes a scorecard-compatible run where
`ask_answer` is replaced by the reader hypothesis.

Important boundaries:

- the prompt does not include gold answers;
- expected answers are loaded only to compute the reader summary;
- the original deterministic kernel answer is preserved as `kmp_ask_answer`;
- reader metadata is written under `memoryarena_reader`;
- this is still a reader over one recovered KMP answer, not the future agentic
  loop that can call `ask`, `near`, `trace`, `inspect`, and temporal moves
  iteratively.
- for `progressive_search`, an explicit `Exact Answer:` recovered from prior
  MemoryArena feedback is treated as a deterministic final-answer candidate
  before calling the LLM. This uses only kernel-recovered prior feedback and is
  recorded as `answer_source=progressive_exact_answer_candidate`.

Example:

```bash
cargo run -p rehydration-testkit --bin memoryarena_kmp_reader --locked -- \
  --artifacts /tmp/memoryarena-post-64cd19e-20260506/combined-artifacts \
  --run /tmp/memoryarena-post-64cd19e-20260506/combined-run \
  --output /tmp/memoryarena-post-64cd19e-20260506/combined-llm-reader \
  --endpoint "$LLM_ENDPOINT" \
  --model "$LLM_MODEL" \
  --provider openai-new \
  --force

cargo run -p rehydration-testkit --bin memoryarena_kmp_scorecard --locked -- \
  --artifacts /tmp/memoryarena-post-64cd19e-20260506/combined-artifacts \
  --run /tmp/memoryarena-post-64cd19e-20260506/combined-llm-reader \
  --output /tmp/memoryarena-post-64cd19e-20260506/combined-llm-reader-scorecard \
  --force
```

For a cheap smoke run:

```bash
cargo run -p rehydration-testkit --bin memoryarena_kmp_reader --locked -- \
  --artifacts /tmp/memoryarena-post-64cd19e-20260506/combined-artifacts \
  --run /tmp/memoryarena-post-64cd19e-20260506/combined-run \
  --output /tmp/memoryarena-post-64cd19e-20260506/progressive-llm-reader-smoke \
  --task-type progressive_search \
  --limit 2 \
  --endpoint "$LLM_ENDPOINT" \
  --model "$LLM_MODEL" \
  --provider openai-new \
  --force
```

Validated progressive reader result:

- run: `/tmp/memoryarena-post-64cd19e-20260506/progressive-llm-reader-v4`
- scorecard:

```text
tasks: 2
subtasks: 21
task_successes: 2
task_success_rate: 1.0000
passed_subtasks: 19
process_score: 0.9028
micro_process_score: 0.9048
failure_classes:
  no_prior_answer_evidence: 2
  passed: 19
known_at_clean_asks: 21
full_ref_recall_asks: 21
future_answer_leaks: 0
unexpected_ref_asks: 0
missing_allowed_ref_asks: 0
```

Reader summary:

```text
total_asks: 21
hard_successes: 19
deterministic_answers: 19
llm_answers: 2
prompt_tokens: 367
completion_tokens: 2
```

## MCP Smart Writer

`memoryarena_kmp_runner --smart-writer` is the first MemoryArena writer layer
that exercises the write protocol instead of replaying every ingest event
through low-level `kmp_ingest`.

For each staged memory entry, the runner:

- builds one `kmp_write_memory` request;
- derives candidate target refs from the adapter's deterministic relation plan;
- reads prior context before writing by calling `kmp_near` and
  `kmp_inspect` on candidate targets;
- optionally calls an LLM to choose a canonical `connect_to` relation;
- validates the LLM output before dry-run: target ref must be a candidate, the
  relation/class pair must match the canonical writer vocabulary, and `why` plus
  `evidence` must be present;
- runs `kmp_write_memory` in strict dry-run mode, then commits the same
  request with `dry_run=false`;
- verifies the written node with `kmp_inspect`;
- falls back explicitly to deterministic anemic or structural relations when no
  richer relation is justified or the proposed relation is rejected.

The writer is intentionally outside the kernel core. The kernel exposes the
write protocol, strict validation, relation quality diagnostics, and retrieval
surface. The benchmark harness acts as the LLM-powered writer client.

Pass `--log-mcp-navigation` or set `MEMORYARENA_LOG_MCP_NAVIGATION=1` to emit
JSONL diagnostics to stderr. The log stream records every writer pre-read
(`kmp_near`/`kmp_inspect`), the LLM relation-selection call, the selected
`connect_to` relation summary, strict dry-run, commit, and post-write inspect
verification. This keeps the stdout run summary parseable while making it clear
when the LLM is making a relation decision from MCP-read context.

The JSONL stream is intentionally for machines and forensic replay, not for
direct human reading. Use the digest tool for a compact operational view:

```bash
cargo run -p rehydration-testkit --bin memoryarena_kmp_log_digest --locked -- \
  --input /tmp/memoryarena-smart-writer-progressive3-20260507-003.stderr \
  --output /tmp/memoryarena-smart-writer-progressive3-20260507-003-log-digest.txt
```

The digest reports:

- relation decisions as one line per writer entry;
- `context_nodes`, the unique refs read by writer `near`/`inspect` before the
  LLM relation decision;
- `about_written_before`, the number of entries already written in the same
  MemoryArena about before that write;
- relation quality counts (`rich`, `anemic`, `structural`, `suspect`);
- commit latency and slow-commit count;
- ask navigation growth by subtask for `near`, `trace`, and `inspect`.

Large answer-feedback entries are compacted deterministically before
`kmp_write_memory`: `current.summary`, `current.evidence`, and fallback
relation evidence are bounded while preserving an `Exact Answer:` line when one
is present. This keeps writer payloads focused on auditable memory rather than
duplicating entire benchmark transcripts into every generated evidence record.

Example:

```bash
cargo run -p rehydration-testkit --bin memoryarena_kmp_runner --locked -- \
  --artifacts /tmp/memoryarena-smart-writer-llm-smoke-artifacts \
  --output /tmp/memoryarena-smart-writer-llm-smoke-run \
  --endpoint http://rehydration-kernel.underpassai.com \
  --smart-writer \
  --mcp-navigation-probe \
  --log-mcp-navigation \
  --writer-llm-endpoint https://api.openai.com/v1/chat/completions \
  --writer-llm-model gpt-4o-2024-08-06 \
  --writer-llm-provider openai \
  --writer-api-key-env LLM_API_KEY \
  --writer-max-tokens 384 \
  --writer-temperature 0 \
  --force
```

Validated public-kernel smoke on 2026-05-06:

- run id: `smart-writer-llm-20260506-005`;
- source fixture: `memoryarena_minimal.jsonl`, `progressive_search`;
- events: 7/7 successful;
- asks: 2/2 known-at-clean;
- future answer leaks: 0;
- MCP navigation calls: 2 `near`, 2 `inspect`, 2 `trace`;
- writer entry writes: 5;
- writer pre-read calls: 6;
- LLM calls: 3;
- LLM valid outputs: 3;
- deterministic fallbacks: 2 structural first-node writes;
- relation total: 5;
- rich relations: 3;
- anemic relations: 0;
- structural relations: 2;
- suspect relations: 0.

The LLM writer promoted the useful benchmark edges to causal `depends_on`
relations:

- answer 1 depends on question 1;
- question 2 depends on answer 1;
- answer 2 depends on question 2.

Validated public-kernel progressive slice on 2026-05-07:

- run id: `smart-writer-progressive3-20260507-003`;
- source fixture: 3 real `progressive_search` tasks;
- events: 81/81 successful;
- asks: 27/27 known-at-clean;
- future answer leaks: 0;
- unexpected refs: 0;
- missing allowed refs: 0;
- MCP navigation calls: 27 `near`, 27 `inspect`, 24 `trace`;
- writer entry writes: 54;
- writer LLM calls: 51;
- writer deterministic fallbacks: 3 structural first-node writes;
- relation total: 54;
- rich relations: 27;
- anemic relations: 24;
- structural relations: 3;
- suspect relations: 0;
- audit inspected 198 projected refs, found 198, missing 0, errored 0, mixed
  run ids false;
- local paper-aligned scorecard: task SR 1.0, PS 0.8796, micro-PS 0.8889,
  passed subtasks 24/27.

The digest for this run showed that relation writing itself used a bounded
context window: `context_nodes_per_relation: 0=3, 1=3, 2=3, 3=45`, with
`max_context_nodes=3`. The expensive part was not LLM context growth. It was
the synchronous write/read-after-write path as the about grew:
`max_about_written_before=23`, `slow>10000ms=30`, and `max_commit=161047ms`.
Ask navigation also showed linear path growth: `trace` observed refs increased
from 3 at subtask 2 to 23 at subtask 12, while `near` stayed capped at 4 refs
but grew from about 115 ms to about 1010 ms. This is a production exposure gap
for the write path, not a relation-prompt size problem.

P0 status after the 2026-05-07 trace and ingest scaling slices:
`kmp_trace` now exposes `page.entries` and `page.cursor`, returns
`page.next_cursor`/`page.has_more`, and calls the path query as a path proof
rather than expanding the target subtree. `kmp_write_memory` commit latency
was then fixed by making existing-ref validation use a shallow direct
memory-edge lookup instead of a full semantic traversal of the about on every
ingest.

Validated public TLS scale slices on 2026-05-07:

```text
deployed kernel: ghcr.io/underpass-ai/rehydration-kernel:dev-17fa3ad
endpoint: https://rehydration-kernel.underpassai.com
helm revision: 125
helm tests: passed
```

Real `progressive_search` smart-writer runs:

```text
1 task:
  run: /tmp/memoryarena-smart-writer-paged-smoke-20260507-171850-run
  scorecard: /tmp/memoryarena-smart-writer-paged-smoke-20260507-171850-scorecard
  events: 27/27 successful
  writes: 18
  max_commit: 979 ms
  slow_commits_gt_10s: 0
  SR: 1.0
  PS: 0.8889

3 tasks:
  run: /tmp/memoryarena-smart-writer-paged-3tasks-20260507-1728-run
  scorecard: /tmp/memoryarena-smart-writer-paged-3tasks-20260507-1728-scorecard-v2
  events: 81/81 successful
  writes: 54
  max_commit: 1313 ms
  slow_commits_gt_10s: 0
  SR: 1.0
  PS: 0.8796

10 tasks:
  run: /tmp/memoryarena-smart-writer-paged-10tasks-20260507-1738-run
  scorecard: /tmp/memoryarena-smart-writer-paged-10tasks-20260507-1738-scorecard-v2
  events: 231/231 successful
  asks: 77/77 known-at-clean
  full_ref_recall: 77/77
  future_answer_leaks: 0
  writes: 154
  max_commit: 1468 ms
  slow_commits_gt_10s: 0
  SR: 1.0
  PS: 0.8579
  micro_PS: 0.8701

25 tasks:
  run: /tmp/memoryarena-smart-writer-paged-25tasks-20260507-1825-run
  scorecard: /tmp/memoryarena-smart-writer-paged-25tasks-20260507-1825-scorecard
  events: 576/576 successful
  asks: 192/192 known-at-clean
  full_ref_recall: 192/192
  future_answer_leaks: 0
  writes: 384
  max_commit: 1648 ms
  slow_commits_gt_10s: 0
  SR: 0.96
  PS: 0.8188
  micro_PS: 0.8333

50 tasks:
  run: /tmp/memoryarena-smart-writer-paged-50tasks-20260508-0009-run
  scorecard: /tmp/memoryarena-smart-writer-paged-50tasks-20260508-0009-scorecard
  events: 1107/1107 successful
  asks: 369/369 known-at-clean
  full_ref_recall: 369/369
  future_answer_leaks: 0
  writes: 738
  max_commit: 1725 ms
  slow_commits_gt_10s: 0
  elapsed: 1987792 ms
  SR: 0.98
  PS: 0.8261
  micro_PS: 0.8374

100 tasks:
  run: /tmp/memoryarena-smart-writer-paged-100tasks-20260508-1407-run
  scorecard: /tmp/memoryarena-smart-writer-paged-100tasks-20260508-1407-scorecard
  audit: /tmp/memoryarena-smart-writer-paged-100tasks-20260508-1407-audit.json
  log_digest: /tmp/memoryarena-smart-writer-paged-100tasks-20260508-1407-log-digest.txt
  events: 2259/2259 successful
  asks: 753/753 known-at-clean
  full_ref_recall: 753/753
  future_answer_leaks: 0
  unexpected_ref_asks: 0
  missing_allowed_ref_asks: 0
  writes: 1506
  max_commit: 2028 ms
  slow_commits_gt_10s: 0
  elapsed: 4602832 ms
  task_SR: 0.97
  candidate_answer_hit_rate: 0.97
  PS: 0.8357
  micro_PS: 0.8446
```

10-task relation quality:

```text
relation_total: 154
relation_rich_count: 80
relation_anemic_count: 64
relation_structural_count: 10
relation_suspect_count: 0
relations:
  depends_on: 80
  answers: 64
  scoped_to: 10
```

25-task relation quality:

```text
relation_total: 384
relation_rich_count: 202
relation_anemic_count: 157
relation_structural_count: 25
relation_suspect_count: 0
relations:
  depends_on: 202
  answers: 156
  follows: 1
  scoped_to: 25
```

50-task relation quality:

```text
relation_total: 738
relation_rich_count: 399
relation_anemic_count: 289
relation_structural_count: 50
relation_suspect_count: 0
relations:
  depends_on: 399
  answers: 287
  follows: 2
  scoped_to: 50
```

100-task relation quality:

```text
relation_total: 1506
relation_rich_count: 910
relation_anemic_count: 496
relation_structural_count: 100
relation_suspect_count: 0
context_nodes_per_relation:
  0: 100
  1: 100
  2: 100
  3: 1206
max_context_nodes: 3
max_about_written_before: 27
relations:
  depends_on: 910
  answers: 490
  follows: 6
  scoped_to: 100
llm_calls: 1406
llm_valid_outputs: 1406
llm_invalid_outputs: 0
prompt_tokens: 1368133
completion_tokens: 221801
```

The 100-task run confirms that the paged trace plus shallow ingest validation
slice removed the previous graph traversal scaling failure: writer commits
stayed below 2.1 seconds, trace proofs returned bounded paths, and the audit
found only the expected run id across all projected refs. The remaining misses
are reader/answer-selection failures, not kernel substrate failures:

```text
task 11: expected "Psychedelics"; reader chose the wrong 2016 citation title.
task 53: expected "Insomnia"; reader chose "Batman Begins" from plausible
         Nolan candidates.
task 69: expected "March 5, 2021"; reader chose "February 26, 2021" from
         plausible release-date evidence.
```

Performance interpretation:

- the pre-fix 1-task run reached `max_commit=35331ms` after only 14 commits;
- the fixed 10-task run reached `max_commit=1468ms` across 154 commits;
- the fixed 25-task run reached `max_commit=1648ms` across 384 commits;
- the fixed 50-task run reached `max_commit=1725ms` across 738 commits;
- `context_nodes_per_relation` remained bounded at `max_context_nodes=3`;
- `trace` stayed stable, typically around 75-110 ms with two observed refs;
- `near` remained result-bounded but still grew with subtask depth, reaching
  about 1100 ms at subtask 12 and about 1400 ms at subtask 14. That is the
  remaining performance target for temporal window indexing/pagination.

The 50-task digest confirmed the same scaling shape:
`context_nodes_per_relation: 0=50, 1=50, 2=50, 3=588`, with
`max_about_written_before=27`. Ask navigation stayed bounded by result count:
`inspect` averaged about 50-60 ms across depths, and `trace` averaged about
78-91 ms from subtask 2 through subtask 14. `near` remained the only visible
depth-sensitive path, growing from about 95 ms at subtask 1 to about 1428 ms at
subtask 14, while still observing only four refs at deep subtasks.

The 25-task and 50-task runs both produced the same single final-task miss,
task id `11`. Kernel behavior was clean for that task (`known_at_clean`, full
ref recall, no future leak, no unexpected refs), but the deterministic answer
reader selected the wrong title from recovered evidence:

```text
expected: Psychedelics
selected: Psilocybin produces substantial and sustained decreases in depression
  and anxiety in patients with life-threatening cancer: A randomized
  double-blind trial
failure_class: reader_reasoning_or_extraction_gap
```

This reinforces the benchmark boundary: the kernel is surfacing the staged
memory without temporal contamination, while answer selection over multiple
plausible exact-answer strings remains a reader/agent task.

A Cohere reranker smoke test on 2026-05-08 confirmed that reranking alone is not
the right fix for this miss. The Cohere API key and `rerank-v4.0-fast` endpoint
worked, but task `11` had `candidate_answer_hit=false`: the exact answer
`Psychedelics` was not present in the current candidate set. Even in an oracle
probe that manually added `Psychedelics` beside the two observed candidates,
both `rerank-v4.0-fast` and `rerank-v4.0-pro` ranked the cancer-trial title
above the exact common-citation title, because the query contains a strong
surface cue for `cancer`. The reader follow-up is therefore not "drop in a
reranker" as a final decision maker. It is:

1. generate exact-answer candidates from evidence and prior stage feedback;
2. apply task constraints to remove incompatible candidates;
3. use a reranker only as a second-stage ordering signal over that constrained
   candidate set.

The scorecard was also corrected on 2026-05-07 to score exact answers against
the extracted `Exact Answer:` candidates before falling back to full-text
containment. This removed false negatives for one-token answers such as
`Manuelita` and equivalent labeled composite answers such as
`Poem: "Namaste" | Book: "Almost Human" by Thomas Centolella`. The scorecard is
still a local, paper-aligned evaluator, not an official MemoryArena evaluator.

Remaining follow-up: keep tightening bounded behavior on the expensive parts of
the read path. Temporal response page metadata is now part of the API/MCP
contract, but expansion budgets still need operational pressure at larger
scale:

- `kmp_near`, `goto`, `rewind`, and `forward` must make entry windows,
  relation expansion, evidence expansion, and proof expansion independently
  bounded.
- MCP tools expose the same pagination contract as the API, with explicit
  `next_cursor`/`has_more` metadata for temporal moves and trace.
- The digest should remain the operational check: after pagination, the same
  MemoryArena slice should show bounded commit latency even when
  `about_written_before` grows.
- `memoryarena_kmp_runner --endpoint https://...` currently constructs a gRPC
  client with TLS disabled; until that testkit bug is fixed, public TLS runs
  should set `REHYDRATION_KERNEL_GRPC_ENDPOINT=https://...` and omit
  `--endpoint`.

Each rich relation had inspected or temporal prior context in
`relation_quality[].prior_context_sources`, so strict dry-run accepted it as
auditable rather than speculative. Per-entry diagnostics are written to
`writer_results.jsonl`.

Validated real progressive task slice on 2026-05-06:

- artifacts: `/tmp/memoryarena-smart-writer-real-progressive-artifacts`;
- primary run: `/tmp/memoryarena-smart-writer-real-progressive-task1-run`;
- retry run for the final write:
  `/tmp/memoryarena-smart-writer-real-progressive-final-retry-portforward-run`;
- task: `progressive_search`, task id `0`, 9 subtasks;
- asks: 9/9 known-at-clean;
- full ref recall: 9/9;
- future answer leaks: 0;
- current question observed: 9/9;
- unexpected refs: 0;
- missing allowed refs: 0;
- scorecard: task success rate `1.0000`, process score `0.8889`,
  passed subtasks `8/9`.

Combined writer relation quality for the completed task:

```text
entry_writes: 18
llm_calls: 17
llm_valid_outputs: 17
llm_invalid_outputs: 0
relation_total: 18
relation_rich_count: 10
relation_anemic_count: 7
relation_structural_count: 1
relation_suspect_count: 0
relations:
  depends_on: 10
  answers: 7
  scoped_to: 1
```

Operational finding:

- the first public-ingress run completed 26/27 events and failed only the final
  answer-feedback commit with a `504 Gateway Timeout` after 60s;
- the failed node was not present after the timeout, so the write did not commit
  behind the client's back;
- the same final write succeeded over direct cluster gRPC and took 67s;
- therefore `kmp_write_memory` dry-run is a semantic/protocol check, not a
  performance or `read_after_write_ready` guarantee;
- the runtime Helm profile now sets NGINX gRPC ingress read/send timeouts to
  300s; direct LoadBalancer exposure or an async accepted-write contract remain
  valid future hardening options for very slow projection/indexing paths.

The current scorecard is `memoryarena-kmp-scorecard-paper-aligned-v1`. It is a
paper-aligned local evaluator, not the official MemoryArena evaluator. It emits
the paper's core aggregate shape:

- `SR`: task success rate;
- `PS`: mean task progress score, computed as the fraction of hard-correct
  subtasks per task and then averaged across tasks;
- `micro_process_score`: hard-correct subtasks over all subtasks;
- `sr_at_depth`: subtask-depth success rate, aligned with the paper's
  subtask-depth decay analysis;
- `soft_process_score`: currently only a local proxy for domains with a partial
  scorer.
- `failure_class`: a diagnostic label on subtask and final task rows that
  separates kernel evidence gaps from reader/agent gaps.

The evaluator keys subtasks by `(task_type, task_id, subtask_index)`, not by
`task_id` alone. This is required because MemoryArena task ids repeat across
dataset configs.

Task success follows the paper's domain distinction:

- `progressive_search`, `formal_reasoning_math`, and
  `formal_reasoning_phys`: final subtask correctness determines task success;
- `bundled_shopping` and `group_travel_planner`: all subtasks must be
  hard-correct because the final bundle or group plan must satisfy the whole
  accumulated task state.

Domain scoring:

- string answers use labelled `Exact Answer:` extraction, fallback normalized
  matching, and explicit alias handling such as `also written as`;
- shopping answers use `target_asin` as the hard success key and report
  attribute text coverage as a diagnostic soft score;
- group-travel answers use expected plan-slot text coverage as a local soft
  proxy. This is intentionally labelled as a proxy because the paper's true
  `sPS` is constraint-satisfaction based and the official environment evaluator
  has not been published.

Failure classes:

| Class | Meaning |
| --- | --- |
| `passed` | The local scorer accepted the produced answer. |
| `kernel_temporal_leak` | The answer path used future answer feedback. |
| `kernel_known_at_gap` | The answer path used refs outside the known-at set. |
| `kernel_current_question_gap` | The current question was not observed in the returned context. |
| `kernel_ref_recall_gap` | Known-at refs expected by the adapter were missing from returned evidence. |
| `no_prior_answer_evidence` | The answer was `UNKNOWN` or empty while the current answer feedback was not yet in memory. |
| `domain_agent_solution_gap` | Evidence was clean, but shopping/travel requires an agent or domain reader to choose the current product/plan. |
| `formal_reader_reasoning_gap` | Evidence was clean, but formal reasoning requires reducing evidence to the requested formula/statement. |
| `reader_reasoning_or_extraction_gap` | Evidence was clean, but the generic reader did not emit the expected answer. |

See [memoryarena-paper-aligned-evaluator.md](memoryarena-paper-aligned-evaluator.md)
for the evaluator contract and extraction plan for a standalone public repo.

## Generated Artifacts

| File | Purpose |
| --- | --- |
| `events.jsonl` | Ordered mixed ingest/ask stream for a future stage-aware runner. |
| `ingest.jsonl` | KMP `kmp_ingest` events only. |
| `ask.jsonl` | KMP `kmp_ask` events only, with required ingest count and event index. |
| `expected.jsonl` | Expected answer and known-at-time refs for each subtask. |
| `replay.jsonl` | Timeline, known-at-time snapshots, and final path refs per task. |
| `summary.json` | Aggregate counts for tasks, subtasks, events, and backgrounds. |
| `manifest.json` | Run metadata and artifact paths. |
| `writer_results.jsonl` | Runner output when `--smart-writer` is enabled: one row per `kmp_write_memory` entry, including pre-read calls, dry-run/commit/verify content, LLM metadata, and relation quality diagnostics. |

When `memoryarena_kmp_runner` is executed with `--mcp-navigation-probe`, each
`results.jsonl` ask row also includes `mcp_navigation`. This records the exact
MCP tool calls made after the deterministic ask:

- `kmp_near` around the current question ref, with `after_entries=0` so the
  probe cannot intentionally move into future feedback;
- `kmp_inspect` on the current question ref, with raw expansion disabled;
- `kmp_trace` from the current question to the latest prior known entry when
  such a target exists.

This probe is not the future LLM-driven reader/writer loop. It is a fail-fast
contract check that proves the benchmark can exercise MCP as a navigation
surface, not only as an `ingest`/`ask` transport.

With `--log-mcp-navigation`, these probe calls are also logged to stderr as
`memoryarena_mcp_navigation_probe.read.start` and
`memoryarena_mcp_navigation_probe.read.done` events with request ids, elapsed
time, and observed refs.

## Memory Mapping

For each task:

- `about`: `memoryarena:task_type:<task_type>:task:<id>`, or run-scoped when
  `--run-id` is provided;
- `benchmark_task` dimension scopes the whole task;
- `agentic_process` dimension scopes the ordered process;
- `agentic_episode` dimension scopes each subtask;
- `group_travel_planner` maps `base_person` into the initial global background
  because the paper initializes travel planning from a finalized base traveler
  state;
- background entries are available before the relevant subtask;
- question entries are written before the corresponding ask;
- answer-feedback entries are written after the corresponding ask;
- answer-feedback entries relate to their question with `rel="answers"`;
- later questions relate to the previous answer-feedback with `rel="follows"`.

The mapping is benchmark-neutral at the core level. It does not encode
MemoryArena scoring rules into KMP.

## Current Scope

Implemented:

- parser for JSON array or JSONL MemoryArena records;
- generic shape validation;
- KMP staged artifact generation;
- replay artifact generation;
- run-id isolation;
- live stage-aware runner against a deployed kernel;
- optional MCP navigation probe over `kmp_near`, `kmp_inspect`, and
  `kmp_trace`;
- optional MCP smart writer harness over `kmp_write_memory`, with
  read-before-write, LLM relation selection, strict dry-run, commit, verify, and
  relation quality aggregation;
- known-at correctness and future-answer leak diagnostics;
- paper-aligned local scorecard over runner artifacts;
- multi-config-safe runner and scorecard keys;
- LLM evidence reader over recovered KMP memory, producing scorecard-compatible
  run artifacts;
- incremental ingest through gRPC/MCP with empty dimension declarations after
  dimensions already exist;
- idempotent redeclaration of existing namespaced dimensions during incremental
  ingest;
- fixture tests and adapter smoke.

Not implemented yet:

- MemoryArena official evaluator integration, blocked until official code is
  published or discoverable;
- agentic retrieval loop that lets an LLM choose multiple KMP moves before
  answering;
- production-grade agentic writer policy loop beyond the current benchmark
  harness, including broader graph search and multi-candidate relation planning;
- write-path operational contract for slow commits: dry-run currently validates
  semantics, but does not guarantee commit latency or public-ingress
  `read_after_write_ready` completion;
- benchmark dashboard or graph visualization.

## Verification

Fixture smoke:

```bash
cargo run -p rehydration-testkit --bin memoryarena_kmp_adapter --locked -- \
  --input crates/rehydration-testkit/tests/fixtures/memoryarena_minimal.jsonl \
  --output /tmp/memoryarena-kmp-adapter-smoke \
  --task-type progressive_search \
  --force
```

Expected summary for the fixture:

```text
dataset_items: 1
prepared_tasks: 1
subtasks: 2
ingest_events: 5
ask_events: 2
replay_events: 7
background_entries: 1
```

The second subtask's known-at-time snapshot includes the global background, the
first question, the first answer feedback, and the second question. It does not
include the second answer before the second ask.

Live runner smoke against the public kernel endpoint:

```bash
cargo run -p rehydration-testkit --bin memoryarena_kmp_adapter --locked -- \
  --input crates/rehydration-testkit/tests/fixtures/memoryarena_minimal.jsonl \
  --output /tmp/memoryarena-mcp-nav-smoke-artifacts \
  --task-type progressive_search \
  --run-id mcp-nav-20260506-001 \
  --force

cargo run -p rehydration-testkit --bin memoryarena_kmp_runner --locked -- \
  --artifacts /tmp/memoryarena-mcp-nav-smoke-artifacts \
  --output /tmp/memoryarena-mcp-nav-smoke-run \
  --endpoint http://rehydration-kernel.underpassai.com \
  --mcp-navigation-probe \
  --force
```

Observed summary:

```text
total_events: 7
successful_events: 7
failed_events: 0
known_at_clean_asks: 2
future_answer_leaks: 0
current_question_observed: 2
unexpected_ref_asks: 0
missing_allowed_ref_asks: 0
mcp_navigation_probe: true
mcp_navigation_asks: 2
mcp_navigation_near_calls: 2
mcp_navigation_inspect_calls: 2
mcp_navigation_trace_calls: 2
mcp_navigation_known_at_clean_asks: 2
mcp_navigation_future_answer_leaks: 0
mcp_navigation_current_question_observed: 2
mcp_navigation_unexpected_ref_asks: 0
```

The first ask correctly returns `UNKNOWN` because the answer feedback is not yet
known. The second ask retrieves the first answer feedback and proves it through
the staged memory path without leaking the second answer. With the navigation
probe enabled, the runner also records a temporal neighborhood around each
question, node details for each current question, and a trace to the latest
prior known entry. In the second subtask, `kmp_trace` returns the procedural
`follows` relation from the current question to the previous answer feedback and
the evidential `answers` relation from that answer feedback to the previous
question.

## Real Progressive Search Slice

Run date: 2026-05-06

Dataset source:

- `ZexueHe/memoryarena`
- config: `progressive_search`
- split: `test`
- Dataset Viewer total rows: 221
- slice: `offset=0`, `length=3`

Dataset export:

```bash
curl -s -o /tmp/memoryarena-progressive-rows-3.json \
  'https://datasets-server.huggingface.co/rows?dataset=ZexueHe/memoryarena&config=progressive_search&split=test&offset=0&length=3'

jq -c '.rows[].row' /tmp/memoryarena-progressive-rows-3.json \
  > /tmp/memoryarena-progressive-real-3.jsonl
```

Slice shape:

```text
task 0: 9 questions / 9 answers
task 1: 12 questions / 12 answers
task 2: 6 questions / 6 answers
```

Adapter command:

```bash
cargo run -p rehydration-testkit --bin memoryarena_kmp_adapter --locked -- \
  --input /tmp/memoryarena-progressive-real-3.jsonl \
  --output /tmp/memoryarena-progressive-real-3-artifacts \
  --task-type progressive_search \
  --run-id memoryarena-real-20260506-s3 \
  --force
```

Adapter summary:

```text
dataset_items: 3
prepared_tasks: 3
subtasks: 27
ingest_events: 54
ask_events: 27
replay_events: 81
background_entries: 0
```

Runner command:

```bash
cargo run -p rehydration-testkit --bin memoryarena_kmp_runner --locked -- \
  --artifacts /tmp/memoryarena-progressive-real-3-artifacts \
  --output /tmp/memoryarena-progressive-real-3-run \
  --endpoint http://rehydration-kernel.underpassai.com \
  --force
```

Runner summary:

```text
total_events: 81
successful_events: 81
failed_events: 0
known_at_clean_asks: 27
future_answer_leaks: 0
current_question_observed: 27
unexpected_ref_asks: 0
missing_allowed_ref_asks: 0
elapsed_ms: 106581
```

Interpretation:

- The deployed kernel accepted a real staged MemoryArena slice end to end.
- Every ask observed the current question ref.
- Every ask respected known-at-time boundaries.
- No ask leaked its current answer feedback before that feedback was ingested.
- Every ask returned all refs that were expected to be available at that point
  in the staged process.
- `lexical_answer_hits` is not meaningful for this staged run because
  `expected.answer` represents environment feedback that intentionally becomes
  memory only after the ask. Task-success scoring belongs in the next evaluator
  layer.

Scorecard command:

```bash
cargo run -p rehydration-testkit --bin memoryarena_kmp_scorecard --locked -- \
  --artifacts /tmp/memoryarena-progressive-real-3-artifacts \
  --run /tmp/memoryarena-progressive-real-3-run \
  --output /tmp/memoryarena-progressive-real-3-scorecard \
  --force
```

Scorecard summary:

```text
scorecard: memoryarena-kmp-scorecard-paper-aligned-v1
schema_version: memoryarena-score-summary-v1
tasks: 3
subtasks: 27
ask_count: 27
task_successes: 3
task_success_rate: 1.0
passed_subtasks: 24
process_score: 0.8796296296296297
micro_process_score: 0.8888888888888888
candidate_answer_hits: 3
candidate_answer_hit_rate: 1.0
known_at_clean_asks: 27
full_ref_recall_asks: 27
current_question_observed_asks: 27
future_answer_leaks: 0
unexpected_ref_asks: 0
missing_allowed_ref_asks: 0
runner_total_events: 81
runner_successful_events: 81
runner_failed_events: 0
runner_elapsed_ms: 106581
```

Interpretation:

- The paper-aligned local scorecard recovered the final exact answer for all
  three tasks in this first `progressive_search` slice, so paper-style `SR`
  is `1.0` for this slice.
- `PS` is lower than `SR` because early progressive subtasks can be correct
  process memory without the current answer text being emitted by
  `kmp_ask.answer`. This is useful signal for separating memory substrate
  behavior from reader/agent answer formatting.
- One task required alias-aware matching: the expected answer used `Daniel Delos
  Santos`, while the retrieved candidate used `John Daniel delos Santos` with an
  explicit alias.
- This is a local reader baseline over kernel evidence, not an official
  MemoryArena score.

Observed consumer gap:

- The deterministic `answer` field can grow as accumulated prior feedback grows.
  This is acceptable for proof/evidence validation, but not sufficient as the
  final agentic benchmark answer. The next layer should score task success from
  an agent/reader that consumes the kernel refs and can choose traversal moves
  before producing the final answer.

## Realistic Two-Per-Domain Slice

Run date: 2026-05-06

Dataset source:

- `ZexueHe/memoryarena`
- configs: all five MemoryArena configs
- split: `test`
- slice: first two valid tasks per config

Slice shape:

| Domain | Tasks | Subtasks |
| --- | ---: | ---: |
| `bundled_shopping` | 2 | 12 |
| `progressive_search` | 2 | 21 |
| `group_travel_planner` | 2 | 15 |
| `formal_reasoning_math` | 2 | 10 |
| `formal_reasoning_phys` | 2 | 15 |
| Total | 10 | 73 |

Live runner result against `http://rehydration-kernel.underpassai.com`:

```text
events: 221/221 successful
asks: 73/73 known-at-clean
full_ref_recall_asks: 73/73
future_answer_leaks: 0
unexpected_ref_asks: 0
missing_allowed_ref_asks: 0
```

Combined scorecard result:

```text
tasks: 10
subtasks: 73
task_successes: 3
task_success_rate: 0.3000
passed_subtasks: 29
process_score: 0.3622
micro_process_score: 0.3973
known_at_clean_asks: 73
full_ref_recall_asks: 73
current_question_observed_asks: 73
future_answer_leaks: 0
unexpected_ref_asks: 0
missing_allowed_ref_asks: 0
failure_classes:
  passed: 29
  no_prior_answer_evidence: 6
  domain_agent_solution_gap: 23
  formal_reader_reasoning_gap: 15
```

By domain:

| Domain | Task SR | Passed subtasks | PS | Micro PS | Soft PS |
| --- | ---: | ---: | ---: | ---: | ---: |
| `bundled_shopping` | 0.0000 | 0/12 | 0.0000 | 0.0000 | 0.0167 |
| `progressive_search` | 1.0000 | 19/21 | 0.9028 | 0.9048 | n/a |
| `group_travel_planner` | 0.0000 | 0/15 | 0.0000 | 0.0000 | 0.5837 |
| `formal_reasoning_math` | 0.0000 | 2/10 | 0.2000 | 0.2000 | n/a |
| `formal_reasoning_phys` | 0.5000 | 8/15 | 0.7083 | 0.5333 | n/a |

Interpretation:

- The kernel substrate passed the memory part of the slice: no temporal leaks,
  no missing allowed refs, no unexpected refs, and every ask observed the
  current question.
- `progressive_search` is the cleanest current fit because later answers can
  often be recovered directly from prior feedback.
- `bundled_shopping` and `group_travel_planner` need an agent/environment or
  domain reader. The kernel retrieves the staged evidence, but `kmp_ask`
  currently replays prior feedback instead of choosing the current product or
  composing the current itinerary.
- The formal domains need a specialized exact-answer reader over recovered
  evidence; otherwise long mathematical/physics context is surfaced correctly
  but not reduced to the requested formula or statement.
- No failure in this slice is classified as a kernel evidence gap.
- This result should not be reported as an official MemoryArena score. It is a
  kernel-backed local scorecard that separates memory recall from task
  reasoning and answer construction.

Post-P0 validation:

- Re-run after `64cd19e` (`Fix idempotent memory dimension appends`) against the
  redeployed public kernel kept the same scorecard values and the same clean
  evidence guarantees.
- Artifacts: `/tmp/memoryarena-post-64cd19e-20260506`
- Runner aggregate:

```text
events: 221/221 successful
asks: 73/73 known-at-clean
full_ref_recall_asks: 73/73
future_answer_leaks: 0
unexpected_ref_asks: 0
missing_allowed_ref_asks: 0
```

- Scorecard stayed unchanged: `task_success_rate=0.3000`,
  `process_score=0.3622`, `micro_process_score=0.3973`, `passed_subtasks=29/73`.
- Interpretation: the P0 idempotent-dimension fix stabilized repeated
  namespaced dimension writes without changing reader capability. Remaining
  MemoryArena failures are still reader/agent solution gaps, not kernel evidence
  gaps.

## Next Cut

1. Add benchmark reader/plugin layers for shopping, travel, and formal exact
   answer extraction, outside the kernel core.
2. Add an agentic retrieval loop that can choose `ask`, `near`, `trace`,
   `inspect`, and temporal moves before answering.
3. Persist projection/traversal/proof metrics needed to classify benchmark
   failures without reading raw logs.
4. Fix production write exposure for slow `kmp_write_memory` commits:
   configure ingress timeout, expose direct gRPC, or introduce async accepted
   writes with explicit completion polling.
5. Extract the paper-aligned evaluator into a standalone public repository once
   the metric contract and fixture suite are stable.
