# PIR Kernel Integration Reference

Status: Active consumer-side guide  
Scope: first live `PIR -> kernel` integration slice

## Intent

Define the smallest integration shape that lets `PIR`:

1. materialize incident context into the kernel
2. read it back via gRPC
3. do both with stable identity and retry discipline

This is not a new kernel transport contract. It is consumer-side guidance for
an integrating product.

## The 5-step minimum

### 1. Close the write path

For `PIR`, the write path should be:

`GraphBatch -> translator -> graph.node.materialized + node.detail.materialized`

Do not use `UpdateContext` as the primary path for model-driven incident
materialization. `UpdateContext` remains part of the stable gRPC contract, but
it is not the recommended graph-ingestion path for this integration.

### 2. Define stable IDs

`PIR` needs stable graph identity before anything else.

Recommended incident-style pattern:

- root incident: `incident:{external_incident_id}`
- findings: `finding:{external_incident_id}:{slug}`
- decisions: `decision:{external_incident_id}:{slug}`
- artifacts: `artifact:{external_incident_id}:{slug}`
- evidence: `evidence:{external_incident_id}:{slug}`

Rules:

- keep the same `root_node_id` across all waves of the same incident
- never generate a fresh random root id per extraction attempt
- keep child ids stable when the same finding or decision is updated later
- move long evidence to `node_details[]`, not to the node summary

Reference fixture:

- [`api/examples/kernel/v1beta1/async/incident-graph-batch.json`](../../api/examples/kernel/v1beta1/async/incident-graph-batch.json)

### 3. Use a thin adapter

`PIR` should not ask the model to emit transport envelopes or NATS subjects.

The adapter should do exactly this:

1. obtain a `GraphBatch`
2. validate it locally
3. translate it to projection events
4. publish those events to NATS

The reusable pieces already live in the repo:

- `kmp_testkit::parse_graph_batch`
- `kmp_testkit::graph_batch_to_projection_events`
- [`publish_llm_graph.rs`](../../crates/kmp-testkit/src/bin/publish_llm_graph.rs)

### 4. Keep the read path minimal

For the first live integration, `PIR` only needs:

- `GetContext`
- `GetNodeDetail`

Suggested first query shape:

- `root_node_id`: the incident root
- `role`: one stable consumer role from `PIR`
- `requested_scopes`: start with `graph` and `details`
- `depth`: `2`
- `token_budget`: `2048`

That is enough to prove the roundtrip:

`publish GraphBatch -> wait for projection -> GetContext -> GetNodeDetail`

### 5. Make retries explicit

There are two different retry domains here and they should stay separate.

**Model extraction / repair**

- primary model timeout budget
- optional experimental `repair-judge` budget

**Kernel ingestion / query**

- NATS publish retry policy
- gRPC query retry policy

For the raw projection-event path, the critical identity is `run_id`.

The translator derives `event_id` values from `run_id`, so retries are only
idempotent if the same logical publish attempt reuses the same `run_id`.

Operational rule:

- keep `root_node_id` stable across the incident
- keep `run_id` stable across retries of the same publish wave
- change `run_id` only when `PIR` intentionally emits a new materialization wave

## Roundtrip Smoke

The repo now includes a generic consumer-side smoke binary:

- [`graph_batch_roundtrip.rs`](../../crates/kmp-testkit/src/bin/graph_batch_roundtrip.rs)

What it does:

1. reads a `GraphBatch` fixture
2. translates and publishes it to NATS
3. polls the kernel until `GetContext` succeeds
4. optionally fetches `GetNodeDetail`
5. prints a JSON summary

### Local plaintext example

```bash
cargo run -p kmp-testkit --bin graph_batch_roundtrip -- \
  --input api/examples/kernel/v1beta1/async/incident-graph-batch.json \
  --nats-url nats://127.0.0.1:4222 \
  --grpc-endpoint http://127.0.0.1:50054 \
  --run-id pir-wave-1 \
  --role incident-commander \
  --requested-scope graph \
  --requested-scope details
```

### Cluster mTLS example

This shape is intended for an in-cluster client pod or job that already has the
kernel TLS material mounted. In `underpass-runtime`, use TLS with the mounted
CA/cert/key paths, but do not enable `--nats-tls-first`.

```bash
cargo run -p kmp-testkit --bin graph_batch_roundtrip -- \
  --input api/examples/kernel/v1beta1/async/incident-graph-batch.json \
  --nats-url nats://kmp-nats:4222 \
  --nats-tls-ca-path /var/run/kmp/nats-tls/ca.crt \
  --nats-tls-cert-path /var/run/kmp/nats-tls/tls.crt \
  --nats-tls-key-path /var/run/kmp/nats-tls/tls.key \
  --grpc-endpoint https://kmp:50054 \
  --grpc-tls-ca-path /var/run/kmp/tls/ca.crt \
  --grpc-tls-cert-path /var/run/kmp/tls/tls.crt \
  --grpc-tls-key-path /var/run/kmp/tls/tls.key \
  --grpc-tls-domain-name kmp \
  --run-id pir-wave-1 \
  --role incident-commander \
  --requested-scope graph \
  --requested-scope details
```

## What this proves

If the smoke passes, `PIR` already has the minimum viable integration:

- stable graph identity
- model-safe write path
- queryable read path
- retry semantics that can be reasoned about

## Next step after the smoke

Once the roundtrip is stable, the next slice should move from fixture-driven
publishes to real `PIR` extraction output, still using the same `GraphBatch`
shape and adapter.
