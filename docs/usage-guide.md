# Usage Guide

How to give your AI agent graph-aware context in 3 steps.

Most users integrating a model such as `vLLM` should start with
[`graph-batch-quickstart.md`](graph-batch-quickstart.md).

Mental model:

- Neo4j stores graph topology and short metadata
- Valkey stores long per-node detail
- the kernel reads both, then renders bounded context for your LLM

## Recommended paths

There are three ways to interact with the kernel write/read boundary, but they
do not serve the same purpose.

| Path | Use when | Recommendation |
|:-----|:---------|:---------------|
| `GraphBatch -> translator -> NATS` | Your producer is an LLM or agent and you want to materialize graph context safely | **Default for new model-driven integrations** |
| Raw projection events | Your producer already owns event envelopes and subject publishing | Good for fully deterministic non-LLM producers |
| `UpdateContext` | You need the generic command/event-store path with idempotency and optimistic preconditions | Useful, but **not** the primary graph-materialization path for model output |

If you only remember one integration pattern from this page, use:

`GraphBatch -> graph.node.materialized + node.detail.materialized -> GetContext`

## Step 1 — Seed your graph

Publish projection events to NATS. Each event materializes a node or detail
in the kernel's graph store. Events follow the
[AsyncAPI contract](../api/asyncapi/context-projection.v1beta1.yaml).

### Recommended for model producers: GraphBatch

If your upstream producer is an LLM such as `vLLM`, prefer a two-step shape:

1. The model emits a simple batch with `nodes`, `relations`, and
   `node_details`
2. A translator turns that batch into kernel events

Do not ask the model to invent NATS subjects or event envelopes. Keep the model
on graph semantics and let the adapter own transport details.

Optional stabilizer:

- an experimental `repair-judge` pass can repair an invalid `GraphBatch` before translation
- this helper lives in the testkit and cluster examples today
- it is not part of the stable kernel contract and should be treated as optional infrastructure

The repo includes a minimal adapter for that path:

```rust
use kmp_testkit::{graph_batch_to_projection_events, parse_graph_batch};

let batch = parse_graph_batch(llm_json_payload)?;
let messages = graph_batch_to_projection_events(&batch, "rehydration", "incident-42")?;
```

Example model payload:

```text
api/examples/kernel/v1beta1/async/vllm-graph-batch.json
```

Canonical schema and request example:

```text
api/examples/kernel/v1beta1/async/vllm-graph-batch.schema.json
api/examples/inference-prompts/vllm-graph-materialization.request.json
```

This is the intended mental split:

```mermaid
flowchart LR
    M[LLM or agent] -->|GraphBatch JSON| T[Translator]
    T -->|graph.node.materialized| NATS[NATS JetStream]
    T -->|node.detail.materialized| NATS
    NATS --> P[Kernel projection runtime]
    P --> N4[Neo4j]
    P --> VK[Valkey]
```

Why this is the recommended path:

- the model stays focused on graph semantics
- the adapter owns validation, hashing, envelopes, and subjects
- the stable kernel-owned write boundary remains async projection events

What remains experimental around this path:

- any direct ingress API that accepts `GraphBatch`
- the dedicated `repair-judge` helper used to salvage invalid model output before translation

Operational rule for real clients:

- always attach an `idempotency_key` if you add a direct `GraphBatch` ingress
- use bounded retries only for transient transport failures
- do not blindly retry domain conflicts such as `ABORTED`

Operational rule for model extraction:

- keep the model-generation timeout policy separate from any future `GraphBatch` ingress timeout
- if you use a `repair-judge`, give it its own timeout budget rather than sharing the primary model budget

For the shortest end-to-end explanation of this write path, see
[`graph-batch-quickstart.md`](graph-batch-quickstart.md).

### If you already own the transport: raw projection events

If your producer is not an LLM and already publishes kernel events directly,
you can skip `GraphBatch` and publish the async contract yourself.

**Materialize a node** (published to `rehydration.graph.node.materialized`):

```json
{
  "event_id": "evt-001",
  "correlation_id": "incident-7",
  "causation_id": "alert-latency-spike",
  "occurred_at": "2026-03-27T14:30:00Z",
  "aggregate_id": "incident-7",
  "aggregate_type": "incident",
  "schema_version": "v1beta1",
  "data": {
    "node_id": "node:incident:latency-spike",
    "node_kind": "incident",
    "title": "API latency spike in payments service",
    "summary": "P95 latency exceeded 2s threshold after config deployment.",
    "status": "investigating",
    "labels": ["payments", "latency", "p1"]
  }
}
```

