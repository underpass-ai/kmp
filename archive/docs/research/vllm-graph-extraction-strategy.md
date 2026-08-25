# vLLM Graph Extraction Strategy

Status: Research note

Date: 2026-04-08

## Question

What is the right boundary for using `vLLM` to materialize graph state for the
rehydration kernel?

More specifically:

- should the model be taught to call a tool
- should it be forced to emit a schema-constrained graph batch
- or should we move to a more open extraction + canonicalization pipeline

This note focuses on the graph materialization path that feeds projection
events. It does **not** claim that `UpdateContext` is already the correct live
ingestion path for Neo4j projection.

## Short Answer

For the current kernel shape, the best default is:

1. semantic prompt
2. strict `JSON Schema` output for `LlmGraphBatch`
3. local validation
4. bounded retry with validation feedback
5. translation to projection events
6. optional post-extraction judging or canonicalization

`tool calling` should **not** be the primary extraction boundary here.

## Why This Matters In This Repo

The repo now has a concrete graph batch contract and a working translation
path:

- [`api/examples/inference-prompts/graph-materialization.txt`](../../../api/examples/inference-prompts/graph-materialization.txt)
- [`api/examples/kernel/v1beta1/async/vllm-graph-batch.json`](../../../api/examples/kernel/v1beta1/async/vllm-graph-batch.json)
- [`crates/rehydration-testkit/src/llm_graph.rs`](../../../crates/rehydration-testkit/src/llm_graph.rs)
- [`crates/rehydration-testkit/src/bin/publish_llm_graph.rs`](../../../crates/rehydration-testkit/src/bin/publish_llm_graph.rs)
- [`crates/rehydration-tests-kernel/tests/llm_graph_materialization_integration.rs`](../../../crates/rehydration-tests-kernel/tests/llm_graph_materialization_integration.rs)
- [`crates/rehydration-testkit/tests/vllm_graph_prompt_smoke.rs`](../../../crates/rehydration-testkit/tests/vllm_graph_prompt_smoke.rs)

That means the open design question is no longer "can the kernel consume a
graph batch?" It can.

The real design question is: what is the most robust and scalable way to make
`vLLM` produce that batch?

## Current Repo Position

As of 2026-04-08:

- the deterministic E2E proves `JSON -> translator -> NATS -> projection
  runtime -> gRPC read`
- the live smoke test proves `prompt -> vLLM -> JSON -> parser/translator`
- the repo still does **not** prove `UpdateContext -> projection -> Neo4j`

So the research scope here is narrow and deliberate:

> What is the best extraction contract for `vLLM` to produce graph material for
> the existing projection pipeline?

## Findings From Papers And Projects

### 1. vLLM itself points toward schema-constrained outputs

The `vLLM` docs support `response_format={"type":"json_schema", ...}` and also
state that it is still better to describe in the prompt how the fields should
be populated. That matches this repo's needs exactly: use the prompt for graph
semantics, and the schema for output shape.

`vLLM` also supports tool calling, but the docs are explicit that named
function calling guarantees a parseable call, not necessarily a high-quality
one. That makes tool calling a weaker default when the main problem is
high-quality graph extraction rather than action selection.

Sources:

- vLLM Structured Outputs:
  <https://docs.vllm.ai/features/structured_outputs.html>
- vLLM Tool Calling:
  <https://docs.vllm.ai/en/stable/features/tool_calling/>

### 2. Structured output research favors constrained decoding

`JSONSchemaBench` frames constrained decoding as the dominant practical method
for reliable structured output generation and evaluates systems against JSON
schemas. This is directly relevant because the kernel does not need free-form
reasoning as its API boundary. It needs a reliably parseable object.

Source:

- Geng et al., "Generating Structured Outputs from Language Models:
  Benchmark and Studies" (2025):
  <https://arxiv.org/abs/2501.10868>

### 3. KG construction research says schema size changes the right strategy

`EDC` (Extract, Define, Canonicalize) is important because it highlights a
real limitation of one-shot schema prompting: as the target ontology grows,
dumping the whole schema into a prompt scales poorly. Their answer is a
pipeline:

