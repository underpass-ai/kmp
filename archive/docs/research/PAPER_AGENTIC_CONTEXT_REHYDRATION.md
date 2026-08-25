> **Draft / may lag implementation.** Verify any claim against [../beta-status.md](../../../docs/beta-status.md) before reuse.

# Draft Paper: Agentic Context Rehydration

Status: Working submission draft

## Working Title

Explanatory Graph Context Rehydration for Agentic Systems: Bounded Retrieval
Across Pull and Event-Driven Runtimes

## One-Sentence Thesis

`rehydration-kernel` can serve as a generic context engine for agents by
rehydrating bounded graph context from projection state, exposing it through
stable query contracts, and driving runtime actions without embedding
product-specific nouns into the kernel core.

## Why This Can Be A Paper

The repo already demonstrates a coherent systems claim:

- context is represented as a generic graph over root node, neighbor nodes,
  relationships, and extended node detail
- graph context can be rehydrated from projection storage instead of being
  assembled ad hoc inside an agent runtime
- the same context engine works for both pull-driven and event-driven agent
  flows
- the runtime boundary stays narrow and replaceable

That is enough for a credible systems or applied agentic paper if the claims
stay disciplined and the evaluation remains tied to observable behavior.

## Strong Claim We Can Defend Today

The strongest claim supported by the current repository is:

> A generic node-centric context rehydration kernel can decouple context
> management from agent runtimes while still supporting bounded retrieval and
> end-to-end runtime execution in both synchronous and asynchronous workflows.

This claim is already backed by code, contracts, and container-backed tests in
the repo.

## Strong Claim We Still Cannot Defend Yet

The infrastructure claim is now stronger than when this draft started.

The kernel already preserves typed relationship explanations end to end,
including:

- `semantic_class`
- `rationale`
- `motivation`
- `method`
- `decision_id`
- `caused_by_node_id`
- `evidence`
- `confidence`
- `sequence`

That means this claim is now defendable:

> Relationships can preserve why a downstream node exists and surface that
> explanation through event, storage, query, and render boundaries.

What is still not defendable without experiments is the stronger effectiveness
claim:

> Explanatory relationships improve agent diagnosis, rehydration-point
> selection, or task success over purely structural graph context.

That claim belongs to the evaluation section and must be supported by
ablations.

## Paper Positioning

This paper should be positioned between:

- GraphRAG style systems that retrieve structured context for LLMs
- event graph and hyper-relational knowledge graph work that preserves richer
  semantics than plain triples
- agent runtime papers that focus on tool execution but under-specify context
  retrieval

Our angle is different from standard RAG papers:

- we are not primarily ranking documents
- we are rehydrating operational graph state from projections
- we expose a reusable kernel boundary instead of coupling context assembly to
  one runtime or one product taxonomy

## Core Research Questions

1. Can a generic node-centric graph model support useful agent context without
   hardcoding product-specific workflow nouns?
2. Can context rehydration be isolated into a reusable kernel that serves
   multiple runtime entrypoints?
3. Does bounded graph rehydration preserve enough information for successful
   downstream runtime action?
4. What is lost when relationships remain non-causal and non-qualified?
5. Can the kernel provide domain-level ground truth that makes LLM reasoning
   fabrication deterministically detectable?

## Proposed Contributions

1. A reusable kernel architecture for context rehydration over projected graph
   state.
2. A generic contract surface for graph retrieval, node detail retrieval, and
   rendered bounded context.
3. A reproducible evaluation harness covering pull-driven and event-driven
   agent execution.
4. A roadmap from simple binary relations to causal or hyper-relational context
   edges for stronger agent reasoning.
