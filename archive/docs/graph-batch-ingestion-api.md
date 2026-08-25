# GraphBatch Ingestion API

> Archived: this proposal is retained for research and traceability. It is not
> a current KMP API commitment.

Status: Experimental
Scope: proposed ingress API for graph materialization before stable contract freeze

## Intent

Define the smallest practical API for submitting graph materialization updates
without exposing event envelopes or asking callers to publish async projection
events directly.

This document is deliberately narrower than `UpdateContext`.

It is for the path:

`producer -> GraphBatch ingress -> validation -> translation -> projection events`

It is **not** a claim that `UpdateContext -> projection -> Neo4j` is already
the correct product write path.

## Design Rules

- one request updates one aggregate rooted at `root_node_id`
- graph topology stays in the request; transport metadata stays outside it
- nodes, relations, and node details are submitted together
- validation runs before any projection event is published
- accepted requests acknowledge ingestion, not read-model completion

## Proposed Domain Shape

### GraphBatch

`GraphBatch` is the domain payload:

- `root_node_id`
- `nodes[]`
- `relations[]`
- `node_details[]`

It follows the same bounded rules already enforced by the testkit translator:

- root node must be present
- all references must target existing nodes
- non-root nodes must be reachable from the root through outward relations
- non-structural relations must include support and `confidence`
- duplicate relations are rejected

Canonical JSON fixture:

- [`api/examples/kernel/v1beta1/async/vllm-graph-batch.json`](../../api/examples/kernel/v1beta1/async/vllm-graph-batch.json)

Canonical JSON schema:

- [`api/examples/kernel/v1beta1/async/vllm-graph-batch.schema.json`](../../api/examples/kernel/v1beta1/async/vllm-graph-batch.schema.json)

### CommandMetadata

Command metadata should travel next to the batch, not inside its domain body:

- `idempotency_key`
- `correlation_id`
- `causation_id`
- `requested_by`
- `requested_at`

This reuses the existing metadata concept already present in
[`command.proto`](../../api/proto/underpass/rehydration/kernel/v1beta1/command.proto).

## Proposed API Shape

### Transport-neutral command

```text
SubmitGraphBatchRequest
├── batch: GraphBatch
└── metadata: CommandMetadata

SubmitGraphBatchResponse
├── acceptance_id
├── accepted_root_node_id
├── accepted_nodes
├── accepted_relations
├── accepted_details
└── warnings[]
```

### Why this is the right response shape

The response should acknowledge command acceptance only.

It should **not** return `BundleVersion`, because projection is asynchronous and
read-model completion is a separate concern.

If a caller needs proof that projection completed, it should:

1. wait for a readiness signal, or
2. query `GetContext` / `GetNodeDetail`

## Retry and Timeout Policy

This API is intended to be retried by clients, but only in a bounded and
typed way.

### Required client behavior

- always send `metadata.idempotency_key`
- treat the command as **acceptance-only**, not read-after-write
- use bounded retries with exponential backoff and jitter

Recommended default policy:

| Attempt | Delay |
|:--------|------:|
| 1 | immediate |
| 2 | 250 ms + jitter |
| 3 | 1 s + jitter |
| 4 | 3 s + jitter |

After the final attempt, surface the error to the caller.

### Safe-to-retry failures

Clients should retry only on transient transport or availability failures:

- gRPC `UNAVAILABLE`
- gRPC `DEADLINE_EXCEEDED`
- connection reset or handshake interruption
- HTTP `502`, `503`, `504` if an HTTP adapter exists

### Do not blindly retry domain conflicts

Clients should **not** blindly replay the same request on:

- gRPC `ABORTED`
- validation rejection
- authorization rejection

Those are not transient transport failures. They require either:

1. reload current state,
2. rebuild the batch, or
3. surface the rejection to the producer.

### Timeouts

Recommended defaults for the ingress call itself:

- request timeout: `5s`
- connect timeout: `2s`
- no infinite retries

These defaults are for a future direct `SubmitGraphBatch` call.

They do not cover upstream model extraction. If your producer uses a primary
model and an optional `repair-judge`, those model calls should keep their own
independent timeout budgets.

The response acknowledges ingestion only. A timeout does not imply that the
request was rejected. This is exactly why `idempotency_key` is mandatory.

## Why retries belong in the client, not in a sidecar

At this stage, retries should live in the calling client or a thin ingress
adapter, not in an Envoy sidecar or mesh policy.

Reason:

- retry safety depends on domain semantics and `idempotency_key`
- transport-only retries cannot distinguish transient failures from domain conflicts
- the current topology is simple enough that a sidecar would add more moving
  parts than value

Envoy or service-mesh retries should only be revisited if this ingress becomes
a shared multi-client platform boundary with centralized traffic policy needs.

The dedicated `repair-judge` used in the current testkit is a separate,
experimental model-stabilization helper. It is not this API.

## Example JSON Request

```json
{
  "batch": {
    "root_node_id": "incident-2026-04-09-checkout-latency",
    "nodes": [
      {
        "node_id": "incident-2026-04-09-checkout-latency",
        "node_kind": "incident",
        "title": "Checkout latency spike",
        "summary": "P95 checkout latency exceeded 2.5s after a rollout.",
        "status": "INVESTIGATING",
        "labels": ["incident", "checkout", "p1"],
        "properties": {
          "service": "checkout-api",
          "region": "eu-west-1"
        }
      }
    ],
    "relations": [],
    "node_details": []
  },
  "metadata": {
    "idempotency_key": "graph-batch-incident-2026-04-09-checkout-latency-wave-1",
    "correlation_id": "incident-2026-04-09-checkout-latency",
    "causation_id": "analysis-wave-1",
    "requested_by": "diagnostic-agent",
    "requested_at": "2026-04-09T09:00:00Z"
  }
}
```

## Example JSON Response

```json
{
  "acceptance_id": "graph-batch-incident-2026-04-09-checkout-latency-wave-1",
  "accepted_root_node_id": "incident-2026-04-09-checkout-latency",
  "accepted_nodes": 6,
  "accepted_relations": 5,
  "accepted_details": 3,
  "warnings": []
}
```

## Validation Semantics

### Reject

Reject the request if:

- the root node is missing
- any node id is duplicated
- any relation references an unknown node
- any non-root node is disconnected from the root
- any non-structural relation lacks support or confidence

### Accept with warnings

Allow warnings for:

- low-confidence but still valid relations
- empty optional provenance fields
- omitted optional detail hashes when the server can derive them
- retry-safe acceptance after idempotency replay

## Mapping To Existing Kernel Contracts

If accepted, the ingress layer translates one `GraphBatch` into:

- `graph.node.materialized`
- `node.detail.materialized`

The current E2E evidence already covers that path.

Relevant proof points:

- live `vLLM` smoke through strict schema
- minimal materialization E2E
- medium incremental materialization E2E

## Minimal client example

Pseudo-flow:

```text
1. build GraphBatch
2. attach idempotency_key
3. submit with 5s timeout
4. retry only on transient transport errors
5. after acceptance, poll readiness or call GetContext/GetNodeDetail
```

## Why This Is More DDD Than `UpdateContext`

`GraphBatch` models the aggregate we actually materialize:

- graph nodes
- graph relations
- node details

It does not hide those concepts inside generic `payload_json`.

That makes:

- invariants explicit
- validation simpler
- translation deterministic
- later API evolution easier to reason about

## What Is Not Frozen Yet

- exact gRPC service name
- exact HTTP mapping if an HTTP adapter is offered
- projection completion notifications
- tenancy propagation
- authorization model for write ingress

Those should remain experimental until a real consumer uses this boundary in a
production workflow.
