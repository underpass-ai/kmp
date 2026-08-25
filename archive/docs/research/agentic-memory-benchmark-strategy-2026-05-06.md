# Agentic Memory Benchmark Strategy

Date: 2026-05-06
Status: planning note

## Decision

LongMemEval remains useful, but it should no longer define the primary product
benchmark track.

LongMemEval is best treated as a secondary conversational-memory regression:
multi-session recall, temporal recall, knowledge updates, abstention, retrieval
coverage, and evidence quality. It is not the main proof point for Underpass
Kernel because many hard LongMemEval failures are aggregate QA failures:
counting, money arithmetic, date normalization, entity deduplication, and
operation-specific derivation.

The main benchmark track should focus on agentic systems where memory and
action are coupled:

- the agent accumulates memory across sessions;
- later actions depend on earlier observations, failed attempts, preferences,
  policies, or environmental feedback;
- the kernel can replay how the process evolved;
- the output is inspectable as temporal, dimensional, relational, and evidence
  paths.

## Operator Boundary

The kernel core must not absorb benchmark or domain operators.

Out of core:

- `sum`, `count`, `average`, `difference`, `max_by`, `latest`, and similar
  aggregate operators;
- money, currency, date, duration, and unit normalization;
- entity deduplication rules;
- predicate-specific inclusion/exclusion rules;
- benchmark-specific answer formatting.

In core:

- durable memory ingestion;
- `about` and dimension scoping;
- temporal coordinates and traversal;
- relation and evidence storage;
- deterministic context retrieval;
- trace and inspect APIs;
- replayable known-at-time state.

If these operators are needed, they should live as derivation plugins above the
kernel. The plugin receives kernel evidence with stable refs, computes a typed
derivation object, and can write the derivation back as inspectable memory if
the product flow needs auditability.

## Fit Criteria

A benchmark is a good fit for the kernel when it can measure at least three of
these properties:

- inter-session memory reuse;
- temporal update and conflict handling;
- agent action consistency over time;
- policy or preference retention;
- reconstruction of failed and successful paths;
- provenance from final outcome back to evidence;
- deterministic inspection of what was known at a given point in time;
- state-based or programmatic evaluation, not only free-form QA judgment.

Token reduction can be reported, but it is not the central claim. The stronger
claim is replayable, inspectable process memory for agents.

## Primary Shortlist

### P0: MemoryArena

Source: `https://memoryarena.github.io/`

Paper: `https://arxiv.org/abs/2602.16313`

Dataset: `https://huggingface.co/datasets/ZexueHe/memoryarena`

Why it fits:

- released in 2026;
- explicitly targets multi-session `Memory-Agent-Environment` loops;
- couples memory acquisition with later decision-making;
- includes interdependent subtasks;
- covers web navigation, preference-constrained planning, progressive search,
  group travel planning, and sequential formal reasoning.

Kernel hypothesis:

The kernel should improve replayability and cross-session context recovery for
interdependent subtasks. It should also expose why a later subtask reused or
ignored memory from earlier subtasks.

First adapter slice:

- use `progressive_search` or `group_travel_planner`;
- map each task id to `about=memoryarena:task:<id>`;
- map each subtask to a temporal episode dimension;
- map personas, plans, preferences, search results, and feedback as dimensions;
- evaluate baseline agent versus kernel-backed agent on final task success and
  replay completeness.

### P0: MemoryAgentBench

Source: `https://iclr.cc/virtual/2026/poster/10010781`

Code: `https://github.com/HUST-AI-HYZ/MemoryAgentBench`

Why it fits:

- ICLR 2026;
- evaluates memory agents through incremental multi-turn interactions;
- targets accurate retrieval, test-time learning, long-range understanding, and
  selective forgetting;
- closer to real agent memory than static long-context QA.

Kernel hypothesis:

The kernel should make memory state explicit across increments and support
auditable update/forget behavior instead of opaque prompt accumulation.

First adapter slice:

- load one competency at a time;
- compare raw context, simple RAG, and kernel traversal;
- add inspection metrics: update trace, conflict trace, and known-at-time
  correctness.

### P1: BEAM

