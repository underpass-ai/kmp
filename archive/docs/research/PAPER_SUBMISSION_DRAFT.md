> **Draft / may lag implementation.** Verify any claim against [../beta-status.md](../../../docs/beta-status.md) before reuse.

# Submission Draft: Explanatory Graph Context Rehydration

Status: Submission-oriented draft based on the current repository artifact

## Title

Explanatory Graph Context Rehydration for Agentic Systems: Bounded Retrieval
Across Pull and Event-Driven Runtimes

## Abstract

Agent systems often assemble context through runtime-specific prompts,
application endpoints, and ad hoc caches, making context management difficult
to reuse, audit, or evaluate independently of execution. We present
`rehydration-kernel`, a graph-native context service that reconstructs bounded
operational context from projected state and exposes it through query and
asynchronous interfaces. The kernel models context as nodes with optional
detail plus typed relationship explanations that preserve why a downstream
node exists, including rationale, motivation, method, and decision linkage.
We evaluate the system with container-backed end-to-end workflows and four
explanatory use cases: failure diagnosis with rehydration-point recovery,
reconstruction of why a task was implemented in a particular way, interrupted
handoff with resumable execution, and constraint-preserving retrieval under
token pressure. Across these use cases, full explanatory context achieves
`1.0` explanation roundtrip fidelity and `1.0` causal reconstruction in the
full-detail setting, while retaining `1.0` causal reconstruction under a
`192`-token budget in the constraint-preserving case. Removing node detail
preserves explanation fidelity but reduces causal reconstruction to `0.857`,
while replacing explanatory relations with structural edges reduces causal
reconstruction to `0.143`, eliminates rehydration-point recovery, and fails
to preserve the dominant reason under budget pressure. These results suggest
that explanatory relationships carry the dominant signal for restartable and
auditable agent workflows, while node detail primarily improves completeness.

## 1. Introduction

Context management remains an under-specified systems problem in agent
architectures. In many practical stacks, the runtime that plans or executes
actions also assembles its own context from prompts, application APIs, caches,
and task-local heuristics. This coupling makes context assembly difficult to
reuse across runtimes, difficult to audit after the fact, and difficult to
evaluate as a system component in its own right.

This paper explores a different separation of concerns. With
`rehydration-kernel`, context rehydration is isolated into a reusable kernel
that reconstructs bounded graph context from projection state. Runtimes
consume already rehydrated context through stable query or event boundaries
instead of rebuilding that context inside each execution loop.

Our central claim is not merely that graph structure can hold context. It is
that explanatory relationships materially improve the usefulness of
rehydrated context for agent workflows. In our model, a relationship does not
only encode connectivity between two nodes. It may also preserve why the
downstream node exists, what decision produced it, what motivation drove it,
or what method verified it.

This distinction matters for operational questions that arise in realistic
agent systems:

- why was a task implemented in a particular way
- which relationships led to an incorrect implementation
- from which upstream node should the system rehydrate and retry

The paper makes three contributions:

- a reusable kernel architecture for bounded graph context rehydration over projected state
- an explanatory relationship model that preserves transition-level intent without moving domain ownership into the kernel
- a reproducible artifact showing that explanatory relations contribute more to causal reconstruction and recovery than node detail alone

## 2. Problem Statement

An agent needs bounded, actionable context, not raw graph storage. A raw
graph alone does not define:

- what part of the graph should be selected
- what explanatory fields should survive retrieval
- how to preserve detail under token pressure
- how to present restartable reasoning context to the runtime

We therefore define the problem as follows:

Given projected graph state and optional node detail, construct a bounded
context bundle that preserves enough local structure and explanation for an
agent runtime to act, diagnose, justify, or recover.

The design constraints in this repository are deliberate:

- the kernel must remain generic and avoid product-specific nouns
- the kernel must support both pull and event-driven entrypoints
- the kernel must not invent semantic motivation on its own
- explanation semantics must be supplied by the agent or domain application

## 3. Context Model

The context model has four primitives:

1. A root node that anchors retrieval.
2. Neighbor nodes that define the selected local graph.
3. Relationships between nodes.
4. Optional extended node detail stored separately from the graph.