5. A domain-level observability mechanism: the kernel reports `causal_density`
   as ground truth, the inference prompt forces `reason_source` declaration,
   and the evaluator cross-references both to detect fabrication
   deterministically — without a judge model and without probabilistic
   thresholds. Early evidence shows that enabling chain-of-thought converts
   fabrication from undetectable to preventable
   (see [Core Thesis](./ROADMAP_MASTER.md#core-thesis-directional-evidence-2026-03-28)).

## Experimental Artifact In This Repo

The repo already contains the main artifact needed for the paper:

- a graph-native kernel implementation
- a gRPC query boundary
- an async projection boundary
- a runnable reference agent
- two runtime implementations
- container-backed integration fixtures

Relevant assets:

- pull-driven agentic flow:
  - `scripts/ci/integration-agentic-context.sh`
  - `crates/rehydration-tests-kernel/tests/agentic_integration.rs`
- event-driven agentic flow:
  - `scripts/ci/integration-agentic-event-context.sh`
  - `crates/rehydration-tests-kernel/tests/agentic_event_integration.rs`
- minimal generic seed scenario:
  - `crates/rehydration-tests-shared/src/seed/generic_data.rs`
- explanatory relation seed scenarios:
  - `crates/rehydration-tests-shared/src/seed/explanatory_data.rs`
- zoomed bounded-context behavior:
  - `crates/rehydration-tests-kernel/tests/kernel_full_journey_integration.rs`

## Evaluation Strategy

### Scenario A: Minimal Generic Agentic Graph

Use the existing `generic_seed_data` fixture as the smallest controlled graph:

- root workspace node
- one focus work item
- one dependency node
- one detailed node

This scenario is useful for:

- verifying that the agent can derive a useful node from graph structure
- measuring end-to-end success with minimal graph noise
- reproducing pull and event-driven workflows deterministically

### Scenario B: Rich Operational Graph

Use the `starship_e2e` scenario as the richer graph:

- incident root
- multiple decisions
- multiple tasks
- subsystem nodes
- explorer workstream, checklist, and artifact leaf
- multiple node details

This scenario is useful for:

- testing bounded context under denser graphs
- testing subtree zoom behavior
- evaluating whether the rendered context keeps the useful local neighborhood

## Representative Use Cases

These are the main use cases the explanatory relation model should support.
They are not the benchmark by themselves. They are the product-level stories
from which the evaluation should be derived.

### UC1. Failure Diagnosis And Rehydration-Point Recovery

Operational question:
Given a graph that implemented a task incorrectly, can we identify which
relationships were wrong and from which upstream node the agent should
rehydrate context to try again correctly?

What the kernel must provide:

- a failing downstream node such as a rejected artifact or failed validation
- explanatory relationships that preserve the producing decision or
  causal source
- enough path context to isolate suspect edges instead of re-reading the whole
  graph

Why this matters:

- this is the concrete recovery workflow for agentic systems
- it moves the kernel from passive retrieval to restartable execution support
- it directly tests whether relationships express why the next node happened

### UC2. Why-Was-This-Implemented-Like-This Analysis

Operational question:
Given a task that was implemented in a particular way, can we reconstruct the
reason for that implementation choice from graph context alone?

What the kernel must provide:

- the incoming explanatory edge to the task
- `rationale`, `motivation`, and decision linkage
- rendered context that preserves the same explanation without domain-specific
  custom code

Why this matters:

- it tests whether the graph captures not just structure but intent
- it makes decisions and tasks auditable after the fact
- it is the smallest defendable example of explanatory context rehydration

### UC5. Fabrication Detection Via Domain Observability

Operational question:
Given a graph with no rationale metadata (structural-only), can the system
detect when the LLM fabricates a plausible-sounding justification instead of
declaring the absence?

What the kernel must provide:

- `causal_density` in `BundleQualityMetrics` reporting whether rationale exists
- rendered context that contains structural edges but no explanatory metadata
- ground truth that the evaluator can cross-reference against the LLM response

Why this matters:

- without the kernel, fabricated rationale is indistinguishable from preserved
  rationale — the model sounds equally confident in both cases
- the kernel's `causal_density` provides the ground truth that enables
  deterministic detection: `reason_source == "graph_metadata" AND
  causal_density == 0.0` → fabrication
- this tests whether the kernel adds value beyond accuracy — it makes LLM
  reasoning auditable

### Use Cases To Tests Mapping

The E2E suite should be presented as evidence for these use cases:

- UC1 maps to failure diagnosis, suspect-relationship isolation, and
  rehydration-point discovery tests
- UC2 maps to explanation reconstruction and rendered-context fidelity tests
- UC5 maps to `llm_reason_fabricated` detection on structural variants via
  `causal_density` ground truth. A/B with thinking enabled tests whether CoT
  converts fabrication to honest `not_available` declarations
- ablations then test whether explanatory relations outperform structural-only
  edges on the same use cases

## Experiments We Can Run Now

### E1. Cross-Runtime Pull Workflow

Question:
Can the same kernel context drive two different runtimes?

Method:

- run `agentic_integration.rs`
- compare `RecordingRuntime` and `UnderpassRuntimeClient`

Expected measurable outcome:

- both flows select the same expected node
- both flows write the expected artifact
- both flows include root title, focused title, and focused detail in output

### E2. Cross-Runtime Event-Driven Workflow

Question:
Can the same kernel context be consumed through an async trigger instead of a
pull-only caller?

Method:

- run `agentic_event_integration.rs`
- publish `context.bundle.generated`

Expected measurable outcome:

- the event trigger completes before timeout
- both runtimes produce the expected artifact
- the same agent logic is reused across sync and async entrypoints

### E3. Bounded Context Sensitivity

Question:
Does the kernel preserve utility under token limits?

Method:

- reuse the existing token-budget test coverage
- sweep token budgets over a small range
- compare rendered output size and task success

Existing evidence:

- the existing use-case and ablation suites already assert token-budget behavior

Expected measurable outcome:

- rendered token count decreases as budgets shrink
- task success stays stable until a threshold
- after that threshold, success drops in a measurable and explainable way

### E4. Failure Diagnosis And Rehydration Point Discovery

Question:
Given a graph that implemented a task incorrectly, can we identify which
relationships were wrong and which upstream node should be used as the
rehydration point?

Method:

- seed a failing chain such as `incident -> bad decision -> wrong task ->
  failed artifact`
- encode the failure evidence on the downstream relationship with
  `decision_id` and `caused_by_node_id`
- query `GetContextPath` to the failed artifact
- reconstruct the suspect relationships associated with the same producing
  decision

Expected measurable outcome:

- the failure evidence points back to a concrete rehydration node
- the set of suspect relationships is smaller than the full local graph
- the rendered context exposes enough explanation to restart from the correct
  upstream point

### E5. Why-Was-This-Implemented-Like-This Analysis

Question:
Can the kernel explain why a task was implemented in a specific way?

Method:

- seed a chain such as `incident -> decision -> task`
- attach `rationale`, `motivation`, and `decision_id` to the incoming task
  edge
- query `GetContextPath` to the task
- reconstruct the implementation reason from the relationship explanation and
  compare it with the rendered context

Expected measurable outcome:

- the incoming edge to the task provides a stable explanation of the chosen
  implementation
- the rendered context includes that explanation without custom domain code in
  the kernel

### E6. Local Zoom / Subgraph Rehydration

Question:
Can the kernel serve a local subgraph without dragging the whole root context?

Method:

- reuse the explorer workstream subtree checks in the full-journey tests

Existing evidence:

- the zoomed rendered context contains local workstream, checklist, and leaf
  detail while excluding the original root title and root detail

Expected measurable outcome:

- selected nodes and relationships shrink
- rendered context excludes unrelated root-level content
- local task evidence remains intact

### E7. Detail Ablation

Question:
How much does extended node detail contribute beyond graph neighborhood alone?

Method:

- rerun Scenario A and Scenario B while withholding detail materialization
- compare output correctness and action quality

Expected measurable outcome:

- tasks that need precise operational detail degrade first
- graph structure alone remains sufficient for coarse focus selection

### E8. Relationship Explanation Ablation

Question:
How much explanatory value comes from rich relation metadata beyond structural
edges alone?

Method:

- rerun the same scenario twice:
  - explanatory edges with rationale, motivation, method, and decision linkage
  - structural-only edges with ordering but without rich explanation fields
- compare rendered specificity, rehydration-point discovery, and diagnostic
  usefulness
- keep nodes and topology fixed so the difference comes from explanation
  richness only

Expected measurable outcome:

- rendered context becomes shorter and less specific without explanations
- diagnostic traces become harder to reconstruct
- rehydration-point discovery should degrade before raw topology fails

### E9. Fabrication Detection Via Domain Observability (UC5)

Question:
Can the kernel's domain-level ground truth (`causal_density`) make LLM
reasoning fabrication deterministically detectable — without a judge model?

Method:

- same graph rendered in two variants: explanatory (`causal_density > 0`)
  and structural (`causal_density = 0.0`)
- inference prompt requires the model to declare `reason_source`
  (`graph_metadata` / `inferred` / `not_available`) and `confidence`
  (`high` / `medium` / `low`) in the JSON response
- evaluator cross-references: `reason_source == "graph_metadata" AND
  causal_density == 0.0` → `llm_reason_fabricated = true`
- A/B: baseline (no thinking) vs thinking (Qwen3 CoT enabled)
- three runs: 324 total evals (108 per arm)

Results (structural variants only, 36 evals per arm):

| Arm | Fabricated | Source=graph | Source=n/a | Confidence=high | Confidence=low |
|-----|:----------:|:------------:|:----------:|:---------------:|:--------------:|
| A — no thinking | **32/36 (89%)** | 32/36 | 0/36 | 32/36 | 0/36 |
| B — thinking | **0/36 (0%)** | 0/36 | **36/36** | 2/36 | **34/36** |
| P — planner (512) | **0/36 (0%)** | 0/36 | **36/36** | 1/36 | **35/36** |

Control check (explanatory variants, 36 evals per arm):

| Arm | Fabricated | Source=graph | Confidence=high |
|-----|:----------:|:------------:|:---------------:|
| A | 0/36 | 36/36 | 36/36 |
| B | 0/36 | 36/36 | 36/36 |
| P | 0/36 | 36/36 | 36/36 |

Interpretation:

- without thinking, the model claims `graph_metadata` with high confidence
  on 89% of structural variants — fabrication is invisible to the consumer
- with thinking, the model declares `not_available` with low confidence
  on 100% of structural variants — fabrication is eliminated
- the kernel's `causal_density` provides the ground truth that makes the
  detection deterministic: no judge needed, no probabilistic threshold
- the control check confirms that on explanatory variants (where rationale
  exists), the model correctly declares `graph_metadata` with high
  confidence — thinking does not suppress legitimate source declaration
- fabrication detection is stable under token pressure (budget=512):
  planner run shows identical honesty to budget=4096

This is the first evidence that a context engine can provide domain-level
observability that converts LLM fabrication from undetectable to
deterministically preventable.

## Preliminary Results

The current paper harness produces direct evidence for five use cases
across multiple ablation variants:

- full explanatory relations with detail
- full explanatory relations without detail
- detail-only relations with detail
- structural-only relations with detail
- meso-scale (denser noisy graph) variant for UC1
- token-budget constrained variants (192 and 96 tokens) for UC4
- fabrication detection via `causal_density` ground truth for UC5

Current results from `archive/artifacts/paper-use-cases/summary.json`,
`archive/artifacts/paper-use-cases/results.md`, and
`archive/artifacts/e2e-runs/2026-03-29_{100518,131923,153051}/` show a clean pattern.

### Result R1. Explanatory Relations Matter More Than Detail Removal

Across all four use cases (UC1-UC4):

- full explanatory context reaches `explanation_roundtrip_fidelity = 1.0`
- full explanatory context reaches `causal_reconstruction_score = 1.0`
- structural-only context drops to `causal_reconstruction_score = 0.143`
- detail-only context reaches `causal_reconstruction_score = 0.429`

Interpretation:

- removing explanation destroys most of the causal or motivational signal even
  when node detail is still present
- detail alone is not enough to reconstruct why the next node exists
- detail-only recovers more than structural-only but far less than explanatory

### Result R2. Detail Still Matters, But Less Than Explanatory Edges

Across UC1-UC3:

- explanatory-without-detail keeps `explanation_roundtrip_fidelity = 1.0`
- explanatory-without-detail drops to `causal_reconstruction_score = 0.857`

Interpretation:

- the relation explanation preserves the why-trace even without extended node
  detail
- detail still improves completeness, especially for failure diagnosis and
  recovery prompts

### Result R3. Token Reduction Tracks Loss Of Explanatory Power

Observed rendered token counts:

- UC1 full: `282`, structural-only: `200`, without-detail: `252`
- UC2 full: `285`, structural-only: `203`, without-detail: `255`
- UC3 full: `575`, structural-only: `374`, without-detail: `535`
- UC4 full: `223`, full@192: `175`, full@96: `89`, structural@96: `73`

Interpretation:

- structural-only edges make the prompt shorter, but that shorter prompt is
  materially worse for diagnosis and implementation-trace recovery
- removing detail gives a smaller token reduction and a smaller quality drop
- UC4 shows that even under token pressure (192 tokens), explanatory
  context preserves `1.0` causal reconstruction while structural at 96 tokens
  drops to `0.125`

### Result R4. Rehydration-Point Discovery Depends On Explanatory Linkage

For UC1 and UC3:

- full explanatory context finds the rehydration or continuation point correctly
- structural-only context loses recovery entirely
- detail-only context also loses recovery entirely
- explanatory-without-detail still recovers the correct upstream node

Interpretation:

- `decision_id` and `caused_by_node_id` are carrying the recovery signal
- this is direct evidence for the kernel value proposition in restartable
  agent workflows

### Result R5. Closed-Loop Retry Success Depends On Explanation

For UC1 and UC3:

- explanatory variants reach `retry_success_rate = 1.0` including no-detail
- structural-only and detail-only variants stay at `0.0`

Interpretation:

- detail-only context can preserve fragments of why-trace text but does not
  expose a stable machine-readable anchor for restart or resume
- the closed-loop signal is sharper than continuation-point hit alone

### Result R6. Meso-Scale Robustness

For UC1 under a denser noisy graph:

- `causal_reconstruction_score = 1.0` (same as micro)
- `retry_success_rate = 1.0` (same as micro)
- rendered token count: `282` (same as micro)

Interpretation:

- the explanatory signal survives distractor branches in the meso graph

### Result R7. Fabrication Is Deterministically Detectable (UC5)

Across 324 evals (3 runs × 108 evals), the kernel's `causal_density`
ground truth enables deterministic fabrication detection:

- **Without thinking**: 89% fabrication rate on structural variants —
  the model claims `graph_metadata` with `confidence: high` when no
  rationale exists. Invisible to consumers without the kernel.
- **With thinking**: 0% fabrication — the model declares `not_available`
  with `confidence: low`. 100% honest across all 72 structural evals
  (thinking + planner runs).
- **Control**: on explanatory variants where rationale exists, the model
  correctly declares `graph_metadata` with `confidence: high` regardless
  of thinking mode. No false negatives.
- **Under token pressure**: fabrication detection is identical at
  budget=512 and budget=4096 — the planner does not compromise honesty.

This is the strongest result in the artifact: the kernel provides
domain-level observability that makes LLM reasoning fabrication
deterministically detectable — without a judge model and without
probabilistic thresholds. Chain-of-thought converts fabrication from
undetectable to preventable.

### Key Finding: Kernel and Thinking Are Independent Contributions

A controlled A/B comparison (same vLLM v0.18.0, same temperature 0.6,
same max_tokens, only thinking ON/OFF differs) separates the two effects:

| Signal | Metric | Without | With | Delta | Source |
|--------|--------|:-------:|:----:|:-----:|--------|
| **Kernel** (explanatory vs structural) | Task | 30% | 66% | **+36pp** | Context quality |
| **Thinking** (CoT ON vs OFF) | Task | 66% | 66% | **0pp** | Reasoning mode |
| **Thinking** | Restart | 30% | 52% | **+22pp** | Recovery reasoning |
| **Thinking** | Fabrication | 97% | 0% | **-97pp** | Honesty |

The kernel improves task accuracy by providing causal chains and rationale
metadata — information the model cannot infer from structural edges alone.
This is a **context quality** effect, not a reasoning mode effect.

Thinking improves recovery accuracy and eliminates fabrication — it helps
the model reason about multi-hop recovery paths and declare when rationale
is absent. This is a **reasoning mode** effect that compounds with the
kernel's context but does not substitute for it.

A small model (8B parameters) with kernel context and no thinking achieves
66% task accuracy. The same model without kernel context achieves 30%
regardless of thinking mode. **The kernel compensates for model capacity.**

### Statistical Analysis

**Null hypothesis (H0):** There is no difference in task accuracy between
explanatory and structural context variants.

**Alternative (H1):** Explanatory context produces higher task accuracy.

**Test:** Bootstrap 95% CI on the gap (10,000 resamples, seed=42).
Wilson score intervals for individual proportions.

| Judge | Explanatory Task | Structural Task | Gap | Gap 95% CI | H0 |
|-------|:----------------:|:---------------:|:---:|:----------:|:--:|
| Sonnet 4.6 | 67% [50%, 80%] | 0% [0%, 10%] | +67pp | [+50pp, +81pp] | **Rejected** |
| GPT-5.4 | 72% [56%, 84%] | 3% [0%, 14%] | +69pp | [+53pp, +83pp] | **Rejected** |

H0 is rejected at 95% confidence for both judges. The lower bound of the
gap CI (+50pp Sonnet, +53pp GPT) is well above zero.

**Per-metric 95% CI (GPT-5.4 judge, Wilson score):**

| Mix | Task | Restart | Reason |
|-----|:----:|:-------:|:------:|
| Explanatory | 72% [56%, 84%] | 75% [59%, 86%] | 72% [56%, 84%] |
| Structural | 3% [0%, 14%] | 0% [0%, 10%] | 0% [0%, 10%] |
| Mixed | 92% [78%, 97%] | 81% [65%, 90%] | 89% [75%, 96%] |

**Fabrication 95% CI (Wilson):** 0% [0%, 10%] — both judges, both thinking runs.
Upper bound 10% is the statistical ceiling given N=36 structural evals.

**Inter-seed variance (GPT-5.4, explanatory, 12 evals per seed):**

| Seed | Task | 95% CI |
|:----:|:----:|:------:|
| 0 | 75% | [47%, 91%] |
| 1 | 83% | [55%, 95%] |
| 2 | 58% | [32%, 81%] |

Seed CIs overlap — no seed produces a significantly different result.
The variance (~25pp range) reflects LLM sampling noise (temp=0.6) and
graph topology differences (kind rotation per seed).

**Threats to validity:**
- N=36 per mix limits statistical power; CIs are wide (±14-16pp)
- LLM temperature 0.6 introduces sampling variance
- Single model (Qwen3-8B); generalization to other models untested
- Synthetic graphs; real operational graphs may behave differently
- Judge agreement does not guarantee ground truth correctness

## Primary Metrics

The paper should use a small set of defensible metrics:

- task success rate
- expected focus-node hit rate
- explanation roundtrip fidelity
- diagnostic rehydration-point hit rate
- causal reconstruction score
- rendered token count
- selected node count
- selected relationship count
- runtime portability score:
  - same task succeeds across multiple runtimes
- async trigger success rate
- fabrication rate: `llm_reason_fabricated` (deterministic, no judge)
- source declaration distribution: `llm_reason_source` (graph_metadata / inferred / not_available)
- confidence calibration: `llm_confidence` vs ground truth
- end-to-end latency:
  - optional if we add timing instrumentation

## Baselines

We need simple baselines, not a benchmark zoo.

Recommended baselines:

1. Root-only baseline
   - root summary and root detail only
2. Flat detail baseline
   - all available details without graph relationships
3. Graph-without-detail baseline
   - nodes and relations only
4. Full kernel baseline
   - current node-centric rehydration

5. Structural-edge baseline
   - typed edges with ordering only
6. Explanatory-edge baseline
   - full kernel with typed relation explanations

## Abstract

Agent runtimes often depend on ad hoc prompt assembly or product-specific
context APIs, making context management difficult to reuse across systems. We
present `rehydration-kernel`, a graph-native context engine that reconstructs
bounded operational context from projection state and serves it through
generic query and async contracts. The kernel models context as nodes with
optional detail plus typed relationship explanations that preserve why
downstream nodes exist, including rationale, motivation, method, and
decision linkage. We evaluate the system across five use cases — failure
diagnosis, implementation justification, interrupted handoff, constraint
retrieval under token pressure, and fabrication detection — using LLM-as-judge
evaluation with 432 evaluations across two independent judges (GPT-5.4 and
Claude Sonnet 4.6), three graph scales, two noise modes per mix, and three
random seeds. The kernel's explanatory context enables 72% task accuracy and
75% recovery accuracy on a local 8B-parameter model, versus 3% task and 0%
recovery with structural-only context — a gap of +69pp (GPT-5.4) and +67pp
(Sonnet 4.6), both significant at 95% confidence. Mixed structural+explanatory context achieves 91% task accuracy,
demonstrating that the two signal types compound. The kernel also provides
domain-level observability: its `causal_density` metric serves as ground truth
that makes LLM fabrication deterministically detectable without a judge model.
Without chain-of-thought, 97% of structural responses are fabricated; with
thinking enabled, 0% — converting fabrication from undetectable to preventable.
Under 8x token budget reduction (4096→512), the tier-aware planner preserves
task accuracy (-3pp) and improves recovery (+17pp) by sacrificing low-value
evidence while preserving the causal spine. These results indicate that
explanatory relationships carry the dominant signal for restartable and
auditable agent workflows, and that the kernel adds value both as a context
engine (accuracy) and as an observability layer (auditability).

## Results Summary

### Phase 1: Rule-based evaluation (UC1-UC4)

Across all four explanatory use cases, `full_explanatory_with_detail` reaches
perfect `explanation_roundtrip_fidelity = 1.0` and
`causal_reconstruction_score = 1.0`. When detail is removed but explanatory
relations remain, causal reconstruction drops only to `0.857`. When
explanatory relations are replaced by structural edges while detail is kept,
causal reconstruction collapses to `0.143`. The net effect is an absolute
drop of `0.857` from losing explanation, versus `0.143` from losing detail.

Artifacts: `archive/artifacts/paper-use-cases/`

### Phase 2: LLM-as-judge evaluation (108-432 evals, 2026-03-29)

LLM inference (Qwen3-8B with thinking) evaluated by independent judges
(Sonnet 4.6 and GPT-5.4). The kernel serves the context; the LLM must
identify failure points, recovery nodes, and rationale from that context.

**Cross-judge consistency (108 evals each, same agent, same data):**

| Judge | Explanatory Task | Structural Task | Gap |
|-------|:----------------:|:---------------:|:---:|
| Sonnet 4.6 | 66% | 0% | +66pp |
| GPT-5.4 | 72% | 3% | +69pp |

Both judges converge: the kernel advantage is robust to evaluator choice.

**GPT-5.4 judge results (108 evals, primary paper reference):**

| Mix | Task | Restart | Reason |
|-----|:----:|:-------:|:------:|
| Explanatory | **26/36 (72%)** | **27/36 (75%)** | **26/36 (72%)** |
| Structural | 1/36 (3%) | 0/36 (0%) | 0/36 (0%) |
| Mixed | **33/36 (91%)** | **29/36 (80%)** | **32/36 (88%)** |

Key findings:

1. **The kernel adds +69pp accuracy.** Without explanatory metadata, the
   model scores 3% Task. With it, 72%. The kernel provides the information
   the LLM needs to reason about the causal chain.

2. **Mixed > Explanatory.** Structural topology (91% Task) combined with
   explanatory rationale outperforms explanatory alone (72%). The kernel
   serves both signals, and they compound.

3. **Recovery improves +75pp.** Restart accuracy jumps from 0% (structural)
   to 75% (explanatory). The model can identify the correct recovery node
   only when the kernel provides `caused_by_node_id` and `decision_id`.

4. **Cross-scale robustness.** The gap holds across micro (66% Task),
   meso (58%), and stress (41%). The kernel value persists as graphs grow.

**Per-seed variance (explanatory, 12 evals each):**

| Seed | Task | Reason |
|:----:|:----:|:------:|
| 0 | 75% | 75% |
| 1 | 83% | 83% |
| 2 | 58% | 58% |

Variance of ~25pp across seeds reflects LLM sampling noise (temp=0.6) and
graph topology differences (kind rotation). The direction is consistent:
all seeds show explanatory >> structural.

**Fabrication detection (UC5, 36 structural evals per arm, 324 total):**

| Arm | Fabricated | Honest |
|-----|:----------:|:------:|
| A — no thinking | 32/36 (89%) | 0/36 |
| B — thinking | 0/36 (0%) | 36/36 (100%) |
| P — planner (512) | 0/36 (0%) | 36/36 (100%) |
| Paper (GPT-5.4) | 0/36 (0%) | 36/36 (100%) |

Deterministic detection: `reason_source == "graph_metadata" AND
causal_density == 0.0` → fabricated. No judge needed. Thinking converts
fabrication from undetectable to preventable.

**Planner under pressure (budget=512, 108 evals):**

| Mix | Task (4096) | Task (512) | Delta |
|-----|:-----------:|:----------:|:-----:|
| Explanatory | 66% | 63% | -3pp |
| Restart | 52% | 69% | **+17pp** |

Tier-aware truncation (L0 guaranteed, L1 prioritized, L2 sacrificed)
preserves the causal spine under 8x budget reduction. Restart improves
because the model receives a cleaner signal with structural noise removed.

Artifacts: `archive/artifacts/e2e-runs/2026-03-29_{100518,131923,153051,170351}/`
Reproducible config: `paper-recalc-gpt54.yaml`

## Conclusion

The repository supports three defensible claims:

1. **Accuracy**: explanatory context from the kernel enables LLMs to perform
   bounded graph tasks (72% Task, 75% Restart) that structural-only context
   cannot support (3% Task, 0% Restart). The +69pp gap is consistent across
   two independent judges, three graph scales, and three random seeds.

2. **Auditability**: the kernel provides domain-level ground truth
   (`causal_density`) that makes LLM fabrication deterministically detectable.
   Without thinking, 89% of structural responses are fabricated. With thinking,
   0%. The detection requires no judge model and no probabilistic threshold.

3. **Bounded retrieval**: the planner preserves accuracy under 8x token
   budget reduction (-3pp Task) and improves recovery (+17pp Restart) by
   sacrificing low-value evidence while preserving the causal spine.

This gives the paper a sharper contribution than generic GraphRAG positioning.
The kernel is not only retrieving nearby nodes; it is preserving an
application-supplied explanation of transition between nodes and making that
explanation available through event, storage, query, and rendered-context
boundaries. The next step for a stronger submission is not to redesign the
model, but to widen the evaluation: larger graphs, stronger baselines,
unified cross-runtime metrics, and experiments that measure whether the
selected rehydration point leads to a corrected retry at scale.

## Draft Paper Outline

### 1. Introduction

- context management is a hidden systems problem in agent stacks
- prompt assembly is often runtime-specific and non-reusable
- graph-native rehydration offers a reusable alternative

### 2. Problem Statement

- agents need bounded, actionable context
- raw graph storage is not sufficient
- context retrieval should stay decoupled from runtime execution

### 3. System Design

- projection inputs over NATS
- graph state in Neo4j
- node detail in Valkey
- query serving over gRPC
- runtime boundary outside the kernel

### 4. Node-Centric Context Model

- root node
- neighbor nodes
- relationships
- extended node detail
- rendered bounded context

### 5. Experimental Setup

- Scenario A: minimal generic graph
- Scenario B: rich operational graph
- runtimes:
  - in-memory recording runtime
  - HTTP runtime adapter
- workflows:
  - pull-driven
  - event-driven

### 6. Results

- cross-runtime success
- async trigger success
- failure-diagnosis success
- why-implementation reconstruction success
- token-budget sensitivity
- subtree zoom behavior
- ablation outcomes

### 7. Limitations

- relation semantics are application-provided and not domain-validated beyond
  the typed explanation shape
- current generic gRPC path does not fully exploit focused-node rendering
- no auth or tenant isolation claims
- no long-lived session claims

### 8. Explanatory Relations

- typed relationship qualifiers
- decision-producing edges
- evidence-bearing edges
- narrative or causal ordering
- evaluation for explanation fidelity and diagnosis quality

### 9. Conclusion

- context rehydration can be a reusable kernel concern
- graph-native bounded retrieval is practical for agent systems
- diagnostic and recovery evaluation is the next step

## Explanatory Relations For The Stronger Paper

The stronger version of the paper is no longer about adding the explanatory
model. That part now exists in the kernel.

The stronger version is about proving what the explanatory model enables:

- diagnosis of wrong task implementations
- discovery of rehydration points
- reconstruction of why a task was implemented in a particular way
- measurable gains over structural-only relations

### Why This Matters

Today the kernel can answer:

- what nodes are near the root
- what detail belongs to each node
- what relationships exist

The stronger context model should answer:

- what produced this downstream node
- what decision justified it
- what evidence or rationale supports the transition
- what order the agent should reason through the neighborhood

That is the real bridge from graph retrieval to agent context management.

## Risks To Validity

- current agent tasks are still small and synthetic
- current success criteria are artifact-oriented, not benchmark-grade task
  scores
- runtime diversity is still limited to two implementations
- focus behavior still needs broader measurement across more agent entrypoints

These should be stated explicitly in the paper.

## Immediate Repo Work Before Submission

### Required For A Credible v1 Submission

- add cross-runtime metrics to the same paper artifact used for UC1-UC4
- capture end-to-end latency in the paper summary
- freeze the current artifacts into a release-ready paper appendix

Note: a denser meso graph (UC1) and detail-only baseline are now included in
the artifact.

### Required For The Stronger Explanatory Submission

- add root-only and flat-detail baselines alongside structural-only edges
- widen UC1-UC4 beyond synthetic local graphs
- add one longer causal chain with multiple competing decisions
- expand meso-scale variants to UC2-UC4

## Reproducibility Plan

Paper artifact commands should start from the existing scripts:

```bash
bash scripts/ci/integration-agentic-context.sh
bash scripts/ci/integration-agentic-event-context.sh
bash scripts/ci/integration-paper-use-cases.sh
```

Then add a paper harness that:

- runs both scenarios
- runs all ablations
- records success or failure
- records token counts
- records selected graph sizes
- emits a CSV or JSON summary

The current repo now has the first paper harness step for the explanatory
relation use cases:

- `scripts/ci/integration-paper-use-cases.sh`
- writes per-use-case metrics into `archive/artifacts/paper-use-cases/cases/`
- writes an aggregated JSON summary into
  `archive/artifacts/paper-use-cases/summary.json`
- writes a Markdown report into `archive/artifacts/paper-use-cases/results.md`
- writes a CSV export into `archive/artifacts/paper-use-cases/results.csv`
- writes Mermaid figures into `archive/artifacts/paper-use-cases/results-figures.md`

## Recommendation

Write the paper in two layers:

- Paper v1:
  - generic node-centric context rehydration for agents
  - fully supported by the current repo with modest experiment harness work
- Paper v2:
  - explanatory graph relations improve diagnosis and task recovery
  - requires ablation results before the claim is strong enough

That sequencing keeps the paper technically honest while still aligning with
the direction you want: relationships that explain the decision that produced
the next node.

For the paper narrative, lead with the four use cases:

- diagnose a wrong implementation and recover from the correct rehydration
  point
- explain why a task was implemented in a particular way
- recover from an interrupted handoff and resume execution
- preserve the dominant constraint reason under token pressure

Those four stories are concrete, easy to evaluate, and tightly aligned with the
explanatory relation model.
