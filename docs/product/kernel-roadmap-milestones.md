# Kernel Roadmap Milestones

Last updated: 2026-05-06
Status: active planning document

## Product Direction

KMP is a queryable, temporal, multidimensional memory layer for
agentic processes.

The kernel should let humans and LLMs search memory, inspect what was known at
a point in time, understand why decisions were made, trace failed and successful
paths, and audit the evidence behind a final outcome.

Non-negotiables:

| Requirement | Meaning |
| --- | --- |
| Easy to use | Small KMP API, simple request shapes, safe defaults, fail-fast errors, usable summaries, inspectable proof. |
| Observable | Operators can see request flow, event flow, projection lag, traversal scope, proof quality, and agent outcomes. |
| Auditable | A decision can be reconstructed from refs, evidence, relations, temporal state, and provenance. |
| Secure | Transport, scopes, raw inspect, secrets, logs, traces, and sensitive memory are protected explicitly. |
| Infrastructure-independent | Neo4j, NATS, Valkey, Kubernetes, MCP, and gRPC are implementations or bindings, not the product itself. |

## Strategy Decisions

- LongMemEval stays as a secondary conversational-memory regression.
- The primary benchmark direction moves to agentic memory, starting with
  MemoryArena.
- Aggregate QA operators do not belong in core.
- Derivation logic such as `sum`, `count`, money, date math, `max_by`, and
  dedupe belongs in plugins above the kernel.
- The core owns memory ingestion, dimensions, temporal coordinates, relations,
  evidence, deterministic traversal, proof, trace, inspect, and known-at-time
  semantics.
- SOTA work is adopted only when it improves queryable process memory without
  contaminating the core with infrastructure or domain logic.

## Milestone 0: Roadmap And Product Baseline

Priority: P0

Status: in progress

Goal:

Establish the product direction and stop optimizing the core around the wrong
benchmark shape.

Deliverables:

- queryable agentic memory product direction;
- benchmark strategy that makes LongMemEval secondary;
- explicit operator/plugin boundary;
- observable, auditable, secure product requirement;
- SOTA technique shortlist filtered by what rents.

Exit criteria:

- `docs/product/queryable-agentic-memory-layer.md` documents the product thesis;
- `docs/research/agentic-memory-benchmark-strategy-2026-05-06.md` documents the
  benchmark pivot;
- `docs/research/longmemeval-benchmark.md` documents LongMemEval as secondary;
- roadmap is linked from the documentation index.

## Milestone 1: Agentic Benchmark Feasibility

Priority: P0

Status: started. The first MemoryArena-to-KMP artifact adapter, live
stage-aware runner, and paper-aligned local scorecard are available in
`kmp-testkit` as `memoryarena_kmp_adapter`,
`memoryarena_kmp_runner`, and `memoryarena_kmp_scorecard`. The first
MemoryAgentBench-to-KMP adapter and live runner are available as
`memoryagentbench_kmp_adapter` and `memoryagentbench_kmp_runner`.

Goal:

Prove the kernel against an external benchmark that matches agentic process
memory better than chat QA.

Primary target:

- MemoryArena.
- MemoryAgentBench.

First slices:

- `progressive_search` for process/search memory;
- `group_travel_planner` for multidimensional people, constraints, and
  preferences.
- MemoryAgentBench `Conflict_Resolution` / `factconsolidation_mh_32k` for
  inject-once/query-many conflict and stale-fact memory.

Deliverables:

- MemoryArena dataset loader in `kmp-testkit`;
- adapter from MemoryArena tasks to KMP memory artifacts;
- MemoryAgentBench dataset loader in `kmp-testkit`;
- adapter from MemoryAgentBench rows to KMP inject-once/query-many artifacts;
- baseline runner without kernel;
- kernel-backed stage-aware runner;
- replay artifacts: timeline, dimensions, evidence refs, failed path, final
  path, known-at-time snapshots;
- scorecard with task success, evidence recall, known-at-time correctness,
  replay completeness, auditability, latency, and cost.

Exit criteria:

- at least one MemoryArena slice runs end to end;
- artifacts can be inspected by a human without reading raw transcripts;
- an LLM can use KMP moves to answer/replay without knowing backend topology;
- failures are classified as ingestion, projection, retrieval, proof, model
  consumption, or task reasoning.

Current proof:

- a fixture MemoryArena task generates staged KMP artifacts;
- the live runner replays the artifact stream against
  `http://kmp.underpassai.com`;
- the smoke run reached 7/7 successful events, 2/2 known-at-clean asks, 0 future
  answer leaks, and complete observed refs for the allowed known-at evidence.