The model becomes stronger when relationships carry typed explanation. In the
current implementation, a relationship explanation can preserve fields such
as:

- `semantic_class`
- `rationale`
- `motivation`
- `method`
- `decision_id`
- `caused_by_node_id`
- `evidence`
- `confidence`
- `sequence`

This is an application-supplied explanation of transition between nodes. The
kernel preserves it, stores it, rehydrates it, and renders it. The kernel
does not infer it.

That distinction is important for DDD boundaries. The domain that knows why a
task exists or why a decision was taken is not the kernel. The kernel is the
infrastructure that keeps that explanation intact across projection, storage,
query, and rendering boundaries.

## 4. System Architecture

The repository implements the model as a graph-native context service:

- async projection inputs over NATS
- graph state persisted in Neo4j
- extended node detail persisted in Valkey
- query serving over gRPC
- runtime execution kept outside the kernel boundary

This architecture enables the same rehydrated context to be consumed by:

- pull-driven runtimes that query context before execution
- event-driven runtimes triggered from generated context bundles

The paper artifact includes container-backed integration suites for both
styles. The current quantitative paper harness focuses on explanatory use
cases, while the broader integration suites demonstrate cross-runtime and
event-driven operability.

## 5. Related Work Positioning

Our work is closest to four lines of prior work, but differs from each in
system boundary and objective.

First, GraphRAG-style systems use graph structure to organize retrieval for
language models. Edge et al. cast GraphRAG as graph-based query-focused
summarization over large corpora [1]. That work is close in its use of
graph-shaped context, but it is document-centric. Our kernel instead
rehydrates projected operational state and serves it as runtime context rather
than constructing a graph from retrieved text.

Second, event-graph and narrative-graph approaches preserve richer temporal
and causal structure than plain knowledge graph triples. Our explanatory
relationship model follows that direction by preserving why a downstream node
exists rather than only which node comes next. ExCAR is especially relevant
because it demonstrates the value of event-graph evidence for explainable
causal reasoning [2].

Third, hyper-relational knowledge graph research shows that relation
qualifiers carry contextual signal that plain triples lose. HAHE and HOLMES
reinforce this point for representation learning and multi-hop reasoning over
qualified relations [3, 4]. Text2NKG extends the same direction by supporting
n-ary and event-based schemas that more closely match how operational
workflows are described in practice [5]. Our contribution is to apply this
principle in a systems setting where qualified relationships must survive
projection, storage, query, and rendered context boundaries.

Finally, there is a semantic-modeling precedent for separating contextual and
conceptual information. GKR uses a layered graph representation with a
dedicated contextual layer rather than collapsing all semantics into a single
flat structure [6]. Our model is not a semantic parser, but the split is
aligned: node identity and node detail remain distinct from
relationship-level explanation about why a transition occurred.

## 6. Experimental Setup

### 6.1 Use Cases

We evaluate four explanatory use cases.

`UC1. Failure diagnosis and rehydration-point recovery`

Given a graph that implemented a task incorrectly, can the system isolate the
suspect relationships and identify the upstream node from which context should
be rehydrated for a corrected retry?

`UC2. Why-was-this-implemented-like-this analysis`

Given a task that was implemented in a specific way, can the system
reconstruct the reason for that implementation choice from graph context
alone?

`UC3. Interrupted handoff and resumable execution`

Given a task interrupted after a blocked execution step, can the system
recover why the task was handed off and from which upstream node execution
should resume?

`UC4. Constraint-preserving retrieval under token pressure`

Given a task chosen under a binding safety constraint, can the system preserve
the dominant reason when the rendered context is restricted to a small token
budget?

### 6.2 Variants

UC1-UC3 are executed under three variants:

- `full_explanatory_with_detail`
- `full_explanatory_without_detail`
- `structural_only_with_detail`

UC4 adds a budgeted comparison:

- `full_explanatory_with_detail`
- `full_explanatory_with_detail__budget_192`
- `full_explanatory_with_detail__budget_96`
- `structural_only_with_detail__budget_96`

This isolates the contribution of explanatory relationships from the
contribution of extended node detail and tests whether those relationships
survive bounded retrieval.