1. open extraction
2. relation definition
3. schema canonicalization

This matters because it suggests a transition point:

- for a compact and stable graph contract, strict schema is the right default
- for a broad or evolving ontology, a canonicalization layer becomes more
  important than raw schema forcing

Source:

- Zhang and Soh, "Extract, Define, Canonicalize: An LLM-based Framework for
  Knowledge Graph Construction" (EMNLP 2024):
  <https://aclanthology.org/2024.emnlp-main.548/>

### 4. Graph extraction quality benefits from validation and judging

`GraphJudge` argues that extraction quality degrades under noise and domain
shift, and that a judge or clean-up stage can materially improve the resulting
graph. This does not mean the kernel should add a second model pass today, but
it does support a staged design:

- first get reliable shape with schema-constrained extraction
- then add judge/canonicalization only if quality errors dominate

Source:

- Huang et al., "Can LLMs be Good Graph Judger for Knowledge Graph
  Construction?" (2024):
  <https://arxiv.org/abs/2411.17388>

### 5. Recent KG extraction work still leans on schema and examples

Recent triplet extraction work also reinforces that strong schema/context
guidance matters. The practical takeaway is not "use the biggest model", but
"provide a disciplined extraction target and good examples".

Source:

- KaLLM 2024, "Zero- and Few-Shots Knowledge Graph Triplet Extraction with
  Large Language Models":
  <https://aclanthology.org/2024.kallm-1.2/>

## What Serious Projects Actually Do

### Microsoft GraphRAG

GraphRAG extracts per text unit, merges subgraphs, and then summarizes entity
and relationship descriptions. This is not a direct `vLLM` serving recipe, but
it shows the production pattern clearly:

- extract in bounded local units
- merge duplicates
- summarize/canonicalize later

That is compatible with this repo's event pipeline and argues against trying to
push all graph semantics into one giant tool call.

Sources:

- GraphRAG dataflow:
  <https://microsoft.github.io/graphrag/index/default_dataflow/>
- GraphRAG outputs:
  <https://microsoft.github.io/graphrag/index/outputs/>

### LlamaIndex

LlamaIndex's `SchemaLLMPathExtractor` is one of the clearest signals from a
production-oriented graph stack. It uses structured outputs plus validation and
lets the caller choose `strict=True` when the schema should be enforced.

Sources:

- Property Graph guide:
  <https://docs.llamaindex.ai/en/stable/module_guides/indexing/lpg_index_guide/>
- Predefined schema example:
  <https://docs.llamaindex.ai/en/latest/examples/property_graph/property_graph_advanced/>

### Neo4j GraphRAG

Neo4j's KG builder pipeline explicitly distinguishes schema building, entity
and relation extraction, and graph pruning. It also notes that schema guidance
helps ground extraction, but the resulting graph may still need filtering and
clean-up.

This again points to the same pattern: schema first, pruning/canonicalization
second.

Sources:

- KG builder user guide:
  <https://neo4j.com/docs/neo4j-graphrag-python/current/user_guide_kg_builder.html>
- LLM Knowledge Graph Builder overview:
  <https://neo4j.com/blog/developer/llm-knowledge-graph-builder/>

### Outlines and Instructor

These are not graph systems, but they are useful signals from production
structured-output practice:

- Outlines emphasizes constrained structured generation from JSON schema or
  Pydantic
- Instructor emphasizes validation and retry with feedback

Those patterns fit the kernel boundary much better than tool calling as the
primary contract.

Sources:

- Outlines JSON structured generation:
  <https://dottxt-ai.github.io/outlines/reference/generation/json/>
- Instructor validation:
  <https://python.useinstructor.com/concepts/validation/>
- Instructor retry mechanisms:
  <https://python.useinstructor.com/learning/validation/retry_mechanisms/>

## Decision Matrix