**Materialize a related node with explanatory relationship:**

```json
{
  "event_id": "evt-002",
  "correlation_id": "incident-7",
  "causation_id": "evt-001",
  "occurred_at": "2026-03-27T14:32:00Z",
  "aggregate_id": "decision-reroute",
  "aggregate_type": "decision",
  "schema_version": "v1beta1",
  "data": {
    "node_id": "node:decision:reroute-traffic",
    "node_kind": "decision",
    "title": "Reroute traffic to secondary region",
    "summary": "Divert 80% of traffic while primary recovers.",
    "status": "accepted",
    "related_nodes": [
      {
        "node_id": "node:incident:latency-spike",
        "relation_type": "RESPONDS_TO",
        "explanation": {
          "semantic_class": "causal",
          "rationale": "latency spike caused by config error requires immediate traffic reroute",
          "method": "DNS weight shift to secondary region",
          "decision_id": "decision-reroute",
          "caused_by_node_id": "node:incident:latency-spike"
        }
      }
    ]
  }
}
```

**Materialize node detail** (published to `rehydration.node.detail.materialized`):

```json
{
  "event_id": "evt-003",
  "correlation_id": "incident-7",
  "causation_id": "evt-001",
  "occurred_at": "2026-03-27T14:31:00Z",
  "aggregate_id": "node:incident:latency-spike",
  "aggregate_type": "node_detail",
  "schema_version": "v1beta1",
  "data": {
    "node_id": "node:incident:latency-spike",
    "detail": "Config deployment at 14:25 changed connection pool size from 50 to 5 (typo). P95 latency rose from 120ms to 2.4s within 3 minutes. Error rate: 23% HTTP 503. Affected endpoints: /v1/payments/settle, /v1/payments/refund.",
    "content_hash": "sha256:a1b2c3",
    "revision": 1
  }
}
```

```mermaid
sequenceDiagram
    participant S as Your Service
    participant NT as NATS
    participant P as Kernel Projection
    participant DB as Neo4j / Valkey

    S->>NT: publish event
    NT->>P: deliver
    P->>DB: upsert node
    P->>DB: upsert detail
    P-->>NT: ack
```

## Step 2 — Query context

Call the kernel via gRPC. The kernel traverses the graph, loads details,
renders text ordered by causal salience, and returns it within your token budget.

```bash
grpcurl -plaintext localhost:50054 \
  underpass.rehydration.kernel.v1beta1.ContextQueryService/GetContext \
  -d '{
    "root_node_id": "node:incident:latency-spike",
    "role": "oncall",
    "depth": 3,
    "token_budget": 2000
  }'
```

**What the kernel returns** (simplified):

```json
{
  "rendered": {
    "content": "Node API latency spike in payments service (incident): P95 latency exceeded 2s threshold after config deployment.\n\nRelationship node:decision:reroute-traffic --RESPONDS_TO--> node:incident:latency-spike [causal] because latency spike caused by config error requires immediate traffic reroute via DNS weight shift to secondary region\n\nDetail node:incident:latency-spike [rev 1]: Config deployment at 14:25 changed connection pool size from 50 to 5 (typo)...",
    "tokenCount": 187,
    "resolvedMode": "REHYDRATION_MODE_REASON_PRESERVING",
    "tiers": [
      { "tier": "L0_SUMMARY", "content": "Objective: API latency spike in payments service — P95 latency exceeded 2s. Status: investigating. Next: Reroute traffic to secondary region." },
      { "tier": "L1_CAUSAL_SPINE", "content": "Node API latency spike... Relationship RESPONDS_TO [causal] because latency spike caused by config error..." },
      { "tier": "L2_EVIDENCE_PACK", "content": "Detail: Config deployment at 14:25 changed connection pool size from 50 to 5 (typo)..." }
    ],
    "quality": {
      "rawEquivalentTokens": 342,
      "compressionRatio": 1.83,
      "causalDensity": 1.0,
      "noiseRatio": 0.0,
      "detailCoverage": 1.0
    }
  }
}
```

