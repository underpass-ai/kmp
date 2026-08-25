# Why This Kernel Is Useful For Agents

## The Problem

LLMs have bounded context windows. Agents that operate on knowledge graphs
need to fit relevant context into that window without losing the reasoning
chain that explains *why things are the way they are*.

Most approaches give the LLM either:

- **Flat text** — dumps everything, loses structure, wastes tokens
- **Vector search (RAG)** — finds similar chunks, misses causal chains
- **Graph dumps** — preserves structure, but no semantic weight on edges

None of these tell the LLM: "this decision caused that failure, and here's
the rationale."

## What This Kernel Actually Does

The kernel takes a knowledge graph with typed, explained relationships and
renders it into token-budgeted text where **causal chains come first**.

Practical capabilities today:

### 1. Explained relationships survive end-to-end

Every edge carries `semantic_class`, `rationale`, `method`, `decision_id`,
`caused_by_node_id`, and `sequence`. These flow from event ingestion through
Neo4j projection to rendered output. The LLM sees *why*, not just *what*.

### 2. Salience ordering under token pressure

When budget is tight, the kernel keeps causal > motivational > evidential
relationships and drops structural noise. At 512 tokens with 49 nodes,
`ResumeFocused` mode preserves the causal spine and achieves 83-89% accuracy
where structural-only drops to 28-44%.

### 3. Multi-resolution tiers

Every render produces L0 (summary), L1 (causal spine), L2 (evidence pack)
simultaneously. An agent can pick L0 for quick triage, L1 for diagnosis,
or full context for deep analysis — one gRPC call, no re-rendering.

### 4. Role-based multi-bundle

`RehydrateSession` loads the graph once and builds per-role bundles.
An oncall engineer and a product manager get different views of the same
incident from a single read.

### 5. Quality metrics on every render

`compression_ratio`, `causal_density`, `noise_ratio`, `detail_coverage` —
observable via OTel and Loki. The agent (or its operator) can see if the
context is actually useful before sending it to the LLM.

### 6. Model-agnostic

Works with GPT, Claude, Llama, Qwen, or any LLM. The kernel renders text,
not model-specific tokens.

## How We Measure

The kernel is evaluated following a scientific methodology
([anexo-procedimiento-cientifico-analisis-datos.md](anexo-procedimiento-cientifico-analisis-datos.md)):

- **Hypothesis**: explanatory relationships improve LLM context quality
  over structural-only edges
- **Null hypothesis**: no difference between explanatory and structural
- **Variables**: relation mix (explanatory vs structural), scale (micro/meso/stress),
  noise mode (clean/competing/conflicting/restart), token budget, model
- **Evaluation**: LLM-as-judge with independent inference agent and judge model
- **Replication**: 3 seeds per cell, 3 agents × 3 judges for cross-validation

Preliminary results (subset run):

- Explanatory consistently outperforms structural (27-72pp gap)
- Gap widens under token pressure and adversarial noise
- `ResumeFocused` recovers accuracy under extreme budgets

Full matrix run planned (see [kernel-roadmap-milestones.md](../../../docs/product/kernel-roadmap-milestones.md)).

## Where It Fits

The kernel is a **practical context engine**, not a research artifact
competing for SOTA in agent memory or graph reasoning.

It is useful when:

- Your agent operates on a knowledge graph with decisions, incidents,
  tasks, or any domain entities connected by explained relationships
- You need bounded, prioritized context for an LLM prompt
- You care about *why* things are connected, not just *that* they are
- You need observability on context quality before it reaches the LLM
- You want one infrastructure that works across models and roles

It is NOT useful when:

- Your context is unstructured text (use RAG instead)
- Your relationships have no semantic meaning (the kernel's value is in
  typed, explained edges)
- You need similarity search over embeddings (the kernel traverses a typed
  graph, not a vector space)

## Open Questions

- Can the kernel's compression ratio claims hold on real production graphs
  (hundreds of nodes, not tens)?
- Does the causal density metric correlate with actual LLM output quality
  in production (not just benchmark)?
- Is the salience ordering robust across domains beyond Operations and
  SoftwareDebugging?
- Can `RehydrateSession` support quality metrics per role without
  unacceptable latency?

These are planned investigations, not solved problems.