### 6.3 Metrics

The current harness records:

- `explanation_roundtrip_fidelity`
- `detail_roundtrip_fidelity`
- `causal_reconstruction_score`
- `rendered_token_count`
- `rehydration_point_hit`
- `retry_success_hit`
- `retry_success_rate`
- `dominant_reason_hit`
- `suspect_relationship_count`

### 6.4 Reproducible Artifact

The paper artifact is generated by:

```bash
bash scripts/ci/integration-paper-use-cases.sh
```

It writes:

- `archive/artifacts/paper-use-cases/summary.json`
- `archive/artifacts/paper-use-cases/results.md`
- `archive/artifacts/paper-use-cases/results.csv`
- `archive/artifacts/paper-use-cases/results-figures.md`

## 7. Results

Table 1 summarizes the current explanatory-use-case results. The primary table
keeps only the comparison needed for the central claim. The `detail-only` and
`meso` ablations are moved to a secondary table below.

| Use case | Variant | Budget | Explanation fidelity | Detail fidelity | Causal score | Retry | Aux. hit | Tokens |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| UC1 | full+detail | 4096 | 1.000 | 1.000 | 1.000 | 1 | 1 | 282 |
| UC1 | full-no-detail | 4096 | 1.000 | 0.000 | 0.857 | 1 | 1 | 252 |
| UC1 | detail-only | 4096 | 0.000 | 1.000 | 0.429 | 0 | 0 | 245 |
| UC1 | structural | 4096 | 0.000 | 1.000 | 0.143 | 0 | 0 | 200 |
| UC2 | full+detail | 4096 | 1.000 | 1.000 | 1.000 | n/a | n/a | 285 |
| UC2 | full-no-detail | 4096 | 1.000 | 0.000 | 0.857 | n/a | n/a | 255 |
| UC2 | detail-only | 4096 | 0.000 | 1.000 | 0.429 | n/a | n/a | 247 |
| UC2 | structural | 4096 | 0.000 | 1.000 | 0.143 | n/a | n/a | 203 |
| UC3 | full+detail | 4096 | 1.000 | 1.000 | 1.000 | 1 | 1 | 575 |
| UC3 | full-no-detail | 4096 | 1.000 | 0.000 | 0.857 | 1 | 1 | 535 |
| UC3 | detail-only | 4096 | 0.000 | 1.000 | 0.429 | 0 | 0 | 441 |
| UC3 | structural | 4096 | 0.000 | 1.000 | 0.143 | 0 | 0 | 374 |
| UC4 | full+detail | 4096 | 1.000 | 1.000 | 1.000 | n/a | 1 | 223 |
| UC4 | full@192 | 192 | 1.000 | 0.000 | 1.000 | n/a | 1 | 175 |
| UC4 | full@96 | 96 | 1.000 | 0.000 | 1.000 | n/a | 1 | 89 |
| UC4 | structural@96 | 96 | 0.000 | 0.000 | 0.125 | n/a | 0 | 73 |

`Retry` reports corrected retry or resume success for UC1 and UC3. `Aux. hit`
reports rehydration or continuation-point hit for UC1 and UC3, and
dominant-reason hit for UC4.

Figure 2 in the paper version compresses the extended ablations that are useful
but secondary to the main story. The same values are listed here for reference.

| Use case | Variant | Scale | Causal score | Retry | Aux. hit | Tokens |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| UC1 | detail-only | micro | 0.429 | 0 | 0 | 245 |
| UC1 | full+detail | meso | 1.000 | 1 | 1 | 282 |
| UC2 | detail-only | micro | 0.429 | n/a | n/a | 247 |
| UC3 | detail-only | micro | 0.429 | 0 | 0 | 441 |

### 7.1 Explanatory Relations Carry The Dominant Signal

The cleanest result is the size and consistency of the degradation. In UC1,
UC2, and UC3, removing explanation while keeping detail reduces
`causal_reconstruction_score` from `1.0` to `0.143`. Removing detail while
preserving explanation reduces it only to `0.857`.

This means the dominant loss is not absence of detail. It is absence of
relationship explanation. The agent can still reconstruct most of the why-trace
without detail, but it cannot reconstruct it from structure alone.