```mermaid
sequenceDiagram
    participant A as Agent
    participant K as Kernel (gRPC)
    participant N as Neo4j
    participant V as Valkey

    A->>K: GetContext(root, role, depth, budget)
    K->>N: load_neighborhood
    N-->>K: nodes + relationships
    K->>V: load_node_details_batch (MGET)
    V-->>K: details
    Note over K: render + truncate<br/>(salience order, token budget, multi-tier)
    K-->>A: RenderedContext + quality metrics
    Note over A: feed to LLM (your model, your prompt)
```

## Step 3 — Feed your LLM

Take `rendered.content` (or pick specific tiers) and include it in your prompt:

```python
# Full context — use rendered.content
context = response.rendered.content

# Or pick a tier — L1 for diagnosis, L0 for quick triage
# context = next(t.content for t in response.rendered.tiers if t.tier == "L1_CAUSAL_SPINE")

prompt = f"""You are the oncall engineer investigating an incident.
Read this rehydrated context as the source of truth. Prefer causal,
motivational, evidential, procedural, and constraint relations over purely
structural links. Use node details as supporting evidence; do not invent
missing causes or mitigations.

{context}

What is the root cause? What was the recovery decision and why?
Cite the rationale from the context."""

answer = llm.chat(prompt)
```

That's it. The kernel handles graph traversal, salience ordering, token
budgeting, and multi-resolution rendering. Your code just calls gRPC and
feeds the text to your LLM.

## RPCs

### ContextQueryService

| RPC | Use when | Key params |
|:----|:---------|:-----------|
| `GetContext` | Full context around a node | `root_node_id`, `role`, `depth`, `token_budget`, `max_tier`, `rehydration_mode` |
| `GetContextPath` | Context along a path (A → B) | `root_node_id`, `target_node_id`, `role`, `token_budget` |
| `GetNodeDetail` | Extended detail for one node | `node_id` |
| `RehydrateSession` | Bundles for multiple roles at once | `root_node_id`, `roles[]`, `persist_snapshot`, `snapshot_ttl` |
| `ValidateScope` | Check scope coverage (set comparison) | `required_scopes`, `provided_scopes` |

### ContextCommandService

| RPC | Use when | Key params |
|:----|:---------|:-----------|
| `UpdateContext` | Record a generic context change command into the event store | `root_node_id`, `role`, `changes[]`, `metadata` (idempotency_key, correlation_id) |

Important:

- `UpdateContext` is part of the stable `v1beta1` gRPC contract
- it is **not** the recommended primary write path for model-generated graph materialization
- for model output, prefer `GraphBatch -> translator -> async projection events`
- the optional `repair-judge` sits before translation and only repairs invalid model output; it does not change the stable kernel boundary

The same retry discipline applies across both write paths:

- retries need idempotency
- transient transport failures may be retried with backoff
- domain conflicts should be handled explicitly, not replayed blindly

## Multi-Resolution Tiers

Every render produces three tiers. Pick the level you need:

| Tier | Content | Size | Use when |
|:-----|:--------|:----:|:---------|
| **L0 Summary** | Objective, status, blocker, next action | ~100 tok | Status checks, dashboards |
| **L1 Causal Spine** | Root + focus + causal/motivational/evidential chain | ~500 tok | Diagnosis, decision review |
| **L2 Evidence Pack** | Structural relations, neighbors, full details | remaining | Deep analysis, audit |

Request specific tiers:

```json
{ "root_node_id": "node:incident:latency-spike", "role": "oncall", "max_tier": "L1_CAUSAL_SPINE", "token_budget": 800 }
```

## Multi-Role Rehydration

`RehydrateSession` loads the graph **once** and builds per-role bundles from shared data:

```mermaid
sequenceDiagram
    participant A as Agent
    participant K as Kernel
    participant N as Neo4j
    participant V as Valkey

    A->>K: RehydrateSession(roles=[oncall, manager])
    K->>N: load_neighborhood (1 read, shared)
    N-->>K: graph
    K->>V: load_details_batch (1 MGET, shared)
    V-->>K: details
    Note over K: build bundle x 2 (clone, ~0.3ms)
    K-->>A: 2 bundles (1 per role)
```

## Token Budget and Quality

The kernel enforces a token budget using the `cl100k_base` BPE tokenizer.
Content is ordered by salience and truncated when the budget is exceeded.