Source: `https://iclr.cc/virtual/2026/poster/10006595`

Project page: `https://mohammadtavakoli78.github.io/beam-light/`

Why it fits:

- ICLR 2026;
- tests long-term memory over coherent conversations up to very large token
  scales;
- includes 100 conversations and 2,000 validated questions;
- stresses long-term episodic memory, working memory, and salient-fact
  accumulation.

Why it is P1, not P0:

BEAM is recent and memory-centric, but still conversation-memory oriented. It is
useful for scale and long-horizon regression, not the clearest agentic process
benchmark.

### P1: tau-bench

Source: `https://huggingface.co/papers/2406.12045`

Code: `https://github.com/sierra-research/tau-bench`

Why it fits:

- dynamic tool-agent-user conversations;
- domain APIs and policy guidelines;
- evaluates final database state against a goal state;
- includes reliability over repeated trials through `pass^k`;
- closer to production agent behavior than static QA.

Why it is P1, not P0:

tau-bench is excellent for tool-use reliability and policy adherence, but it is
not primarily a long-term memory benchmark. The kernel value would be measured
through replay, policy provenance, and consistency over interaction history.

### P1: AppWorld

Source: `https://appworld.dev/`

Paper: `https://aclanthology.org/2024.acl-long.850/`

Why it fits:

- realistic autonomous agent tasks across 9 apps and 457 APIs;
- 750 tasks;
- programmatic state and execution-based evaluation;
- checks for collateral damage, not only final text answers.

Why it is P1, not P0:

AppWorld is strong for agent action and stateful APIs, but the memory value may
need an added multi-session protocol or replay harness.

### P2: WorkArena

Source:
`https://www.servicenow.com/research/publication/alexandre-drouin-work-icml2024.html`

Why it fits:

- realistic enterprise web-agent tasks;
- stateful software interaction;
- knowledge-worker workflows with SOPs and contextual judgment.

Why it is P2:

It is realistic, but heavier operationally and less directly focused on
long-term memory. It is better after the kernel has a stable agentic benchmark
harness.

## Secondary Conversational Memory Track

These benchmarks remain useful, but they should not dominate kernel product
positioning:

- LongMemEval: current implemented adapter; keep as secondary regression for
  multi-session and temporal retrieval.
- LoCoMo: long-term multi-session conversation memory with temporal and causal
  dynamics.
- MemBench: factual and reflective memory across participation and observation
  scenarios.
- LoCoMo-Plus: cognitive memory and latent constraint consistency.

The wording for public claims should be explicit:

> LongMemEval is a useful stress test for conversational memory retrieval. It is
> not the primary benchmark for Underpass Kernel's agentic-process value.

## Measurement Plan

The primary benchmark report should separate five measurements:

| Measurement | Meaning |
| --- | --- |
| Task success | Did the agent complete the benchmark task? |
| Evidence recall | Did the kernel retrieve the relevant prior observations? |
| Known-at-time correctness | Could the agent inspect only the memory available at step `t`? |
| Replay completeness | Can we reconstruct failed attempts, successful decisions, and final path? |
| Auditability | Can an external reader inspect why the outcome happened without raw prompt archaeology? |

LongMemEval mainly covers evidence recall and conversational answer quality.
MemoryArena and MemoryAgentBench can cover the full set.

## Next Cut

1. Keep the current LongMemEval adapter as a secondary regression.
2. Stop optimizing the core kernel around aggregate QA derivation.
3. Build a MemoryArena feasibility adapter in `rehydration-testkit`.
4. Start with one small slice:
   - `progressive_search` if we want search/process memory;
   - `group_travel_planner` if we want multidimensional people/preferences;
   - `formal_reasoning_math` only if we want derivation plugins in scope.
5. Add benchmark-native replay artifacts:
   - temporal timeline;
   - dimension coverage;
   - evidence refs;
   - failed/successful path graph;
   - known-at-time snapshots.

The target claim for the next article/demo should be:

> Underpass Kernel is an inspectable memory substrate for agentic systems. It
> does not merely retrieve chat snippets; it preserves the temporal and
> multidimensional process needed to replay how an agent reached an outcome.