### 7.2 Detail Improves Completeness, Not Primary Intent

Detail still matters. The repeated drop from `1.0` to `0.857` in UC1-UC3 shows
that explanatory relations do not fully replace node detail. However, detail
behaves as a completeness amplifier rather than the primary carrier of
intent.

This is a useful systems result. It suggests that token budgets should prefer
preserving explanatory edges before preserving every extended detail block.
UC4 makes that concrete: at a `192`-token budget, the explanatory variant loses
detail fidelity entirely while preserving both causal reconstruction and the
dominant reason.

### 7.3 Operational Continuation Depends On Explanation Fields

UC1 and UC3 show the operational value of the model. In both cases the
explanatory variants recover the correct upstream continuation point. The
structural-only variants do not.

That is strong evidence that fields such as `decision_id` and
`caused_by_node_id` are not decorative qualifiers. They carry the recovery and
handoff signal that allows the system to isolate where to restart.

The closed-loop signal is sharper than continuation hit alone. In both UC1 and
UC3, the explanatory variants reach `retry_success_rate = 1.0`, including the
no-detail setting. Structural-only and detail-only variants stay at `0.0`.
This matters because detail-only context can still preserve fragments of the
why-trace, but it does not expose a stable machine-readable anchor from which a
runtime can retry or resume execution.

### 7.4 Bounded Retrieval Preserves Reasons When Explanation Survives

UC4 provides the strongest bounded-retrieval result. The full explanatory
context rendered at `192` tokens still reaches `1.0`
`causal_reconstruction_score` and preserves the dominant reason, even though
detail fidelity falls to `0.0`. The structural-only variant at `96` tokens
drops to `0.125`.

This is exactly the systems behavior we want: when budget forces truncation,
explanation should survive before detail.

### 7.5 Shorter Context Can Be Worse Context

Structural-only context is shorter, but materially less useful. In UC1 the
rendered context falls from `282` to `200` tokens. In UC2 it falls from `285`
to `203`. In UC3 it falls from `575` to `374`. In UC4 the explanatory `@192`
variant uses `175` tokens, while the structural-only `@96` variant uses `73`.
In all cases that shorter context coincides with severe degradation of causal
reconstruction or loss of the dominant reason.

This matters because bounded retrieval is often optimized for compactness. The
results here show that compression through loss of explanation is the wrong
tradeoff for restartable or auditable agent workflows.

## 8. Discussion

The main implication of the current artifact is narrower and more useful than
a generic claim about graph retrieval. The evidence supports a systems claim
about preservation and reuse of explanatory context: when transition-level
explanations are retained across projection, storage, query, and rendering
boundaries, the resulting context is substantially more useful for diagnosis
and recovery than context built from structure alone.

This is also the right DDD split. The kernel does not decide why an action was
taken and does not infer motivation on its own. That meaning is emitted by the
agent or domain application. The kernel's responsibility is to preserve that
meaning in a form that can be rehydrated later and consumed by another
runtime.

The result matters because many agent systems currently treat context assembly
as an implementation detail of a single runtime. Our artifact suggests that it
is better treated as a reusable service boundary. Once that boundary exists,
diagnostic and recovery workflows can be evaluated directly instead of being
hidden inside prompt construction logic.

## 9. Threats To Validity

The current evidence has several limitations.

- The quantitative use cases are small and synthetic.
- The measured local bundles remain small, ranging from three to seven nodes.
- The current artifact includes only a limited meso-scale distractor graph for `UC1`, so it does not yet establish behavior across substantially denser retrieval neighborhoods.
- The current metrics center on explanatory use cases rather than the full
  pull-driven and event-driven suite.
- Runtime diversity is still limited.
- Explanation quality is application-supplied; the kernel preserves the shape
  and transport of explanation, but it does not validate truthfulness.

These limitations do not invalidate the core result, but they do narrow the
claim. The current paper should therefore be positioned as artifact-backed
systems evidence about preservation and utility of explanatory context, not as
a large-scale task benchmark or a claim of automatic causal discovery.

## 10. Reproducibility

The repository already contains the minimal reproduction path:

