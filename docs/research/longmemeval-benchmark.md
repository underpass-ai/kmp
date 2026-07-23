# LongMemEval Benchmark Adapter

Status: P0 adapter, LLM evidence builder, embedding candidate-recall harness,
live KMP runner, external reader, and official-evaluator path available.

Positioning: LongMemEval remains a secondary conversational-memory regression
for retrieval, temporal recall, multi-session recall, knowledge updates, and
abstention. It should not define the primary Underpass Kernel product claim.
The primary benchmark track is agentic memory: replayable, inspectable
multi-session processes where memory and action are coupled.

Current active mapping is intentionally binary at the evidential edge level:
LongMemEval answer evidence is represented as `supports_answer`. A typed
relation taxonomy was investigated on May 5, 2026 and rolled back because it
did not improve the benchmark enough to justify changing the API contract. The
details and artifacts are documented in
[Relation Semantics Investigation](#relation-semantics-investigation).

Important boundary: relations are not a substitute for operation semantics.
They are useful for causality, provenance, temporal navigation, and "why" paths,
but they cannot decide by themselves whether a value inside a turn is countable
or summable. A money mention can be a paid amount, budget, refund, cancelled
quote, comparison, plan, or context-only detail. Aggregate correctness needs a
typed derivation contract, not just a richer relation label.

LongMemEval is an external benchmark. The kernel must not encode
LongMemEval-specific business logic. This adapter maps LongMemEval records into
Kernel Memory Protocol artifacts so that the same kernel API can be measured
against single-session, multi-session, knowledge-update, temporal-reasoning,
and abstention questions.

## Source

Official sources:

- Paper: `https://arxiv.org/abs/2410.10813`
- Repository: `https://github.com/xiaowu0162/LongMemEval`
- Cleaned dataset: `https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned`

The cleaned dataset files expected by the adapter are:

- `longmemeval_s_cleaned.json`
- `longmemeval_m_cleaned.json`
- `longmemeval_oracle.json`

Do not commit those dataset files to this repository.

## Mapping

For each LongMemEval item:

- `IngestRequest.about` is `longmemeval:item:<question_id>`.
- `benchmark_record` dimension scopes the whole item.
- `conversation` dimensions scope each haystack session.
- `question` dimension scopes the benchmark question.
- `evidence_set` dimension records the expected evidence group.
- each chat turn becomes a memory entry with both benchmark-record and
  conversation coordinates.
- evidence turns produce `supports_answer` evidential relations and explicit
  evidence records.
- `kernel_ask` requests use `dimensions.scope=current_about` and
  `dimensions.mode=all`.

This preserves the kernel rule that dimensions are namespaced by `about` while
still making session and evidence provenance inspectable.

There are two evidence-construction modes:

- Oracle baseline: when no generated evidence file is provided, LongMemEval
  `has_answer: true` turns are mapped to `supports_answer`. This measures the
  kernel retrieval path with perfect evidence edges.
- LLM-built evidence selection: when `--evidence-labels` is provided, only the
  generated labels decide which turns become kernel evidence. The generated
  labels currently contain `turn_ref`, `reason`, and `confidence`; the adapter
  still emits `supports_answer` relations. The original LongMemEval
  `has_answer` refs remain in `expected.jsonl` for coverage measurement, but
  they are not used to create kernel relations.

`idempotency_key` includes a stable fingerprint of the full LongMemEval record.
This avoids collisions when a fixture, oracle file, or cleaned split reuses the
same `question_id` with different content.

## Build Evidence With An LLM

The evidence builder is outside kernel core. It receives raw LongMemEval turns
and asks an OpenAI-compatible or Anthropic model to select the turn refs needed
to answer each question. The prompt does not include the gold answer or
`has_answer` labels.

```bash
cargo run -p rehydration-testkit --bin longmemeval_kmp_evidence_builder -- \
  --input /path/to/longmemeval_oracle.json \
  --output artifacts/longmemeval-kmp/evidence-labels \
  --endpoint https://api.openai.com/v1/chat/completions \
  --model gpt-4o-2024-08-06 \
  --provider openai \
  --api-key-env OPENAI_API_KEY \
  --max-tokens 512 \
  --temperature 0 \
  --limit 40
```

Generated builder files:

- `evidence_labels.jsonl`: validated labels consumed by the adapter.
- `builder_results.jsonl`: per-item selected refs, token usage, and latency.
- `summary.json`: aggregate selected-turn and token counts.

The builder fails fast on malformed JSON, duplicate refs, unknown refs,
unsupported confidence values, missing endpoint/model, or a non-empty output
directory unless `--force` is provided.

## Measure Embedding Candidate Recall

The embedding sidecar is outside kernel core. Its first measurable contract is
candidate generation: given the benchmark question and all scoped conversation
turns, rank turns through an OpenAI-compatible `/v1/embeddings` endpoint and
measure whether the LongMemEval gold evidence refs appear in top-K.

This is not answer scoring. It tells us whether a local embedding model can
reduce the candidate set presented to the evidence builder or reader without
dropping required evidence.

```bash
cargo run -p rehydration-testkit --bin longmemeval_kmp_embedding_candidates -- \
  --input /path/to/longmemeval_oracle.json \
  --output artifacts/longmemeval-kmp/embedding-candidates \
  --endpoint http://localhost:8000/v1/embeddings \
  --model Qwen/Qwen3-Embedding-0.6B \
  --top-k 30 \
  --batch-size 128 \
  --limit 40
```

Generated embedding files:

- `candidate_results.jsonl`: per-item top-K ranked refs, scores, gold refs,
  hit refs, missing refs, and latency.
- `summary.json`: aggregate recall@K by question type, token usage when
  reported by the provider, and request counts.

The harness fails fast on malformed embedding responses, duplicate or missing
embedding indexes, dimension mismatches, zero-norm vectors, missing
endpoint/model, `--top-k 0`, `--batch-size 0`, or a non-empty output directory
unless `--force` is provided.

### Paid Embedding Probe, May 6, 2026

We first exercised the embedding contract with a paid OpenAI-compatible
provider to avoid deploying local sidecar infrastructure before knowing whether
candidate ranking is useful.

Run:

```bash
cargo run -p rehydration-testkit --bin longmemeval_kmp_embedding_candidates -- \
  --input /tmp/longmemeval-official-eval/longmemeval_oracle.json \
  --output /tmp/longmemeval-kmp-embedding-openai-balanced40-top30 \
  --endpoint https://api.openai.com/v1/embeddings \
  --model text-embedding-3-small \
  --top-k 30 \
  --batch-size 128 \
  --per-question-type-limit 8 \
  --exclude-abstention \
  --force
```

Summary:

- items: 48 balanced items, 8 per question type;
- candidate turns: 975;
- gold evidence turns: 89;
- embedding requests: 48;
- provider-reported prompt tokens: 296,664;
- elapsed wall time: 23.8s.

Recall from the same paid ranking, recomputed offline without additional API
calls:

| K | Selected turns | Hit turns | Evidence recall | Full-hit items |
|---|---:|---:|---:|---:|
| 5 | 234 | 70 / 89 | 78.7% | 34 / 48 |
| 10 | 456 | 84 / 89 | 94.4% | 44 / 48 |
| 15 | 634 | 86 / 89 | 96.6% | 46 / 48 |
| 20 | 763 | 88 / 89 | 98.9% | 47 / 48 |
| 25 | 861 | 89 / 89 | 100.0% | 48 / 48 |
| 30 | 901 | 89 / 89 | 100.0% | 48 / 48 |

The remaining top-20 miss was `gpt4_d84a3211`, a `multi-session` bike-expense
sum question. One required evidence turn ranked 21st, so top-25 recovered all
gold evidence in this slice. This supports using embeddings as a broad
candidate generator, not as the final aggregate reasoner.

## Derivation Reader For Aggregates

The derivation reader is a benchmark-side reader, not kernel core. It consumes
embedding-ranked candidates, asks an LLM to extract typed operands with refs,
then applies the aggregate operation deterministically in code. The LLM does
not receive the gold answer and does not produce the final arithmetic result.

These operations are testkit prototypes for future derivation plugins. They
must not move into the kernel core or the public KMP/gRPC surface as built-in
business operators. Kernel core owns memory ingestion, traversal, dimensions,
temporal cursors, evidence paths, and inspection. Operator logic owns domain
semantics such as "this amount is paid and included", "this value is only a
quote", "this date is the active one", or "this entity is a duplicate".

Supported first-slice operations:

- `sum`;
- `count`;
- `average`;
- `difference`;
- `max_by`;
- `list`;
- `unknown`.

The extraction contract requires every operand to carry:

- source `ref`;
- `label`: `include`, `exclude`, or `context`;
- `role`: for example `addend`, `counted_item`, `minuend`, `subtrahend`;
- optional `entity`;
- optional numeric `value`;
- reason.

For first-person personal-history totals, the reader filters prompt candidates
to user-authored turns when available. This prevents assistant suggestions,
hypothetical route proposals, and recommendations from becoming aggregate
operands.

```bash
cargo run -p rehydration-testkit --bin longmemeval_kmp_derivation_reader -- \
  --input /tmp/longmemeval-official-eval/longmemeval_oracle.json \
  --candidates /tmp/longmemeval-kmp-embedding-openai-balanced40-top30/candidate_results.jsonl \
  --output /tmp/longmemeval-kmp-derivation-ms8-gpt4o \
  --endpoint https://api.openai.com/v1/chat/completions \
  --model gpt-4o-2024-08-06 \
  --provider openai \
  --candidate-top-k 25 \
  --max-tokens 1200 \
  --temperature 0 \
  --force
```

First multi-session derivation probe, May 6, 2026:

- input: the 8 `multi-session` items from the paid embedding balanced slice;
- candidate source: paid embedding ranking, top-25;
- extractor: `gpt-4o-2024-08-06`;
- operations selected: 4 `count`, 4 `sum`;
- lexical answer hits: 8 / 8;
- prompt tokens: 43,196;
- completion tokens: 2,116;
- elapsed wall time: 18.0s.

Observed failure modes before the final prompt/filtering changes:

- the extractor counted assistant route proposals as actual trips;
- it merged pickup and return obligations for an exchange;
- it omitted a safety accessory purchase from an expense total;
- lexical matching missed number-word equivalence such as `5` vs `five`.

The final probe is not yet a benchmark claim. It shows that the architecture is
viable: embeddings recover a broad candidate set, the LLM extracts typed
operands with refs, and deterministic code performs the aggregate derivation.

### 100-Item Paid Embedding And Derivation Probe, May 6, 2026

The next scale step used the first 100 non-abstention oracle items with the paid
embedding candidate generator:

```bash
cargo run -p rehydration-testkit --bin longmemeval_kmp_embedding_candidates -- \
  --input /tmp/longmemeval-official-eval/longmemeval_oracle.json \
  --output /tmp/longmemeval-kmp-embedding-openai-100-top30 \
  --endpoint https://api.openai.com/v1/embeddings \
  --model text-embedding-3-small \
  --top-k 30 \
  --batch-size 128 \
  --limit 100 \
  --exclude-abstention \
  --force
```

Embedding recall summary:

- items: 100;
- question mix: 46 `multi-session`, 54 `temporal-reasoning`;
- candidate turns: 3,216;
- gold evidence turns: 277;
- selected top-K turns: 2,649;
- hit turns: 275 / 277;
- full-hit items: 98 / 100;
- partial-hit items: 2 / 100;
- recall: 99.28%;
- provider-reported prompt tokens: 1,013,686;
- embedding requests: 100;
- elapsed wall time: 58.2s.

Per type:

| Type | Items | Hit turns | Evidence recall | Full-hit items |
|---|---:|---:|---:|---:|
| `multi-session` | 46 | 160 / 161 | 99.38% | 45 / 46 |
| `temporal-reasoning` | 54 | 115 / 116 | 99.14% | 53 / 54 |

For `multi-session`, offline top-K analysis from the same paid ranking showed:

| K | Hit turns | Evidence recall | Full-hit items |
|---|---:|---:|---:|
| 20 | 154 / 161 | 95.65% | 40 / 46 |
| 25 | 158 / 161 | 98.14% | 43 / 46 |
| 30 | 160 / 161 | 99.38% | 45 / 46 |

The derivation reader was then run on the 46 `multi-session` items with top-30
candidates:

```bash
cargo run -p rehydration-testkit --bin longmemeval_kmp_derivation_reader -- \
  --input /tmp/longmemeval-official-eval/longmemeval_oracle.json \
  --candidates /tmp/longmemeval-kmp-embedding-openai-100-top30/candidate_results.jsonl \
  --output /tmp/longmemeval-kmp-derivation-100-ms-gpt4o \
  --endpoint https://api.openai.com/v1/chat/completions \
  --model gpt-4o-2024-08-06 \
  --provider openai \
  --candidate-top-k 30 \
  --max-tokens 1200 \
  --temperature 0 \
  --force
```

Derivation summary:

- extractor: `gpt-4o-2024-08-06`;
- items: 46 `multi-session`;
- prompt tokens: 335,344;
- completion tokens: 13,377;
- elapsed wall time: 115.7s;
- stored lexical hits before numeric comma normalization: 31 / 46;
- adjusted lexical hits with the fixed matcher: 34 / 46;
- official LongMemEval `gpt-4o` autoeval: 29 / 46, `0.6304`.

Operation breakdown:

| Operation | Items | Raw lexical hits | Adjusted lexical hits | Official hits |
|---|---:|---:|---:|---:|
| `average` | 1 | 1 | 1 | 1 |
| `count` | 26 | 16 | 16 | 12 |
| `difference` | 3 | 3 | 3 | 2 |
| `max_by` | 2 | 1 | 1 | 1 |
| `sum` | 13 | 10 | 13 | 13 |
| `unknown` | 1 | 0 | 0 | 0 |

The three raw `sum` misses were scoring false negatives caused by thousands
separators (`$2500` vs `$2,500`, `$5850` vs `$5,850`, `$3750` vs `$3,750`). The
reader matcher now normalizes comma-grouped numbers before comparing.

Remaining misses after adjusted scoring:

- `count` questions with non-trivial predicates, exclusions, or deduplication;
- one `max_by` store-spend question where the wrong merchant won;
- one temporal lookup question routed to `unknown`, which is outside the
  aggregate derivation reader's first-slice responsibility.

One `multi-session` item had only partial top-30 evidence recall
(`gpt4_31ff4165`), but the derivation still answered it correctly. The current
limiting factor for this slice is therefore not broad recall. It is typed
operand quality for predicate-heavy counting and comparison.

The official evaluator is stricter than the lexical matcher on compact numeric
answers. For example, some responses containing only the correct count were
marked wrong when the reference answer also contained entity details. The
official score remains the benchmark-facing number, while the adjusted lexical
score is used only for local failure analysis.

### P0 Derivation Reader Hardening, May 6, 2026

The first 100-item derivation probe showed that evidence recall was not the
limiting factor for the `multi-session` aggregate slice. The next P0 therefore
kept the same paid embedding candidates and hardened the derivation reader
contract.

Changes:

- count answers now include the counted entity names when available;
- distinct/type/kind counts deduplicate canonical entity names deterministically;
- duration differences use absolute differences for duration-style questions;
- ISO date strings accidentally returned in `value` are parsed into numeric day
  ordinals instead of failing the whole run;
- the prompt now makes OR-action predicates, pickup/return obligations,
  ownership semantics, relative month windows, `max_by` spend candidates, and
  unsupported direct lookups explicit.

Run:

```bash
cargo run -p rehydration-testkit --bin longmemeval_kmp_derivation_reader -- \
  --input /tmp/longmemeval-official-eval/longmemeval_oracle.json \
  --candidates /tmp/longmemeval-kmp-embedding-openai-100-top30/candidate_results.jsonl \
  --output /tmp/longmemeval-kmp-derivation-100-ms-gpt4o-p0-count-maxby-v2 \
  --endpoint https://api.openai.com/v1/chat/completions \
  --model gpt-4o-2024-08-06 \
  --provider openai \
  --candidate-top-k 30 \
  --max-tokens 1400 \
  --temperature 0 \
  --force
```

P0 v2 result:

- items: 46 `multi-session`;
- lexical hits: 35 / 46;
- official LongMemEval `gpt-4o` autoeval: 33 / 46, `0.7174`;
- prompt tokens: 352,088;
- completion tokens: 13,811;
- elapsed wall time: 150.7s.

Operation breakdown:

| Operation | Items | Lexical hits | Official hits |
|---|---:|---:|---:|
| `average` | 1 | 1 | 1 |
| `count` | 25 | 17 | 15 |
| `difference` | 3 | 3 | 3 |
| `max_by` | 2 | 1 | 1 |
| `sum` | 14 | 13 | 13 |
| `unknown` | 1 | 0 | 0 |

Compared with the baseline derivation run, official hits improved from 29 to
33. Newly corrected items included distinct doctor/citrus/cuisine counts,
duration date subtraction, simultaneous-project exclusion, and current musical
instrument ownership.

Remaining misses are dominated by omitted qualifying evidence rather than
arithmetic:

- predicate-heavy counts where one qualifying item/event is missed;
- one `max_by` grocery-store comparison that still omits the Thrive Market
  online order;
- one workshop spend total that still excludes a paid November workshop inside
  the four-month window;
- official-judge false negatives where the count is correct but the response is
  too compact or lacks the reference's explanatory details.

Final slice conclusion:

> Underpass Kernel separates memory retrieval from reasoning. In our first
> LongMemEval multi-session slice, retrieval reached ~99% evidence recall, while
> end-to-end QA reached 71.7%, showing that the kernel reliably surfaces the
> right memory but the current generic reader still misses structured operands.
> This makes the next step clear: a specialized graph/operand extractor on top
> of a deterministic, inspectable memory substrate.

### Verifier Probe Decision, May 6, 2026

We tested a second LLM verifier/repair pass over extracted operands. It is not
part of this benchmark cut.

Observed result:

- full verifier official autoeval: 33 / 46, `0.7174`;
- selective `max_by` verifier official autoeval: 33 / 46, `0.7174`;
- verifier fixed some individual omissions, including the Thrive Market
  `max_by` case;
- verifier also introduced count regressions through over-inclusion.

Decision:

- keep embedding/reranking as the retrieval-side improvement;
- do not add verifier/repair to the main derivation path;
- treat money sums, date/duration parsing, `max_by`, and count semantics as
  explicit derivation plugins instead of free-form verifier behavior;
- train or replace the generic extractor against those typed plugin contracts.

The plugin boundary should keep deterministic computation outside the LLM. The
LLM should select refs and typed operands; plugins should own normalization,
date math, currency handling, entity canonicalization, and operation-specific
validation.

The reusable boundary is split between `rehydration-plugin-api` for
request/result traits, `rehydration_domain::plugins` as the kernel-domain
re-export, and `rehydration_interpretation` for money/date extractors plus
deterministic value-operation plugins. The existing LongMemEval derivation
reader still performs its own prompt-time operand extraction; the next cleanup
is to emit the shared `DerivationRequest` contract and let the reusable plugin
compute the final result.

## Generate Artifacts

```bash
cargo run -p rehydration-testkit --bin longmemeval_kmp_adapter -- \
  --input /path/to/longmemeval_s_cleaned.json \
  --output artifacts/longmemeval-kmp/smoke \
  --limit 40
```

The output directory must be empty unless `--force` is provided.
Use `--per-question-type-limit N` to build a deterministic balanced subset by
question type from the dataset order.

To use LLM-built relations instead of the oracle baseline, pass the generated
labels:

```bash
cargo run -p rehydration-testkit --bin longmemeval_kmp_adapter -- \
  --input /path/to/longmemeval_oracle.json \
  --output artifacts/longmemeval-kmp/smoke \
  --evidence-labels artifacts/longmemeval-kmp/evidence-labels/evidence_labels.jsonl \
  --limit 40
```

Generated files:

- `ingest.jsonl`: MCP-style `kernel_ingest` tool calls.
- `ask.jsonl`: MCP-style `kernel_ask` tool calls.
- `expected.jsonl`: deterministic expected answer/evidence metadata.
- `summary.json`: aggregate item/session/turn counts, gold expected evidence
  refs, and relation evidence refs actually ingested.
- `manifest.json`: run manifest with methodology pointer and artifact paths.

## Run Against A Live Kernel

The runner replays generated artifacts through the public KMP tool surface backed
by `KernelMemoryService` gRPC. It fails fast on tool errors, artifact mismatches,
or non-empty output directories unless `--force` is provided.

```bash
cargo run -p rehydration-testkit --bin longmemeval_kmp_runner -- \
  --artifacts artifacts/longmemeval-kmp/smoke \
  --output artifacts/longmemeval-kmp/run-smoke \
  --endpoint http://rehydration-kernel.underpassai.com
```

Without `--endpoint`, configure the MCP gRPC backend through:

```bash
export REHYDRATION_KERNEL_GRPC_ENDPOINT=http://rehydration-kernel.underpassai.com
cargo run -p rehydration-testkit --bin longmemeval_kmp_runner -- \
  --artifacts artifacts/longmemeval-kmp/smoke \
  --output artifacts/longmemeval-kmp/run-smoke
```

Generated run files:

- `results.jsonl`: one row per item with expected refs, observed refs, missing
  refs, evidence-hit classification, expected answer, deterministic ask answer,
  lexical answer-hit heuristic, and latencies.
- `hypotheses.jsonl`: official-evaluator-friendly hypotheses with
  `question_id` and `hypothesis`.
- `summary.json`: aggregate counts for ingested items, asked items, abstentions,
  evidence items, full hits, partial hits, missing hits, lexical answer hits,
  and elapsed time.

## Run The External Reader

The reader is intentionally outside the kernel. It consumes recovered KMP
evidence and asks an OpenAI-compatible or Anthropic model to produce the compact
LongMemEval hypothesis. It does not put LongMemEval scoring logic in kernel
core, and it does not include the gold answer in the prompt.

The reader fails fast when `--endpoint`/`LLM_ENDPOINT` or `--model`/`LLM_MODEL`
is missing. Public OpenAI-compatible endpoints protected by mTLS can be used with
`--tls-cert-path`/`LLM_TLS_CERT_PATH` and `--tls-key-path`/`LLM_TLS_KEY_PATH`;
providing only one side of the client identity is an error.

```bash
cargo run -p rehydration-testkit --bin longmemeval_kmp_reader -- \
  --artifacts /tmp/longmemeval-kmp-balanced60-artifacts \
  --run /tmp/longmemeval-kmp-balanced60-run \
  --output /tmp/longmemeval-kmp-balanced60-reader \
  --endpoint http://localhost:8000/v1/chat/completions \
  --model Qwen/Qwen3-8B \
  --provider openai \
  --max-tokens 256 \
  --temperature 0 \
  --force
```

Gemma 4 31B on the Underpass mTLS ingress:

```bash
LLM_ENABLE_THINKING=false \
cargo run -p rehydration-testkit --bin longmemeval_kmp_reader -- \
  --artifacts /tmp/lme-ms30-smart-writer-artifacts-20260509 \
  --run /tmp/lme-ms30-smart-writer-run-20260509 \
  --output /tmp/lme-ms30-smart-writer-reader-gemma4 \
  --endpoint https://llm.underpassai.com/v1/chat/completions \
  --model google/gemma-4-31B-it \
  --provider openai \
  --tls-cert-path /tmp/underpass-demo-client.crt \
  --tls-key-path /tmp/underpass-demo-client.key \
  --max-tokens 256 \
  --temperature 0 \
  --force
```

Generated reader files:

- `reader_results.jsonl`: one row per item with question, evidence-hit class,
  hypothesis, lexical answer-hit heuristic, token counts, and reader latency.
- `hypotheses.jsonl`: official-evaluator-friendly hypotheses with
  `question_id` and `hypothesis`.
- `summary.json`: aggregate reader counts and token usage by question type.

## First Metrics

P0 measures adapter correctness and deterministic retrieval coverage:

- dataset shape validity;
- temporal coordinate parseability;
- per-item evidence-turn coverage;
- per-item evidence-session coverage;
- `kernel_ask` proof hit rate against `answer_turn_refs`;
- live ingest and ask latency per item.
- optional LLM evidence-builder token usage and selected-turn coverage.
- optional embedding candidate recall@K before evidence construction.

Evidence hit classification:

- `full`: every expected `answer_turn_ref` appears in the structured ask result.
- `partial`: at least one expected `answer_turn_ref` appears.
- `missing`: expected evidence refs exist but none appear.
- `not_applicable`: abstention item or item without expected answer refs.

`lexical_answer_hit` is a deterministic string containment heuristic only. It is
useful for triage, but it is not the official LongMemEval LLM-judge accuracy.
The official evaluator can consume `hypotheses.jsonl` plus the original
LongMemEval reference file.

P1 adds answer quality:

- raw full-history baseline;
- oracle-history baseline;
- kernel evidence answer;
- kernel evidence plus LLM reader;
- official LongMemEval QA evaluator where applicable.

Do not claim LongMemEval success from adapter generation alone. A public claim
requires a completed run with persisted artifacts and exact commit SHA.

The current branch supports two different claims:

- Oracle evidence score: the kernel can retrieve all gold evidence when perfect
  `supports_answer` relations are provided.
- LLM-built evidence score: a separate model first selects evidence turns from
  raw conversations, then the adapter emits `supports_answer` evidence
  relations, then the kernel retrieves those relations, then the reader produces
  the LongMemEval hypothesis.

## Smoke Result

On May 5, 2026, the minimal repository fixture was replayed against
`http://rehydration-kernel.underpassai.com`:

- total items: 1
- ingested items: 1
- asked items: 1
- evidence items: 1
- full evidence hits: 1
- partial evidence hits: 0
- missing evidence hits: 0

This smoke only proves the harness and live KMP path. It is not a LongMemEval
score.

## Balanced60 Oracle Evidence Baseline

On May 5, 2026, a deterministic balanced subset was generated from
`longmemeval_oracle.json` with 10 items per `question_type`:

```bash
cargo run -p rehydration-testkit --bin longmemeval_kmp_adapter -- \
  --input /tmp/longmemeval-official-eval/longmemeval_oracle.json \
  --output /tmp/longmemeval-kmp-balanced60-artifacts \
  --per-question-type-limit 10 \
  --force

cargo run -p rehydration-testkit --bin longmemeval_kmp_runner -- \
  --artifacts /tmp/longmemeval-kmp-balanced60-artifacts \
  --output /tmp/longmemeval-kmp-balanced60-run \
  --endpoint http://rehydration-kernel.underpassai.com \
  --force
```

Adapter summary:

- total source items: 500
- prepared items: 60
- sessions: 102
- turns: 1209
- expected evidence turns: 111
- relation evidence turns: 111
- question types: 10 each for `knowledge-update`, `multi-session`,
  `single-session-assistant`, `single-session-preference`,
  `single-session-user`, and `temporal-reasoning`

Runner summary:

- ingested items: 60/60
- asked items: 60/60
- full evidence hits: 60/60
- partial evidence hits: 0
- missing evidence hits: 0
- lexical answer hits: 23/60
- elapsed time: 46.456 seconds

Lexical answer hits by type:

- `knowledge-update`: 7/10
- `multi-session`: 1/10
- `single-session-assistant`: 5/10
- `single-session-preference`: 0/10
- `single-session-user`: 7/10
- `temporal-reasoning`: 3/10

Interpretation: KMP ingest and deterministic evidence retrieval work across all
question types in this balanced slice. The low lexical answer-hit rate is
expected because `kernel_ask.answer` currently returns evidence/context, not a
LongMemEval-normalized QA answer. Official accuracy requires running the
LongMemEval evaluator over `hypotheses.jsonl`.

Reader plus official evaluator on the same Balanced60 run, using
`gpt-4o-2024-08-06` as external reader and `gpt-4o` as official metric model:

- reader lexical answer hits: 34/60
- reader prompt tokens: 24,161
- reader completion tokens: 246
- official accuracy: 40/60, `0.6667`

Official accuracy by type:

- `knowledge-update`: 5/10
- `multi-session`: 8/10
- `single-session-assistant`: 9/10
- `single-session-preference`: 0/10
- `single-session-user`: 9/10
- `temporal-reasoning`: 9/10

This Balanced60 result is still the oracle-evidence baseline because the
relations came from LongMemEval `has_answer` labels.

## LLM-Built Relation Smoke

On May 5, 2026, a 3-item temporal-reasoning smoke was built from
`longmemeval_oracle.json` without using gold answers for relation construction:

```bash
cargo run -p rehydration-testkit --bin longmemeval_kmp_evidence_builder -- \
  --input /tmp/longmemeval-official-eval/longmemeval_oracle.json \
  --output /tmp/longmemeval-kmp-llm3-labels \
  --endpoint https://api.openai.com/v1/chat/completions \
  --model gpt-4o-2024-08-06 \
  --provider openai \
  --api-key-env OPENAI_API_KEY \
  --max-tokens 512 \
  --temperature 0 \
  --limit 3 \
  --force

cargo run -p rehydration-testkit --bin longmemeval_kmp_adapter -- \
  --input /tmp/longmemeval-official-eval/longmemeval_oracle.json \
  --output /tmp/longmemeval-kmp-llm3-artifacts \
  --limit 3 \
  --evidence-labels /tmp/longmemeval-kmp-llm3-labels/evidence_labels.jsonl \
  --force

cargo run -p rehydration-testkit --bin longmemeval_kmp_runner -- \
  --artifacts /tmp/longmemeval-kmp-llm3-artifacts \
  --output /tmp/longmemeval-kmp-llm3-run \
  --endpoint http://rehydration-kernel.underpassai.com \
  --force
```

Builder summary:

- total items: 3
- selected turns: 5
- prompt tokens: 26,968
- completion tokens: 376
- elapsed time: 7.917 seconds

Runner summary:

- ingested items: 3/3
- asked items: 3/3
- full evidence hits: 3/3
- partial evidence hits: 0
- missing evidence hits: 0
- lexical answer hits: 1/3
- elapsed time: 2.787 seconds

Reader plus official evaluator:

- reader lexical answer hits: 2/3
- reader prompt tokens: 809
- reader completion tokens: 11
- official accuracy: 3/3, `1.0`
- question type: `temporal-reasoning`

This smoke proves the no-oracle relation path end to end, but it is not a
representative LongMemEval score. The next benchmark cut should run the same
LLM-built flow on a balanced multi-type subset.

## Balanced60 LLM-Built Relation Run

On May 5, 2026, the same balanced 60-item subset was run with evidence
relations built by `gpt-4o-2024-08-06` from raw conversations. The builder
prompt did not include the gold answer or `has_answer` labels.

Important correction: an earlier non-isolated run reused the default
`longmemeval:item:<question_id>` `about` values after the oracle baseline had
already been ingested into the live kernel. That contaminated the retrieval
metric with stale oracle edges and was superseded. Clean benchmark runs must set
`--run-id` so `about`, turn refs, question refs, session scopes, evidence refs,
and idempotency keys are isolated.

```bash
cargo run -p rehydration-testkit --bin longmemeval_kmp_evidence_builder -- \
  --input /tmp/longmemeval-official-eval/longmemeval_oracle.json \
  --output /tmp/longmemeval-kmp-llm-balanced60-clean-labels \
  --endpoint https://api.openai.com/v1/chat/completions \
  --model gpt-4o-2024-08-06 \
  --provider openai \
  --api-key-env OPENAI_API_KEY \
  --max-tokens 512 \
  --temperature 0 \
  --per-question-type-limit 10 \
  --run-id lme-b60-llm-clean-20260505-a \
  --force

cargo run -p rehydration-testkit --bin longmemeval_kmp_adapter -- \
  --input /tmp/longmemeval-official-eval/longmemeval_oracle.json \
  --output /tmp/longmemeval-kmp-llm-balanced60-clean-artifacts \
  --per-question-type-limit 10 \
  --run-id lme-b60-llm-clean-20260505-a \
  --evidence-labels /tmp/longmemeval-kmp-llm-balanced60-clean-labels/evidence_labels.jsonl \
  --force

cargo run -p rehydration-testkit --bin longmemeval_kmp_runner -- \
  --artifacts /tmp/longmemeval-kmp-llm-balanced60-clean-artifacts \
  --output /tmp/longmemeval-kmp-llm-balanced60-clean-run \
  --endpoint http://rehydration-kernel.underpassai.com \
  --force
```

Builder summary:

- total items: 60
- selected turns: 104
- prompt tokens: 384,719
- completion tokens: 6,991
- elapsed time: 153.864 seconds

Adapter summary:

- total source items: 500
- prepared items: 60
- sessions: 102
- turns: 1209
- expected evidence turns: 111
- relation evidence turns: 104

Runner summary:

- ingested items: 60/60
- asked items: 60/60
- full evidence hits: 43/60
- partial evidence hits: 14/60
- missing evidence hits: 3/60
- lexical answer hits: 21/60
- elapsed time: 54.422 seconds

Evidence retrieval by type:

- `knowledge-update`: 7 full, 3 partial, 0 missing
- `multi-session`: 4 full, 6 partial, 0 missing
- `single-session-assistant`: 10 full, 0 partial, 0 missing
- `single-session-preference`: 4 full, 3 partial, 3 missing
- `single-session-user`: 9 full, 1 partial, 0 missing
- `temporal-reasoning`: 9 full, 1 partial, 0 missing

Reader plus official evaluator, using `gpt-4o-2024-08-06` as external reader
and `gpt-4o` as official metric model:

- reader lexical answer hits: 22/60
- reader prompt tokens: 30,995
- reader completion tokens: 767
- official accuracy: 48/60, `0.8`

Official accuracy by type:

- `knowledge-update`: 8/10
- `multi-session`: 6/10
- `single-session-assistant`: 9/10
- `single-session-preference`: 6/10
- `single-session-user`: 9/10
- `temporal-reasoning`: 10/10

Interpretation: on this deterministic balanced slice, isolated LLM-built
relations no longer preserve full expected-evidence coverage. The builder
misses some gold refs, especially in multi-session and preference cases. The
reader update improves `knowledge-update` from 5/10 in the contaminated run to
8/10 by passing ordered, structured evidence with relation sequence and
rationale. The preference reader must answer as a personalized assistant
response, not as a meta-summary of preferences; after that correction,
`single-session-preference` improves from 0/10 to 6/10 on the clean Balanced60
run.

## Preference Builder V6 Slice

On May 5, 2026, a focused `single-session-preference` slice was rerun with a
stricter evidence-builder and reader prompt. The builder prompt now uses a
compact-but-complete preference evidence policy instead of a minimal evidence
policy: missing a personal constraint is treated as worse than including one
extra related user turn. It also asks the builder to preserve transferable
preferences, follow-up refinements, concrete examples, and workflow subgoals.

Earlier focused preference attempts were superseded:

- v2: 7/10 official accuracy; kitchen and hotel still failed.
- v3: 7/10 official accuracy; kitchen passed, but hotel, cultural-events, and
  slow-cooker failed.

```bash
cargo run -p rehydration-testkit --bin longmemeval_kmp_evidence_builder -- \
  --input /tmp/longmemeval-official-eval/longmemeval_oracle.json \
  --output /tmp/longmemeval-kmp-pref-v6-labels \
  --endpoint https://api.openai.com/v1/chat/completions \
  --model gpt-4o-2024-08-06 \
  --provider openai \
  --api-key-env OPENAI_API_KEY \
  --max-tokens 768 \
  --temperature 0 \
  --question-type single-session-preference \
  --per-question-type-limit 10 \
  --run-id lme-pref-v6-20260505-a \
  --force
```

Preference v6 summary:

- total items: 10
- selected turns: 32
- builder prompt tokens: 56,120
- builder completion tokens: 2,483
- expected evidence turns: 17
- relation evidence turns: 32
- retrieval: 7 full, 3 partial, 0 missing
- reader prompt tokens: 6,875
- reader completion tokens: 540
- official preference accuracy: 9/10, `0.9`

Remaining failure:

- slow cooker: builder selected beef stew and plant-based recipe interest but
  missed the yogurt-specific turn required by the rubric.

No deterministic post-processing was added for this remaining failure. The
slice remains an LLM-built relation run: the builder does not use the gold
answer or `has_answer`, and the expected refs stay isolated in `expected.jsonl`
for measurement only.

## Balanced60 V6 Full Run

On May 5, 2026, the full Balanced60 slice was rerun with the v6 preference
builder/reader prompts and a fresh isolated run id:
`lme-b60-v6-20260505-a`.

Artifacts:

- labels: `/tmp/longmemeval-kmp-b60-v6-labels`
- adapter: `/tmp/longmemeval-kmp-b60-v6-artifacts`
- kernel run: `/tmp/longmemeval-kmp-b60-v6-run`
- reader: `/tmp/longmemeval-kmp-b60-v6-reader-openai`
- official eval:
  `/tmp/longmemeval-kmp-b60-v6-reader-openai/hypotheses.jsonl.eval-results-gpt-4o`

Summary:

- total items: 60
- selected turns: 119
- builder prompt tokens: 405,264
- builder completion tokens: 9,762
- expected evidence turns: 111
- relation evidence turns: 119
- retrieval: 47 full, 13 partial, 0 missing
- reader prompt tokens: 30,180
- reader completion tokens: 709
- official accuracy: 50/60, `0.8333`

Official accuracy by type:

- `knowledge-update`: 8/10
- `multi-session`: 6/10
- `single-session-assistant`: 10/10
- `single-session-preference`: 7/10
- `single-session-user`: 9/10
- `temporal-reasoning`: 10/10

## 100-Item Prefix V6 Run

The next scale step used `--limit 100` with run id
`lme-100-v6-20260505-a`. This is not a balanced slice. The deterministic
dataset prefix contains only:

- `temporal-reasoning`: 60
- `multi-session`: 40

Artifacts:

- labels: `/tmp/longmemeval-kmp-100-v6-labels`
- adapter: `/tmp/longmemeval-kmp-100-v6-artifacts`
- kernel run: `/tmp/longmemeval-kmp-100-v6-run`
- reader: `/tmp/longmemeval-kmp-100-v6-reader-openai`
- official eval:
  `/tmp/longmemeval-kmp-100-v6-reader-openai/hypotheses.jsonl.eval-results-gpt-4o`

Summary:

- total items: 100
- answerable evidence items: 94
- abstention items: 6
- selected turns: 229
- builder prompt tokens: 1,080,608
- builder completion tokens: 18,701
- expected evidence turns: 259
- relation evidence turns: 229
- retrieval: 59 full, 33 partial, 2 missing
- reader prompt tokens: 54,072
- reader completion tokens: 262
- official accuracy: 74/100, `0.74`

Official accuracy by type:

- `multi-session`: 23/40, `0.575`
- `temporal-reasoning`: 51/60, `0.85`

The 100-item run should block the full 500-item run until the multi-session
coverage gap is improved. The kernel can traverse multiple session dimensions,
but relation construction still often selects only a subset of the counted
evidence, which produces partial retrieval and incomplete aggregate answers.

## Relation Semantics Investigation

On May 5, 2026, the 100-item prefix v6 run exposed a structural limitation in
the benchmark harness:

- all 229 LLM-built evidence relations were emitted as `supports_answer`;
- all 229 had `class=evidential`;
- all 229 had `confidence=high`;
- no relation type distinguished count members, sum operands, temporal anchors,
  superseded facts, preference constraints, excluded candidates, or comparison
  operands.

This mattered most for `multi-session`. The kernel was not losing the selected
relations. In the 100-item v6 run, every relation selected by the builder was
observed in the kernel output:

- `multi-session`: expected refs 137, selected refs 110, selected gold refs 96,
  expected-not-selected refs 41, selected-not-observed refs 0;
- `temporal-reasoning`: expected refs 122, selected refs 119, selected gold
  refs 105, expected-not-selected refs 17, selected-not-observed refs 0.

Interpretation: the main failure was upstream relation construction and answer
aggregation, not multidimensional kernel traversal. Session was already modeled
as a dimension, and `kernel_ask` recovered the selected evidence. The weak part
was that every selected edge had the same semantic meaning:
`supports_answer`.

### Relations Versus Derivations

The relation problem is narrower than "we need more relation labels". Relations
are valuable when the kernel needs to explain why two facts are connected,
recover provenance, navigate temporal order, expose causal chains, or show an
inspectable path from a query to evidence. They are not enough to compute an
aggregate answer.

For example, the first money value found in a relevant turn is not necessarily a
sum operand. It might be:

- a real paid amount that should be included;
- a price quote that was never paid;
- a budget or target;
- a refund or discount that must be subtracted;
- a cancelled or superseded amount;
- a comparison value from another option;
- context that explains the situation but should not be counted.

Even a typed edge such as `sum_operand` is still too coarse if it points to a
whole turn. A single turn can contain multiple values with different roles. The
answer path needs operand-level semantics:

- operation: `sum`, `count`, `difference`, `max_by`, `latest`, or another
  supported derivation;
- predicate scope: the exact user condition, such as paid workshops within the
  last four months;
- candidate lifecycle: candidate, included operand, excluded candidate,
  superseded candidate, negative evidence, or context-only;
- normalized operand: raw span, normalized value, unit, currency or date type,
  entity key, and source ref;
- computation proof: the deterministic arithmetic or comparison that produced
  the final answer.

This is the reason the active cut keeps `supports_answer` as the evidential
edge and moves aggregate semantics to derivation plugins. The kernel should
retrieve and expose the inspectable substrate; a typed extractor plus operation
plugins should build the derivation object that says which operands count and
why.

### Hypothesis

The hypothesis was that a typed evidence relation taxonomy could help both the
kernel and the external reader use multidimensional context more effectively.
The proposed closed catalog was:

- `supports_answer`
- `aggregation_member`
- `count_member`
- `sum_operand`
- `comparison_operand`
- `temporal_anchor`
- `temporal_before`
- `temporal_after`
- `latest_value`
- `superseded_value`
- `preference_signal`
- `entity_attribute`
- `excluded_candidate`

The intended behavior was fail-fast:

- the builder would have to choose one relation type from the closed catalog;
- unknown relation types would be rejected;
- the adapter would preserve the chosen relation in
  `memory.relations[].rel`;
- the runner and reader would treat those typed relations as valid answer
  evidence;
- the reader prompt would receive the relation type as structured evidence.

### Results

The experiment proved that typed relation labels are technically feasible, but
not sufficient as a benchmark improvement.

| Attempt | Slice | Builder output | Kernel retrieval | Official accuracy | Outcome |
| --- | --- | --- | --- | --- | --- |
| `rel-v2` smoke | 3 `multi-session` items | 7 `count_member` | 0 full, 3 partial, 0 missing | not claimed | E2E typed relation path worked, but selected evidence was incomplete |
| `rel-v2` | 10 `multi-session` items | 24 relations: 14 `count_member`, 5 `sum_operand`, 4 `temporal_anchor`, 1 `temporal_before` | 3 full, 7 partial, 0 missing | 7/10 | Best typed run, but only a small slice and still partial coverage |
| `rel-v3` | 10 `multi-session` items | 22 relations: 15 `count_member`, 5 `sum_operand`, 1 `temporal_anchor`, 1 `temporal_before` | 4 full, 6 partial, 0 missing | 6/10 | Stricter prompt selected less evidence and regressed |
| `rel-v4` | 10 `multi-session` items | 26 relations: 17 `count_member`, 5 `sum_operand`, 2 `excluded_candidate`, 1 `supports_answer`, 1 `temporal_anchor` | 3 full, 7 partial, 0 missing | 5/10 | Aggregate ledger produced richer labels but worse answers |

Additional validation:

- `rel-v2` proof paths from the live public kernel included the typed
  relations, so the kernel could store and return them.
- Re-running the reader over `rel-v2` with relation semantics in the prompt
  stayed at 7/10. The extra relation text did not fix missing evidence.
- `rel-v4` demonstrated that forcing a candidate ledger can make the model
  produce more structured output, including `excluded_candidate`, but it also
  increased omission and counting mistakes.

### Possible Failure Causes

The evidence so far points to multiple plausible causes. These are hypotheses,
not final conclusions, and each should be validated with a targeted experiment.

Relation construction may be weak because:

- The builder selects evidence turns directly from raw sessions instead of first
  constructing an explicit candidate set. For aggregate questions, this lets it
  stop after a few obvious matches and miss later or earlier members.
- There is no dedicated embedding-retrieval stage before evidence labeling.
  The builder currently has to scan and reason over raw candidate text itself;
  a local embedding model could propose a broader semantic candidate pool across
  dimensions before the LLM or deterministic derivation step classifies it.
- The builder operates at turn granularity. A single turn can contain multiple
  answer operands, such as two model kits or two costs, but one edge from the
  turn to the question cannot represent the internal items separately.
- Value mentions are not self-classifying. A dollar amount, date, duration, or
  item name must be interpreted against the requested operation and predicate
  before it can become an operand.
- The question intent is not represented as a typed query plan. The model has
  to infer whether the question asks for count, sum, latest value, comparison,
  exclusion, duration, or entity deduplication from prose every time.
- There is no explicit candidate lifecycle. The system does not distinguish
  `candidate`, `positive_operand`, `negative_operand`, `excluded_candidate`,
  `superseded_candidate`, and `context_only` before creating the final answer
  evidence.
- Negative evidence is missing from the active representation. For questions
  like "how many did I visit?" or "how many did I spend?", assistant
  suggestions and future plans can be close lexical matches but should not be
  counted.
- Entity normalization is too shallow. The builder must infer whether repeated
  mentions are the same entity, a refinement, or a new item without a stable
  entity id or deduplication key.
- Temporal qualifiers are not first-class in the selection step. Phrases like
  "last month", "since the start of the year", "the day before", and "current"
  are handled by prompt reasoning instead of a typed temporal filter.
- The prompt asks for evidence, not for an auditable derivation. A correct
  aggregate answer often needs intermediate state: included refs, excluded
  refs, normalized operands, arithmetic, and final value.

Path construction or path usage may be weak because:

- The current answer path is evidence-edge centric. It proves that a selected
  turn supports the question, but it does not encode the operation that combines
  multiple turns into an answer.
- `supports_answer` collapses very different path semantics into one relation:
  membership, arithmetic operand, update, temporal anchor, comparison side,
  preference constraint, and exclusion.
- The path does not expose a grouped aggregate structure. The reader sees an
  ordered list of snippets, not a typed set such as "these five refs are the
  counted members" or "these three refs are summed operands".
- Multi-dimensional traversal can recover evidence across sessions, but it does
  not currently generate all plausible cross-session candidates on its own. The
  path is bounded by whatever the builder selected.
- The proof path mixes structural relations (`records`, `contains_entry`,
  `has_dimension`) with evidential relations. This is useful for audit, but a
  downstream reader still needs a clean answer-evidence projection.
- The active API has no explicit aggregate operator in the request or response.
  Without `count`, `sum`, `latest`, `dedupe`, `compare`, or `exclude`, the path
  cannot prove how the final answer was computed.
- Relation sequence helps order evidence, but sequence alone does not say which
  older values are superseded, which values are current, or which events are
  before/after anchors.

Reader and evaluation gaps may also contribute:

- The reader must infer aggregate computation from recovered snippets. If the
  snippets are partial, the reader can still produce a plausible but wrong
  number.
- The reader currently receives evidence text and some metadata, but no formal
  derivation object. This makes arithmetic and deduplication fragile.
- Lexical local scoring is intentionally weak for triage. Official LongMemEval
  scoring can mark some compact answers correct, but it cannot explain whether
  the failure came from missing evidence, wrong aggregation, or poor wording.

The strongest current diagnosis is therefore: kernel traversal is adequate for
selected evidence, but the system does not yet build or expose an exhaustive,
typed answer derivation path.

### Rollback Decision

The typed taxonomy was intentionally rolled back from active code.

Reasons:

- The best typed slice improved `multi-session` from the Balanced60 v6 category
  score of 6/10 to 7/10 on a 10-item slice, but this was not strong enough to
  justify changing the active adapter contract.
- The larger conceptual issue remained: the builder still missed gold evidence
  refs. Changing `supports_answer` to `count_member` or `sum_operand` did not
  make the builder exhaustive.
- The aggregate ledger design was more expressive but degraded official
  accuracy to 5/10 on the same slice.
- The active kernel API should not absorb benchmark-specific relation names
  until the relation model is useful beyond LongMemEval.
- Keeping the active harness binary makes the current claim cleaner: the kernel
  retrieves selected evidence; the known gap is evidence construction and
  aggregate reasoning.

After rollback, the active implementation is again:

- generated labels contain `turn_ref`, `reason`, and `confidence`;
- the adapter emits `rel="supports_answer"` for every selected evidence turn;
- runner and reader count only `supports_answer` and `supports` as answer
  evidence;
- typed relation artifacts remain experimental `/tmp` outputs and are not part
  of the committed API.

Rollback smoke:

```bash
cargo run -p rehydration-testkit --bin longmemeval_kmp_adapter -- \
  --input /tmp/longmemeval-official-eval/longmemeval_oracle.json \
  --output /tmp/longmemeval-kmp-back-to-supports-artifacts \
  --evidence-labels /tmp/longmemeval-kmp-ms10-rel-v2-labels/evidence_labels.jsonl \
  --question-type multi-session \
  --limit 3 \
  --run-id lme-ms10-rel-v2-20260505-a \
  --force

jq -r '.arguments.memory.relations[]?.rel' \
  /tmp/longmemeval-kmp-back-to-supports-artifacts/ingest.jsonl | sort | uniq -c
```

Observed output:

```text
      7 supports_answer
```

### Kernel Improvement Backlog

This investigation should not be treated as wasted work. It identified concrete
kernel and harness improvements that are broader than LongMemEval.

Potential next cuts:

- Derivation plugin contracts: represent count, sum, min/max, comparison,
  latest, date math, currency handling, deduplication, and exclusion outside
  kernel core. The kernel may pass retrieved evidence and stable refs to those
  plugins, but it must not own the business rules for deciding which values
  count as operands.
- Candidate-set construction: let the kernel or an upstream service construct
  all plausible candidates across dimensions before the LLM labels which ones
  are positive, excluded, or contextual.
- Local embedding model: add an internal embedding/candidate-retrieval component
  so broad semantic recall does not depend entirely on the evidence-builder
  prompt. This should remain an implementation detail or explicit inference
  port, not turn KMP into a vector database API.
- Negative evidence support: model excluded candidates explicitly when they are
  needed to avoid over-counting.
- Reader contract separation: keep `kernel_ask` deterministic, but expose
  enough structured evidence for an external reader to compute aggregate
  answers without guessing relation semantics from prose.
- Benchmark-independent relation taxonomy: if relation types return, define
  them in the domain/API layer with use cases outside LongMemEval, then map
  LongMemEval into that taxonomy.

The next benchmark work should therefore focus on exhaustive candidate
construction and aggregate reasoning, not on merely renaming evidential edges.

## LongMemEval Smart Writer Slice

On May 8, 2026, the LongMemEval runner gained an optional benchmark-specific
smart writer path. It intentionally duplicates the MemoryArena writer harness
instead of sharing that module, because the two benchmarks have different graph
shapes and should not silently change each other's behavior.

This slice is a writer-quality benchmark, not evidence discovery. The
smart-writer consumes `supports_answer` / `supports` candidate relations already
present in the generated artifacts, reads the target/question through MCP, and
rewrites those candidate edges through `kernel_write_memory` with richer,
auditable relation semantics. A blind LongMemEval claim still requires an
upstream candidate generator, such as the embedding candidate stage or the LLM
evidence builder, before this writer step.

Flow:

1. `longmemeval_kmp_adapter` still creates the normal LongMemEval ingest and ask
   artifacts.
2. `longmemeval_kmp_runner --smart-writer` first ingests the item normally so all
   turn/question refs exist.
3. The LongMemEval smart writer then reads each candidate target with
   `kernel_near` and `kernel_inspect`.
4. The writer LLM emits one strict `kernel_write_memory.connect_to` relation per
   evidence relation.
5. The runner dry-runs, commits, verifies with `kernel_inspect`, and records
   writer quality diagnostics in `writer_results.jsonl`.

The first public TLS smoke used 3 `multi-session` items:

```text
artifacts: /tmp/lme-ms3-smart-writer-artifacts-20260508
run:       /tmp/lme-ms3-smart-writer-run-20260508
reader:    /tmp/lme-ms3-smart-writer-reader-20260508
endpoint:  https://rehydration-kernel.underpassai.com
model:     gpt-4o-2024-08-06
```

Observed smart-writer metrics:

```text
relation_writes: 12
llm_calls: 12
llm_valid_outputs: 12
deterministic_fallbacks: 0
relation_rich_count: 12
relation_anemic_count: 0
relation_structural_count: 0
relation_suspect_count: 0
```

Relation distribution:

```text
10 component_of / evidential
 2 excluded_from / constraint
```

Reader and official evaluator:

```text
reader graph_relation_rich: 12
reader lexical_answer_hits: 2/3
official accuracy: 2/3, 0.6667
```

The 10-item `multi-session` slice then compared directly against the previous
10-item slice without the smart writer:

```text
without smart writer:
  artifacts: /tmp/lme-ms10-core-relation-vocab-artifacts-20260508
  run:       /tmp/lme-ms10-core-relation-vocab-run-20260508
  reader:    /tmp/lme-ms10-core-relation-vocab-reader-20260508
  full evidence hits: 10/10
  reader graph_relation_rich: 0
  reader lexical_answer_hits: 4/10
  official accuracy: 6/10, 0.6

with LongMemEval smart writer:
  artifacts: /tmp/lme-ms10-smart-writer-artifacts-20260508
  run:       /tmp/lme-ms10-smart-writer-run-20260508
  reader:    /tmp/lme-ms10-smart-writer-reader-20260508
  full evidence hits: 10/10
  writer relation_writes: 32
  writer llm_valid_outputs: 32
  writer deterministic_fallbacks: 0
  writer relation_rich_count: 32
  writer relation_anemic_count: 0
  reader graph_relation_rich: 30
  reader lexical_answer_hits: 5/10
  official accuracy: 7/10, 0.7
```

Writer relation distribution on the 10-item slice:

```text
28 component_of / evidential
 2 excluded_from / constraint
 2 supports / evidential
```

This is the first LongMemEval run where the reader sees rich relation semantics
created through the Kernel write protocol rather than through adapter-side
renaming. The improvement is modest but real on this small slice. The remaining
failures still point at aggregate reasoning and domain-specific operators:
money totals, entity deduplication, and cases where the official evaluator is
stricter than the short numeric hypothesis.

### 30-Item Multi-Session Smart-Writer Comparison

On May 9, 2026, the next scale step used the first 30 `multi-session` items in
two isolated runs over the same logical slice:

```text
baseline:
  artifacts: /tmp/lme-ms30-baseline-artifacts-20260509
  run:       /tmp/lme-ms30-baseline-run-20260509
  reader:    /tmp/lme-ms30-baseline-reader-20260509
  run_id:    lme-ms30-baseline-20260509-a

smart writer:
  artifacts: /tmp/lme-ms30-smart-writer-artifacts-20260509
  run:       /tmp/lme-ms30-smart-writer-run-20260509
  reader:    /tmp/lme-ms30-smart-writer-reader-20260509
  run_id:    lme-ms30-smart-writer-20260509-a
```

Both runs used the public TLS kernel endpoint
`https://rehydration-kernel.underpassai.com`, `gpt-4o-2024-08-06` for the
reader, and the official LongMemEval `gpt-4o` evaluator. This is still a
writer-quality comparison over candidate relations already present in the
artifacts, not a blind evidence-discovery score.

| Metric | Baseline | Smart writer |
| --- | ---: | ---: |
| Items | 30 | 30 |
| Expected evidence turns | 104 | 104 |
| Full evidence hits | 30 / 30 | 30 / 30 |
| Writer LLM calls | 0 | 104 |
| Writer valid outputs | 0 | 104 |
| Writer deterministic fallbacks | 0 | 0 |
| Writer rich relations | 0 | 104 |
| Reader graph rich relations | 0 | 98 |
| Reader lexical hits | 19 / 30 | 19 / 30 |
| Reader prompt tokens | 32,393 | 44,845 |
| Official accuracy | 21 / 30, `0.7000` | 22 / 30, `0.7333` |

Smart-writer relation distribution:

```text
91 component_of / evidential
 6 supports / evidential
 4 excluded_from / constraint
 2 contributes_to / evidential
 1 matches_requirement / constraint
```

Outcome deltas:

```text
baseline false -> smart true:
  0a995998: clothing pickup/return count, 2 items -> 3 items
  gpt4_a56e767c: movie festivals, 3 -> 4
  7024f17c: jogging/yoga hours, 2.5 hours -> 0.5 hours
  2ce6a0f2: art-related events, UNKNOWN -> 4

baseline true -> smart false:
  gpt4_59c863d7: model kits, 5 -> 6
  gpt4_d84a3211: bike expenses, $185 -> $225
  gpt4_15e38248: furniture pieces, 4 pieces -> 5 pieces
```

The 30-item slice confirms that rich writer relations are visible to the reader
and can improve some aggregate answers, but they do not yet solve aggregate
operator correctness. The remaining failures still concentrate around counting,
deduplication, currency inclusion/exclusion, and average calculation. The
official judge also appears to mark at least one compact correct numeric answer
as false (`gpt4_2f8be40d`, answer says three weddings and the hypothesis is
`3`), so failure analysis should use both official labels and artifact-level
inspection.

### 30-Item Multi-Session Gemma 4 31B Reader Check

The same smart-writer artifacts were read again through the cluster-hosted
Gemma 4 31B vLLM endpoint exposed at `https://llm.underpassai.com` with mTLS.
The kernel run and recovered evidence were unchanged; only the external reader
model changed.

```text
reader:   /tmp/lme-ms30-smart-writer-reader-gemma4-20260509
model:    google/gemma-4-31B-it
endpoint: https://llm.underpassai.com/v1/chat/completions
judge:    official LongMemEval gpt-4o evaluator
```

| Metric | GPT-4o reader | Gemma 4 31B reader |
| --- | ---: | ---: |
| Items | 30 | 30 |
| Full evidence hits | 30 / 30 | 30 / 30 |
| Reader graph rich relations | 98 | 98 |
| Reader lexical hits | 19 / 30 | 20 / 30 |
| Reader prompt tokens | 44,845 | 53,932 |
| Reader completion tokens | not recorded here | 96 |
| Official accuracy | 22 / 30, `0.7333` | 25 / 30, `0.8333` |

Gemma fixed five answers that `gpt-4o` missed:

```text
gpt4_59c863d7: model kits, 6 -> 5
gpt4_d84a3211: bike expenses, $225 -> $185
gpt4_f2262a51: doctor count, 4 -> 3
gpt4_15e38248: furniture pieces, 5 pieces -> 4
gpt4_5501fe77: follower platform, Twitter -> TikTok
```

Gemma regressed two answers that `gpt-4o` got right:

```text
0a995998: clothing pickup/return count, 3 items -> 2 items
46a3abf7: aquarium tank count, 3 -> UNKNOWN
```

This is not yet a full official LongMemEval score; it is a model-swap check on
the same 30-item oracle multi-session slice. It is still useful: the kernel
evidence substrate is model-independent, and a local/open reader can outperform
the previous hosted reader on aggregate interpretation when the same recovered
context and rich relations are provided.

### Next Serious Run Plan

The next publishable LongMemEval run should be treated as an official-style
secondary regression, not as the primary product benchmark:

- dataset: `longmemeval_s_cleaned.json`;
- sample: 500 questions;
- context construction: real kernel retrieval over the available history;
- forbidden fields: do not use `has_answer`, `answer_session_ids`, or gold
  answer metadata to build reader context;
- reader: document the exact hosted or GPU-backed model;
- judge: document the exact judge model and evaluator command;
- artifacts: preserve generated KMP artifacts, run output, reader output,
  official evaluator output, and failure analysis.

RunPod is the preferred external GPU path for the next long-context reader
validation because local 4x RTX 3090 capacity is not enough for the target
Gemma long-context configuration. The first cloud step should be a short smoke
that validates model loading, context length, tokens/s, TLS/API plumbing, and
cost before running the full 500-question pass.

## Embedded backend parity run (2026-07-23)

First LongMemEval run against the **embedded edition** (in-process kernel
over redb, `REHYDRATION_MCP_BACKEND=embedded`, fresh store; zero
infrastructure, zero LLM — deterministic kernel-side scoring only):

| Metric | Embedded run | Recorded infra reference |
| --- | --- | --- |
| Dataset | `longmemeval_oracle.json`, 500 items | 100-item scale run |
| Ingested / asked | 500 / 500 | — |
| Full evidence hits | **470/470 (100%)** — 0 partial, 0 missing | 99.28% recall, 98/100 full-hit |
| Abstention items | 30/30 | — |
| Lexical answer hits (reader-less) | 199/470 | reader is the bottleneck (34/46 adjusted lexical baseline) |
| Wall clock | 112.6 s total | — |

Reproduction (no code changes — the runners select the backend by env):

```bash
cargo build --release -p rehydration-testkit \
  --bin longmemeval_kmp_adapter --bin longmemeval_kmp_runner
target/release/longmemeval_kmp_adapter \
  --input longmemeval_oracle.json --output <artifacts>
REHYDRATION_MCP_BACKEND=embedded REHYDRATION_MCP_DATA_DIR=<fresh dir> \
  target/release/longmemeval_kmp_runner --artifacts <artifacts> --output <run>
```

Conclusion: embedded storage recall is at parity with the infrastructure
backend (E6 parity evidence, deterministic phase). End-to-end answer
quality (LLM reader phase) remains a reader concern, identical across
backends by construction.