- a real `progressive_search` slice from `ZexueHe/memoryarena` reached 81/81
  successful events, 27/27 known-at-clean asks, 0 future answer leaks, and 0
  missing allowed refs against the deployed kernel.
- the paper-aligned local scorecard over that slice reached 3/3 task
  successes, 24/27 hard-correct subtasks, `PS=0.8796`, and 3/3
  candidate-answer hits, while preserving the distinction between kernel
  substrate retrieval and benchmark answer consumption.
- a realistic MemoryArena 2x/domain slice across all five configs reached
  221/221 successful KMP events, 73/73 known-at-clean asks, 73/73 full ref
  recall, 0 future leaks, and 0 unexpected or missing refs against the deployed
  kernel. The local task score was 3/10 task successes and 29/73 hard-correct
  subtasks, which classifies the next gap as reader/agent task reasoning over
  correct kernel evidence rather than kernel retrieval failure.
- the MemoryArena scorecard now emits local failure classes. In the same
  2x/domain slice the distribution was 29 `passed`, 6
  `no_prior_answer_evidence`, 23 `domain_agent_solution_gap`, and 15
  `formal_reader_reasoning_gap`, with no kernel evidence failure class.
- a fixture MemoryAgentBench Conflict Resolution row generates inject-once KMP
  artifacts with 1 ingest event, 2 ask events, 4 context refs, and complete
  known-at snapshots.
- the MemoryAgentBench fixture live runner reached 3/3 successful events, 2/2
  known-at-clean asks, 2/2 lexical answer hits, 0 unexpected refs, and 0
  missing refs against `http://kmp.underpassai.com`.
- a real official MemoryAgentBench `Conflict_Resolution` /
  `factconsolidation_mh_6k` smoke reached 4/4 successful KMP events, 3/3
  known-at-clean asks, 0 unexpected refs, and 0 missing refs with a 64-fact /
  3-query bounded slice. It also exposed that current `kernel_ask` is a generic
  deterministic evidence summary, not a MemoryAgentBench-grade question reader.
- a real public-TLS MemoryArena `progressive_search` 100-task smart-writer run
  reached 2259/2259 successful KMP events, 753/753 known-at-clean asks,
  753/753 full-ref recall, 0 future answer leaks, 0 unexpected refs, and 0
  missing refs. The local paper-aligned task score was 97/100, with all 3 final
  misses classified as reader answer-selection failures over complete evidence,
  not kernel retrieval or graph contamination failures.

Non-goals:

- do not claim SOTA benchmark performance from the first feasibility slice;
- do not add benchmark-specific behavior to core;
- do not remove the LongMemEval regression.

## Milestone 2: Hybrid Candidate Retrieval

Priority: P0

Goal:

Improve recall for process memory queries using SOTA retrieval composition while
keeping KMP independent of vector database semantics.

Technique:

- lexical search for exact ids, names, dates, amounts, and tool outputs;
- embedding search for semantic candidates;
- graph traversal for related entries and paths;
- temporal expansion around candidate refs;
- reranker or cross-encoder for final candidate ordering.

Deliverables:

- search/candidate port with stable ref output;
- paid embedding adapter for experiments;
- local embedding sidecar plan or adapter;
- reranker experiment behind a port;
- metrics: recall@K, selected dimensions, temporal expansion count, rerank
  latency, missing evidence count;
- no public KMP vector-search API.

Exit criteria:

- candidate retrieval can be evaluated independently from answer generation;
- retrieval output is a stable list of memory refs with scores and evidence
  handles;
- MemoryArena and LongMemEval can both use the same candidate pipeline;
- KMP remains a memory navigation API, not a vector DB API.

## Milestone 2B: Kernel Tool Operator Model

Priority: P1 after KMP limits/pagination are stable

Execution plan:

- [kernel-tool-operator-model-plan.md](kernel-tool-operator-model-plan.md)

Goal:

Train or fine-tune a small specialist model that knows how to operate the
kernel MCP/API, not a generalist model that owns memory reasoning. The model is
a tool operator for `ask`, `near`, `trace`, `inspect`, `goto`, `rewind`,
`forward`, and `kernel_write_memory`.

Why this is plausible:

- the task surface is narrow and tool-shaped;
- the desired behavior is observable as trajectories: state, selected tool,
  bounded arguments, result, next action, stop condition;
- a small model can learn kernel-specific navigation discipline better than a
  generic model if the MCP contract is stable;
- it keeps the kernel infrastructure-independent because the model remains a
  sidecar/client, not core logic.

Non-negotiable prerequisites:

- KMP and MCP must expose stable limits, pagination, scopes, and fail-fast
  errors before training;