```bash
bash scripts/ci/integration-paper-use-cases.sh
```

For broader operational coverage, the artifact can also be paired with:

```bash
bash scripts/ci/integration-agentic-context.sh
bash scripts/ci/integration-agentic-event-context.sh
```

Those broader suites validate pull-driven and event-driven execution paths,
while the paper harness produces the machine-readable explanatory metrics.

## 11. Conclusion

`rehydration-kernel` supports a concrete and defensible systems claim: bounded
context rehydration can be isolated into a reusable kernel, and typed
explanatory relationships materially improve the usefulness of that context
for diagnosis, justification, and recovery.

Across four explanatory use cases, the dominant signal is carried by
relationship explanation rather than by node detail alone. When explanation is
removed, causal reconstruction collapses; when detail is removed but
explanation is preserved, most of the useful trace remains. In operational
cases, explanatory context recovers the correct rehydration or continuation
point. Under token pressure, explanatory context preserves the dominant reason
even after detail is dropped.

The next step is therefore not a different representation, but broader
evaluation: larger graphs, stronger baselines, unified cross-runtime metrics,
and experiments that measure whether the selected rehydration point leads to a
corrected retry.

## 12. References

[1] Darren Edge, Ha Trinh, Newman Cheng, Joshua Bradley, Alex Chao, Apurva
Mody, Steven Truitt, Dasha Metropolitansky, Robert Osazuwa Ness, and Jonathan
Larson. 2024. From Local to Global: A Graph RAG Approach to Query-Focused
Summarization. Preprint. DOI: `10.48550/arXiv.2404.16130`.
Microsoft Research page:
https://www.microsoft.com/en-us/research/publication/from-local-to-global-a-graph-rag-approach-to-query-focused-summarization/

[2] Li Du, Xiao Ding, Kai Xiong, Ting Liu, and Bing Qin. 2021. ExCAR: Event
Graph Knowledge Enhanced Explainable Causal Reasoning. In Proceedings of the
59th Annual Meeting of the Association for Computational Linguistics and the
11th International Joint Conference on Natural Language Processing (Volume 1:
Long Papers), pages 2354-2363. Association for Computational Linguistics.
https://aclanthology.org/2021.acl-long.183/

[3] Haoran Luo, Haihong E, Yuhao Yang, Yikai Guo, Mingzhi Sun, Tianyu Yao,
Zichen Tang, Kaiyang Wan, Meina Song, and Wei Lin. 2023. HAHE: Hierarchical
Attention for Hyper-Relational Knowledge Graphs in Global and Local Level. In
Proceedings of the 61st Annual Meeting of the Association for Computational
Linguistics (Volume 1: Long Papers), pages 8095-8107. Association for
Computational Linguistics. https://aclanthology.org/2023.acl-long.450/

[4] Pranoy Panda, Ankush Agarwal, Chaitanya Devaguptapu, Manohar Kaul, and
Prathosh Ap. 2024. HOLMES: Hyper-Relational Knowledge Graphs for Multi-hop
Question Answering using LLMs. In Proceedings of the 62nd Annual Meeting of
the Association for Computational Linguistics (Volume 1: Long Papers), pages
13263-13282. Association for Computational Linguistics.
https://aclanthology.org/2024.acl-long.717/

[5] Haoran Luo, Haihong E, Yuhao Yang, Tianyu Yao, Yikai Guo, Zichen Tang,
Wentai Zhang, Shiyao Peng, Kaiyang Wan, Meina Song, Wei Lin, Yifan Zhu, and
Anh Tuan Luu. 2024. Text2NKG: Fine-Grained N-ary Relation Extraction for
N-ary relational Knowledge Graph Construction. In Advances in Neural
Information Processing Systems 37. https://proceedings.neurips.cc/paper_files/paper/2024/hash/305b2288122d46bf0641bdd86c9a7921-Abstract-Conference.html

[6] Aikaterini-Lida Kalouli and Richard Crouch. 2018. GKR: the Graphical
Knowledge Representation for semantic parsing. In Proceedings of the Workshop
on Computational Semantics beyond Events and Roles, pages 27-37. Association
for Computational Linguistics. https://aclanthology.org/W18-1304/