The planner automatically selects a rehydration mode based on budget pressure
and graph content:
- **ReasonPreserving** (default): all tiers populated, full rationale
- **ResumeFocused**: under tight budgets, prunes L2 evidence to preserve L1 causal spine
- When `causal_density >= 50%`, the planner keeps ReasonPreserving even under pressure

Every response includes quality metrics and auditability data:

| Field | What it tells you |
|:------|:------------------|
| `compressionRatio` | How much the kernel compressed vs a flat dump (>1.0 = savings) |
| `causalDensity` | Fraction of explanatory relationships (higher = richer signal) |
| `detailCoverage` | Fraction of nodes with extended detail loaded |
| `noiseRatio` | Fraction of noise/distractor nodes (0.0 for clean graphs) |
| `resolvedMode` | Which rehydration mode the planner selected |
| `contentHash` | Deterministic hash of rendered content — verify the LLM received what the kernel produced |
| `truncation` | Budget requested vs used, sections kept vs dropped (when budget applied) |

### Provenance

Both nodes and relationships carry optional provenance metadata:
- `source_kind`: HUMAN, AGENT, PROJECTION, DERIVED
- `source_agent`: identifier of the agent that wrote the data
- `observed_at`: ISO-8601 timestamp

This enables auditability: if an LLM cites rationale from a relationship,
the consumer can verify who originally wrote that rationale.

## Event Store and Projection Runtime

### How events become graph state

When you publish events to NATS, the kernel's projection runtime materializes
them into Neo4j (graph) and Valkey (details):

```mermaid
graph LR
    P[Your service] -- publish --> NT[NATS JetStream]
    NT -- pull --> C1[Graph consumer]
    NT -- pull --> C2[Detail consumer]
    C1 -- upsert --> N4[(Neo4j)]
    C2 -- upsert --> VK[(Valkey)]
    C1 -. ack .-> NT
    C2 -. ack .-> NT
```

Three durable pull consumers run in parallel:
- `context-projection-graph-node-materialized` — writes nodes to Neo4j
- `context-projection-graph-relation-materialized` — writes relationships to Neo4j
- `context-projection-node-detail-materialized` — writes detail to Valkey

All three use explicit ack. If a handler fails, the message is nak'd (requeued)
and the runtime stops — operator must investigate and restart.

### Event store backend

The kernel supports two event store backends for `UpdateContext`:

| Backend | Config | Behavior |
|:--------|:-------|:---------|
| **Valkey** (default) | `KMP_EVENT_STORE_BACKEND=valkey` | Events stored as RESP keys, idempotency via key lookup |
| **NATS JetStream** | `KMP_EVENT_STORE_BACKEND=nats` | Events stored in `CONTEXT_EVENTS` stream, file-backed |

Both implement the same `ContextEventStore` port with:
- **Atomic CAS** — NATS uses `Nats-Expected-Last-Subject-Sequence` header; Valkey uses a Lua EVAL script. Concurrent writes to the same `(root_node_id, role)` are rejected with `ABORTED`
- **Idempotency** — outcome recording by key; same key returns same result on retry
- **Revision tracking** — monotonic per `(root_node_id, role)` pair

### NATS JetStream subjects

| Subject pattern | Purpose |
|:----------------|:--------|
| `{prefix}.graph.node.materialized` | Inbound: nodes |
| `{prefix}.graph.relation.materialized` | Inbound: relationships |
| `{prefix}.node.detail.materialized` | Inbound: extended detail |
| `{prefix}.cmd.evt.{root_node_id}.{role}` | Event store: command events |
| `{prefix}.cmd.idem.{key}` | Event store: idempotency outcomes |

Prefix is configured via `KMP_EVENTS_PREFIX` (default: `rehydration`).

### Operational notes

- **Ordering**: sequential within each subject, no cross-subject ordering between graph and detail
- **Failure**: handler error stops the runtime (nak + exit). No poison-pill queue — operator must investigate
- **Deduplication**: application-level via Valkey (`ProcessedEventStore`), separate from JetStream ack
- **Checkpointing**: stored in Valkey (`ProjectionCheckpointStore`), survives restarts
- **State dependency**: even with NATS event store, the projection runtime always uses Valkey for deduplication and checkpoints

### Retry semantics for consumers

The kernel provides **at-least-once** delivery with idempotency key support.
Consumers should design for safe retries.

**UpdateContext retries:**