- the training corpus must be generated from successful audited trajectories,
  not raw prompts or broken traces;
- the model must learn bounded navigation and pagination, so it does not
  overuse `trace` or request whole histories by default;
- evaluation must compare tool-use quality against a generalist LLM:
  correct refs, missing future leaks, relation quality, number of tool calls,
  latency, and stop accuracy.

Boundary:

- the operator model may choose tools and arguments;
- it may propose writer relations only with cited MCP-read evidence;
- it must not invent memory, bypass scopes, or become the public memory API;
- kernel retrieval, traversal, proof, validation, and audit remain deterministic
  kernel responsibilities.

## Milestone 3: Temporal Validity, Supersession, And Known-At-Time

Priority: P0

Goal:

Make temporal memory more than ordered entries. The kernel must represent which
facts, decisions, and evidence were active at a point in time.

Deliverables:

- validated semantics for `valid_from` and `valid_until`;
- explicit supersedes/superseded-by relation guidance;
- stale fact and contradicted fact warnings;
- known-at-time query helper over current KMP primitives;
- tests for current fact, stale fact, superseded decision, and conflict paths;
- demo artifact showing a decision that was reasonable at time `t` and stale
  later.

Exit criteria:

- callers can ask what was known at time/ref/sequence and receive bounded
  evidence;
- a trace can explain why a later decision superseded an earlier one;
- missing or conflicting temporal proof is reported explicitly.

## Milestone 4: Process Query Helpers

Priority: P1

Goal:

Expose common process questions as composed helpers above KMP without bloating
the core API.

Target helpers:

- `search_memory(scope, query)`;
- `known_at(time/ref, scope)`;
- `decision_at(time/ref, scope)`;
- `why(decision_ref)`;
- `evidence_for(ref)`;
- `failed_paths(scope)`;
- `successful_paths(scope)`;
- `final_path(scope)`;
- `compare_paths(criteria)`;
- `best_path(criteria)`.

Deliverables:

- helper contract document;
- proof shape for each helper;
- implementation for a minimal incident/demo slice;
- policy/plugin boundary for `compare_paths` and `best_path`.

Exit criteria:

- helpers are implemented as composition over `ask`, `goto`, `near`, `forward`,
  `rewind`, `trace`, and `inspect`;
- no helper requires storage-specific knowledge;
- every helper returns a usable layer and proof layer.

## Milestone 5: Observability, Audit, And Security Upgrade

Priority: P0/P1

Goal:

Make the memory layer operationally trustworthy for agents and humans.

Deliverables:

- projection lag metric recorded by the projection runtime;
- KMP request/response/error metrics by move;
- proof quality metrics: evidence count, missing count, warning count, conflict
  count, weak-proof count;
- traversal scope metrics/logs: selected abouts, dimensions, time window, path
  length, hop count;
- end-to-end OTel trace shape for KMP through event append/read, projection,
  traversal, proof, LLM/tool consumer, and optional write-back;
- audit log events for ingest, read, trace, inspect, and rejected access;
- privacy rules for logs/traces: no secrets, API keys, raw prompts, or
  unrestricted raw memory;
- security backlog for caller identity, RBAC, raw inspect permission, redaction
  policy, and data-at-rest strategy.

Exit criteria:

- an operator can tell whether a memory write became queryable;
- a failed agent answer can be classified by failure stage;
- trace and inspect access is visible in audit logs;
- metrics do not use high-cardinality user data as labels;
- sensitive raw memory does not leak through normal logs or traces.

## Milestone 6: Derivation Plugin Contracts

Priority: P1

Current status:

- reusable plugin contract implemented in the exportable lightweight crate
  `kmp-plugin-api`, with a kernel-domain re-export at
  `kmp_domain::plugins`;
- reusable plugin implementations live in `kmp_interpretation`;
- documented in `docs/product/kernel-plugin-architecture.md` and
  `docs/product/reusable-interpretation-plugins.md`;
- current scope treats currency and dates as domain plugins with their own
  extraction, normalization, deterministic operations, and fail-fast validation;
- MemoryArena interpretation probe validates those plugins against real runner
  evidence without using gold answers.

Goal:

Keep domain and benchmark operators outside core while still supporting
structured derivations when applications need them.

Initial plugin families:

- money/currency;
- date/duration;
- count/dedupe;
- max/min comparison;
- latest/current value;
- domain-specific best-path policy.

Deliverables:

- plugin contract: input evidence refs, typed operands, normalized values,
  included/excluded/context labels, computation proof, output memory write-back;
- example plugin using LongMemEval aggregate cases;
- raw evidence and derived memory provenance via `derived_from`;
- no core dependency on benchmark operators.

