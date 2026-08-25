# GraphBatch Quickstart

The fastest way to feed new graph context into the kernel.

## Who this is for

Start here if:

- your upstream producer is an LLM or agent, such as `vLLM`
- you want the kernel to own traversal, detail loading, and rendering
- you do **not** want your model to invent NATS subjects or event envelopes

## The 30-second mental model

The kernel stores graph context in two places:

- **Neo4j** holds graph topology and short node metadata
- **Valkey** holds the long per-node detail text

Your producer should therefore emit one bounded `GraphBatch`:

- `nodes[]` for graph entities
- `relations[]` for graph edges and explanations
- `node_details[]` for the long evidence attached to those nodes

The translator then turns that batch into the kernel's stable async contract:

- `graph.node.materialized`
- `node.detail.materialized`

## Why this path exists

Do not ask the model to produce:

- event ids
- correlation envelopes
- NATS subjects
- projection transport details

Those are adapter concerns, not model concerns.

The model should stay focused on graph semantics:

- what nodes exist
- how they relate
- what evidence belongs in node detail

## The simplest flow

1. Ask the model for a `GraphBatch` JSON object.
2. Parse and validate the batch.
3. Translate it to projection events.
4. Publish those events to NATS.
5. Query the kernel with `GetContext` or `GetNodeDetail`.

```rust
use kmp_testkit::{graph_batch_to_projection_events, parse_graph_batch};

let batch = parse_graph_batch(llm_json_payload)?;
let messages = graph_batch_to_projection_events(&batch, "rehydration", "incident-42")?;
```

## Canonical fixtures

Use these files as the source of truth:

- Batch example:
  [`api/examples/kernel/v1beta1/async/vllm-graph-batch.json`](../api/examples/kernel/v1beta1/async/vllm-graph-batch.json)
- Batch schema:
  [`api/examples/kernel/v1beta1/async/vllm-graph-batch.schema.json`](../api/examples/kernel/v1beta1/async/vllm-graph-batch.schema.json)
- `vLLM` request example:
  [`api/examples/inference-prompts/vllm-graph-materialization.request.json`](../api/examples/inference-prompts/vllm-graph-materialization.request.json)
- Prompt:
  [`api/examples/inference-prompts/graph-materialization.txt`](../api/examples/inference-prompts/graph-materialization.txt)

## What makes a good GraphBatch

- Include the `root_node_id` node inside `nodes[]`
- Keep the batch local to one aggregate
- Put short summaries in nodes and long evidence in `node_details[]`
- Prefer a few strong explanatory relations over many weak ones
- Include `confidence` on non-structural relations
- Reject disconnected or speculative duplicates

## What is stable vs experimental

Stable today:

- gRPC reads: `GetContext`, `GetContextPath`, `GetNodeDetail`, `RehydrateSession`
- async projection subjects: `graph.node.materialized`, `node.detail.materialized`

Experimental today:

- a higher-level ingress API that accepts `GraphBatch` directly
- the dedicated `repair-judge` helper that repairs invalid model output before translation

That means:

- `GraphBatch` is the recommended producer shape
- the stable kernel-owned write boundary is still the async projection contract
- the `repair-judge` is optional scaffolding around model extraction, not part of the stable write boundary

## Evidence that this works

The repo now has three proof points:

- a live `vLLM` smoke using strict schema-constrained output
- a deterministic minimal materialization E2E
- a deterministic medium incremental E2E over the same root aggregate

There is also an experimental live repair path:

- if a primary model response is invalid, a dedicated `repair-judge` can rewrite it against the same `GraphBatch` contract
- that path is useful for stabilization and benchmarking, but it is still experimental

See:

- [`docs/testing.md`](testing.md)
- [`docs/graph-batch-ingestion-api.md`](graph-batch-ingestion-api.md)
- [archived GraphBatch decision record](../archive/docs/adr/ADR-008-graph-batch-ingestion-boundary.md)

## Next documents

- If you want the broad product view:
  [`usage-guide.md`](usage-guide.md)
- If you want the proposed ingress API:
  [`graph-batch-ingestion-api.md`](graph-batch-ingestion-api.md)
- If you want the current typed KMP boundary:
  [`api/proto/README.md`](../api/proto/README.md)