| Approach | Fit now | Why |
|:---------|:--------|:----|
| Prompt-only free JSON | No | Cheap, but brittle on shape, ids, and relation references |
| Tool calling as primary boundary | Limited | Parseable call shape, but extra serving complexity and weaker signal on semantic graph quality |
| Prompt + strict `JSON Schema` batch | Yes | Best match for the current compact `LlmGraphBatch` contract |
| Open extraction + canonicalization | Later | Better once ontology drift, synonym collapse, or schema growth dominate |

## Recommended Architecture For This Repo

### Phase 1: Stable extraction boundary

Use `vLLM` with:

- a semantic extraction prompt
- `response_format={"type":"json_schema", ...}`
- the existing `LlmGraphBatch` contract as the target schema

The prompt should teach:

- what the `root_node_id` means
- how to orient relations
- what qualifies as a `node_detail`
- how to avoid invention

The schema should enforce:

- field presence
- nesting
- enums where useful
- overall JSON validity

### Phase 2: Local validation

The kernel-side validator should keep enforcing invariants that are awkward to
push fully into a schema:

- `root_node_id` must exist in `nodes`
- all referenced node ids must exist
- relations must point to valid nodes
- details must target valid nodes
- duplicate ids should fail

This is already the right boundary for
[`parse_llm_graph_batch`](../../../crates/rehydration-testkit/src/llm_graph.rs).

### Phase 3: Bounded retry with validation feedback

If parsing or validation fails:

1. feed the concrete validation errors back to the model
2. retry once or twice
3. fail closed if the batch is still invalid

This is an inference from production structured-output practice rather than a
claim specific to the kernel codebase, but it is strongly supported by the
Instructor pattern and by the general constrained-output literature.

### Phase 4: Translation and publishing

Once validated, the batch should be translated to:

- `graph.node.materialized`
- `node.detail.materialized`

and then published through the existing event pipeline.

### Phase 5: Optional judge or canonicalizer

If extraction quality becomes the main problem, add a second-stage component
for:

- relation canonicalization
- duplicate collapse
- low-confidence edge rejection
- provenance enrichment

That is the point where an `EDC`-like or `GraphJudge`-like layer starts to
make sense.

## When Tool Calling *Does* Make Sense

Tool calling still has a role, but not as the first extraction boundary.

It becomes useful when the model must choose among actions such as:

- `emit_graph_batch`
- `ask_for_more_evidence`
- `emit_no_update`
- `request_human_review`

In that design, tool calling is an orchestration surface. The graph payload
inside the tool should still be schema-constrained.

## When To Move Beyond The Current Schema-First Design

The current recommendation should be revisited if one or more of these become
true:

- the ontology grows beyond a compact, stable set of node and relation shapes
- relation synonyms create heavy canonicalization pressure
- cross-batch deduplication dominates error rate
- extraction must merge many local subgraphs into a large global graph
- a single batch routinely becomes too large for stable extraction quality

These thresholds are engineering heuristics, not hard research boundaries. The
main signal is simple: if the model keeps producing syntactically valid batches
that are semantically inconsistent, then the next investment should be
canonicalization, not stronger tool-calling machinery.

## Implication For The Paper

This research direction does **not** weaken the core paper claim.

The paper evidence in this repo is still about:

- graph-aware rehydration quality
- bounded retrieval over materialized graph state
- comparative behavior across context variants

It is **not** yet evidence that the live ingestion path from a model-generated
write request into Neo4j projection is production-ready.

That distinction should remain explicit in any paper or product-facing claim.

## Recommended Next Experiments

1. Compare `prompt-only` vs `prompt + json_schema` on the active cluster model.
2. Compare `tool calling` vs `json_schema` for validity rate, latency, and
   downstream translation success.
3. Add one retry-with-feedback experiment and measure recovery rate.
4. Expand the E2E from the current tiny graph to a medium batch
   (roughly 30-50 nodes, 60-100 relations).
5. Add an incremental test with multiple batches over the same `root_node_id`.

## Bottom Line

The strongest current recommendation is:

> For `vLLM`-driven graph materialization in this repo, use a semantic prompt
> plus strict `JSON Schema`, validate locally, retry narrowly on failure, and
> keep tool calling only for higher-level action selection.

That is where the literature, the `vLLM` docs, and the closest production
graph projects converge.
