# Runtime Guarantees

What the kernel guarantees, what it delegates, and what it does not do.

## Executable Definition: The Conformance Suite

The storage semantics described in this document are pinned by the
backend-independent conformance suite in `crates/rehydration-conformance`:
scenario tests expressed only in terms of the domain ports and the KMP
application services (write→read coherence, depth-bounded traversal, context
paths, ingest idempotency, projection-event dedup, replay safety,
known-at-time temporal navigation, relation proof). The same suite runs in CI
against the in-memory kernel store (`test` job) and against the containerized
Neo4j/Valkey adapters (`integration-conformance` job). A storage backend that
passes the suite implements KMP storage semantics; a behavior difference
between backends is a bug by definition.

## Consistency Model

The kernel uses **optimistic concurrency with CAS (compare-and-swap)**:

1. Client sends `UpdateContext` with optional `expected_revision` and
   `expected_content_hash`
2. Application layer loads current revision from event store
3. If revision or hash mismatch → `Conflict` (gRPC `ABORTED`)
4. If match → append event atomically via CAS

**CAS implementations:**
- **NATS JetStream**: `expected_last_subject_sequence` header on publish.
  Server rejects if another writer advanced the sequence.
- **Valkey**: Lua EVAL script atomically compares revision and writes
  event + revision + hash in a single round-trip.

Both prevent lost updates under concurrent writes. If CAS fails, no event
is written and the client receives an immediate conflict response.

## Delivery Semantics

**At-least-once delivery.** Events are durably stored (NATS file-backed
JetStream) and consumed via durable consumers with explicit ACK.

- Messages not ACKed until successfully processed
- On processing error: NAK triggers redelivery by the NATS broker
- Durable consumer survives restarts and replays from the beginning

**Event deduplication.** The projection runtime tracks processed `event_id`
values in Valkey. If an event is redelivered after a restart, it is
detected as duplicate and skipped without re-applying mutations.

**Idempotency.** Clients may provide an `idempotency_key` on commands.
The kernel checks if the key was already processed and returns the cached
outcome (revision + content_hash) without side effects.

For the experimental `GraphBatch` ingress shape, the same rule should be
treated as mandatory: retries are only safe when the client provides a stable
idempotency key.

## Projection

Events are materialized into the read model:
- **Neo4j** — graph nodes and relationships (upsert semantics)
- **Valkey** — node detail, snapshots, projection state

All mutations for a single event are applied in one call. Neo4j provides
ACID transaction semantics via the driver.

**Eventual consistency.** There is a window between event append and
projection materialization where the read model lags. The kernel does not
expose a synchronous read-after-write guarantee.

## Durability

| Component | Persistence | Guarantee |
|-----------|-------------|-----------|
| NATS JetStream | File-backed (`StorageType::File`) | Survives process restart |
| Neo4j | ACID transactions | Disk-durable on commit |
| Valkey | Depends on deployment config (AOF/RDB) | Kernel delegates to operator |

The kernel does not configure Valkey persistence. Operators must enable
AOF or RDB based on durability requirements.

## What the Kernel Does NOT Guarantee

**No timeouts on external calls.** Network calls to Neo4j, Valkey, and
NATS rely on client library defaults. No explicit per-operation timeout
or deadline is configured.

**No circuit breaker.** If a backend is unavailable, the kernel propagates
the error immediately. There is no retry with backoff, bulkhead, or
graceful degradation. All three backends (Neo4j, Valkey, NATS) must be
available for the kernel to operate.

**No authorization backend.** `ValidateScope` performs set-comparison
only. `GetContext` does not enforce scopes. There is no access control
beyond what the gRPC transport provides (mTLS client certificates).

**No timeline filtering.** `RehydrateSession` accepts `timeline_window`
but does not filter events by time. The field is echoed in the response.

**Idempotency outcome publish is fire-and-forget.** If the outcome
publish fails after event append, a retry may be treated as a new request
(logged as warning, not retried).

**NAK redelivery limits depend on NATS broker config.** The kernel does
not configure max redelivery attempts. Poison messages are handled by
NATS, not the kernel.

**No mesh-managed retry semantics.** The kernel does not assume an Envoy
sidecar or service-mesh retry policy. Retry behavior must be owned by the
calling client or ingress adapter and must respect idempotency and domain
error types.

## Recommended Client Policy For Experimental GraphBatch Ingress

The recommended client policy is:

- require `idempotency_key`
- bounded retries only for transient transport failures
- no blind replay on `ABORTED` or validation failures
- explicit request deadlines

Recommended defaults:

| Setting | Value |
|---------|------:|
| Connect timeout | 2 s |
| Request timeout | 5 s |
| Max attempts | 4 |
| Backoff | 250 ms, 1 s, 3 s (+ jitter) |

This table is for a future direct `GraphBatch` ingress call.

It does **not** describe the upstream model-extraction step. The current
testkit uses separate budgets for `vLLM` primary extraction and the optional
experimental `repair-judge`, with longer request deadlines at that layer.

Safe-to-retry categories:

- `UNAVAILABLE`
- `DEADLINE_EXCEEDED`
- connection-level transport failure

Do-not-blindly-retry categories:

- `ABORTED`
- validation errors
- authorization errors

## Operational Language

When describing the system, use these terms:

| Correct | Avoid |
|---------|-------|
| Eventual consistency | Strong consistency |
| At-least-once delivery | Exactly-once delivery |
| Idempotent replay | Atomic end-to-end |
| Optimistic concurrency via CAS | Distributed transaction |
| Upsert projection (safe for replay) | Append-only projection |