Exit criteria:

- a plugin can compute a derivation from kernel evidence and write an auditable
  derived result back into memory;
- core remains unaware of money/date/count semantics;
- LongMemEval aggregate QA improvements can happen without changing KMP core.

## Milestone 7: Infrastructure Independence

Priority: P1/P2

Goal:

Demonstrate that the kernel is the memory model and protocol, not a specific
storage distribution.

Deliverables:

- in-memory adapter for conformance tests;
- KMP conformance suite for ingest, wake, ask, temporal movement, trace,
  inspect, known-at-time, and proof;
- conformance run against in-memory and production adapters;
- port boundary review for domain/application crates;
- backend-specific behavior documented as adapter behavior, not protocol
  semantics.

Exit criteria:

- the same semantic tests pass against at least two backends;
- applications can reason about KMP without knowing Neo4j/NATS/Valkey topology;
- new storage adapters can be evaluated by conformance, not manual judgment.

## Milestone 8: Human And LLM API Ergonomics

Priority: P1

Goal:

Make KMP easy for both LLM tools and human API users.

Deliverables:

- concise examples for each memory move;
- canonical JSON examples for MCP and future HTTP/SDKs;
- fail-fast error examples;
- stable ref conventions;
- response shape guide: usable layer plus proof layer;
- minimal SDK or CLI helper for demos and benchmark harnesses;
- graph/replay visualization artifact for incident and benchmark runs.

Exit criteria:

- a human can replay a process from docs and artifacts;
- an LLM can call the API without storage-specific instructions;
- every response is compact enough for an agent and inspectable enough for an
  auditor.

## Milestone 9: Public Proof And Comparisons

Priority: P2

Goal:

Turn the architecture into a defensible public claim.

Deliverables:

- article/demo based on an agentic process, not flat chat QA;
- MemoryArena feasibility report;
- LongMemEval secondary regression report;
- comparison framing against Zep/Graphiti, LangGraph, Temporal, Letta, Mem0,
  Hindsight, Mastra OM, LangSmith/Phoenix/OTel;
- limitations section that states what is not solved;
- reproducible artifacts and commands.

Exit criteria:

- public claim is narrow, measured, and reproducible;
- the demo is visually inspectable with low cognitive load;
- the report shows where the kernel helps and where plugins or consumers still
  own reasoning.

## Execution Order

Recommended order:

1. Finish Milestone 0 documentation baseline.
2. Build Milestone 1 MemoryArena feasibility adapter and real-slice runner.
3. Add MemoryArena task-success scoring and an agentic retrieval loop.
4. Add Milestone 5 observability gaps needed to debug the benchmark run.
5. Implement Milestone 2 hybrid candidate retrieval behind ports.
6. Tighten Milestone 3 known-at-time and supersession semantics.
7. Add Milestone 4 process query helpers for the demo.
8. Define Milestone 6 plugin contracts after retrieval/replay is stable.
9. Start Milestone 7 infrastructure conformance once semantics stop moving.
10. Package Milestone 9 public proof only after reproducible runs exist.

## Current Next Slice

The immediate next slice is the benchmark/documentation hardening path that can
support a public article without overstating the kernel:

```text
TOP 1: remote scorecard/audit/replay must expose progress, paging, and resume
  points before larger benchmark runs are considered operationally
  reproducible
+ Temporal `goto`/`near`/`rewind`/`forward` and `trace` expose page metadata
  through the API/MCP contract; replay reports partial reads explicitly.
LongMemEval official 500-question regression preparation
+ RunPod/GPU reader validation when local VRAM is insufficient
+ MemoryArena remains the primary agentic-memory proof line
+ P1 operator-model plan waits until KMP limits/pagination/tool trajectories
  are stable
```

Execution order:

1. Keep the 30-item LongMemEval smart-writer/Gemma reader result documented as
   a model-swap check, not a full official score.
2. Prepare a serious 500-question LongMemEval run with the official cleaned
   split, no forbidden gold fields for context construction, real kernel
   retrieval over history, and explicit reader/judge configuration.
3. Use external GPU capacity only for the reader validation path that cannot
   fit locally; document provider, model, context length, cost, and artifacts.
4. Bring the result back into the roadmap as secondary regression evidence.
5. Continue the primary product proof through MemoryArena/MemoryAgentBench,
   where temporal replay, multidimensional process memory, and auditability are
   the core claim.

This roadmap validates the strongest product claim:

```text
KMP is an observable, auditable, secure memory substrate for
agentic processes. It preserves enough temporal and multidimensional structure
to replay what happened, why it happened, what failed, and what finally worked.
```
