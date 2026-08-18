# Inference Prompt Examples

The rehydration kernel is LLM-agnostic. It renders structured context via
gRPC (`GetContext`, `GetContextPath`, `RehydrateSession`) and returns text
that any LLM can consume.

These example prompts show how to feed rehydrated context into an LLM for
common agentic tasks. They are **not** part of the kernel contract — adapt
them to your model, domain, and task.

## How the kernel renders context

The `RenderedContext` response contains:

- `content` — flat text with salience-ordered sections (root, focus,
  relationships by semantic class, node details)
- `sections[]` — keyed sections for selective consumption
- `tiers[]` — multi-resolution tiers:
  - **L0 Summary** — objective, status, blocker, next action (~100 tokens)
  - **L1 Causal Spine** — root + focus + explanatory relations (~500 tokens)
  - **L2 Evidence Pack** — structural relations, neighbors, details (remaining)

Choose the level of detail based on your token budget and task.

## Prompt examples

### 1. Failure diagnosis (incident response)

Use case: an agent needs to identify what went wrong and where to restart.

```
api/examples/inference-prompts/failure-diagnosis.txt
```

### 2. Implementation rationale (code review, audit)

Use case: an agent needs to explain why a task was implemented a specific way.

```
api/examples/inference-prompts/implementation-rationale.txt
```

### 3. Handoff resume (shift change, agent swap)

Use case: a new agent picks up work from a previous agent mid-task.

```
api/examples/inference-prompts/handoff-resume.txt
```

### 4. Constraint-aware planning (safety, compliance)

Use case: an agent plans under binding constraints that must be preserved.

```
api/examples/inference-prompts/constraint-planning.txt
```

### 5. Graph materialization (write path)

Use case: an agent must emit structured graph updates that can be translated
into kernel projection events.

```
api/examples/inference-prompts/graph-materialization.txt
```

### 6. Kernel context consumption (PIR read path)

Use case: an agent consumes `rendered.content` from `GetContext` after a graph
has been materialized.

```
api/examples/inference-prompts/kernel-context-consumption.txt
```

### 7. Kernel write memory (writer-first KMP)

Use case: an agent emits a safe `kernel_write_memory` argument object after
using kernel read tools to build auditable `read_context`.

```text
api/examples/inference-prompts/kernel-write-memory.txt
```

OpenAI-compatible structured request example:

```text
api/examples/inference-prompts/kernel-write-memory.request.json
```

## Tips

- **Keep rationale and proof distinct**: `why` explains the specific semantic
  connection; `evidence` names the concrete observation or source that
  supports it. KMP preserves and uses both, but generates neither.
- **Use tiers under budget pressure**: L0+L1 gives the causal spine in
  ~600 tokens. Feed only those tiers when the full context is too large.
- **Structured JSON output**: ask for JSON responses when you need to
  parse the LLM output programmatically.
- **Write through KMP**: for new memory, prefer `kernel_write_memory` over
  low-level `kernel_ingest` unless the caller already has canonical entries,
  relations, evidence, provenance, and idempotency.
- **Semantic class hints**: mention that relationships have `semantic_class`
  (causal, motivational, evidential, etc.) so the LLM knows which
  relationships carry explanatory weight.

## If You Want The LLM To Fill The Graph

Do not ask the model to emit full kernel events with `event_id`,
`correlation_id`, and NATS subjects. That is transport noise, not model work.

The recommended pattern is:

1. Ask the model for a simple batch with `nodes`, `relations`, and
   `node_details`.
2. Translate that batch into `graph.node.materialized` and
   `node.detail.materialized`.
3. Publish the translated events to NATS.

Example batch payload:

```text
api/examples/kernel/v1beta1/async/vllm-graph-batch.json
```

Canonical schema for that payload:

```text
api/examples/kernel/v1beta1/async/vllm-graph-batch.schema.json
```

Example prompt for that extraction task:

```text
api/examples/inference-prompts/graph-materialization.txt
```

Example OpenAI-compatible request body for `vLLM`:

```text
api/examples/inference-prompts/vllm-graph-materialization.request.json
```

The repo now ships a minimal translator for that contract:

- `kmp_testkit::parse_graph_batch`
- `kmp_testkit::graph_batch_to_projection_events`

Legacy helper names remain available for compatibility:

- `kmp_testkit::parse_llm_graph_batch`
- `kmp_testkit::llm_graph_to_projection_events`

That keeps the LLM focused on graph semantics while the translator owns
envelopes, hashing, validation, and subject naming.

Use `response_format.type=json_schema` for current `vLLM` structured outputs.
Do not use deprecated `guided_json` examples from older snippets.