```
Client                          Kernel
  │                                │
  │  UpdateContext(key="abc")      │
  │───────────────────────────────>│  1. Check idempotency key "abc"
  │                                │  2. Not found → process command
  │                                │  3. Append event (revision N+1)
  │                                │  4. Record outcome for key "abc"
  │  ←── OK (revision N+1)        │
  │                                │
  │  (network timeout — retry)     │
  │                                │
  │  UpdateContext(key="abc")      │
  │───────────────────────────────>│  1. Check idempotency key "abc"
  │                                │  2. Found → return cached outcome
  │  ←── OK (revision N+1, same)  │  (no re-processing)
```

**Rules for consumers:**

- Always set `metadata.idempotency_key` on `UpdateContext`. Without it, retries
  are treated as new requests
- Use a deterministic key per logical operation (e.g. `{correlation_id}:{entity_id}`)
- If you get `ABORTED` (revision conflict), reload current revision and retry
  with the new `expected_revision`
- Idempotency outcomes are persisted but not TTL'd — they live as long as the
  event store retains data

**Projection event retries (NATS):**

- Durable consumers with explicit ack — messages are redelivered on nak
- Application-level deduplication via `ProcessedEventStore` (Valkey) prevents
  re-processing of already-materialized events
- If the projection handler fails, the runtime stops. Restart the pod to
  resume from the last acked message

**Known limitation:** idempotency outcome publish is fire-and-forget. If the
event appends but the outcome publish fails (network issue), the next retry
will be treated as a new request. The kernel logs a warning when this happens.

## Relations Carry the Why

The most important quality lever in KMP memory is not the nodes — it is the
relations. A graph where every link says `related_to` or `supports_answer` is
connected but explains nothing; a graph whose links carry their reason can be
traversed to answer *what happened, in what order, and why each decision was
taken*.

The rules, enforced by the writer and taught by the MCP tool schema itself
(the `connect_to.rel` documentation is generated from the kernel's own
relation spec, so it cannot drift):

- **Every non-structural relation requires `why`, `evidence` and
  `confidence`.** A link you cannot justify is a link you should not write.
- **Prefer rich types** — `triggers`, `chosen_because`, `contradicts`,
  `supports`, `verified_by`, `supersedes`, `depends_on`, `derived_from`,
  `satisfies_constraint`, `violates_constraint`, `confirms_selection`, … Each
  type has a fixed set of allowed semantic classes (causal, motivational,
  evidential, procedural, constraint) and a documented reason to exist.
- **Anemic types (`follows`, `answers`, `uses_background`) are an honest
  fallback**, the explicit signal that the writer could not prove a richer
  semantic dependency. They are never a default, and never to be upgraded
  into causal or motivational language without proof.
- **Read before you link.** A rich relation to existing memory declares the
  refs it inspected (`read_context.inspected_refs`); `kmp_write_memory`
  reports per-relation quality (`rich`/`anemic`/`suspect`, `proof_complete`)
  so the writer — human or model — is accountable for every link.
- Richer explanation fields are available where the semantics earn them:
  `motivation`, `method`, `decision_id`, `caused_by_node_id`, and the
  relation's own temporal coordinate.

The full model is specified in
[research/RELATION_EXPLANATION_MODEL.md](research/RELATION_EXPLANATION_MODEL.md).

## Further Reading

- [GraphBatch Quickstart](graph-batch-quickstart.md) — fastest model-driven path
- [GraphBatch ingestion API](graph-batch-ingestion-api.md) — experimental ingress proposal
- [Proto contracts](../api/proto/underpass/rehydration/kernel/v1beta1/) — gRPC API definition
- [Kernel Memory Protocol API design](product/kernel-context-api-design.md) — the typed `kernel_*` tool surface (wake/ask/near/trace/inspect/write_memory)
- [MCP stdio adapter](operations/mcp-stdio.md) — run the KMP tools over MCP stdio
- [AsyncAPI contract](../api/asyncapi/context-projection.v1beta1.yaml) — event schema
- [Reference fixtures](../api/examples/kernel/v1beta1/grpc/) — example request/response JSON
- [Integration contract](migration/kernel-node-centric-integration-contract.md) — stability rules
- [Beta status](beta-status.md) — RPC maturity, path to v1, known limitations
- [Observability](observability.md) — quality metrics, OTel, Loki, Grafana
- [Testing](testing.md) — how to run the test suite
