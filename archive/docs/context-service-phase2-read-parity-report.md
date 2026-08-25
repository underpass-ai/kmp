# Context Service Phase 2 Read-Parity Report

> Review required. This report compares against a migration target that no
> longer exists in the kernel repo. It may still be useful as historical
> analysis, but every conclusion should be revalidated before reuse.

Status: Complete
Scope reviewed: `GetContext`, `RehydrateSession`, `ValidateScope`, `GetGraphRelationships`

## Purpose

Record the current parity state of the implemented compatibility read RPCs
against the Phase 0 freeze and the Python source baseline.

This report is the decision point for whether Phase 2 can be closed or must
stay open before async NATS work begins.

## Evidence Reviewed

Frozen contract and test catalog:

- [`context-service-phase0-contract-freeze.md`](./context-service-phase0-contract-freeze.md)
- [`context-service-golden-tests.md`](./context-service-golden-tests.md)

Python baseline:

- [`services/context/server.py`](../../to-delete-when-finish-the-standalone-extraction/swe-ai-fleet-base-for-context-extraction/services/context/server.py)
- [`test_context_service_servicer.py`](../../to-delete-when-finish-the-standalone-extraction/swe-ai-fleet-base-for-context-extraction/services/context/tests/unit/test_context_service_servicer.py)
- [`get_graph_relationships.py`](../../to-delete-when-finish-the-standalone-extraction/swe-ai-fleet-base-for-context-extraction/core/context/application/usecases/get_graph_relationships.py)
- [`test_get_graph_relationships.py`](../../to-delete-when-finish-the-standalone-extraction/swe-ai-fleet-base-for-context-extraction/core/context/tests/unit/application/usecases/test_get_graph_relationships.py)

Rust boundary and tests:

- [`context_service_grpc_service.rs`](../../crates/rehydration-transport-grpc/src/transport/context_service_compatibility/context_service_grpc_service.rs)
- request mapping under [`request_mapping/`](../../crates/rehydration-transport-grpc/src/transport/context_service_compatibility/request_mapping/)
- response mapping under [`response_mapping/`](../../crates/rehydration-transport-grpc/src/transport/context_service_compatibility/response_mapping/)
- read golden tests under [`compatibility_golden/`](../../crates/rehydration-transport-grpc/tests/compatibility_golden/)
- render controls under [`render_graph_bundle.rs`](../../crates/rehydration-application/src/queries/render_graph_bundle.rs)
- snapshot persistence options under [`snapshot_store.rs`](../../crates/rehydration-domain/src/repositories/snapshot_store.rs)
- Valkey TTL handling under [`snapshot_store.rs`](../../crates/rehydration-adapter-valkey/src/adapter/snapshot_store.rs)

## Current Result

### `GetContext`

Boundary DTO status: `match`

Confirmed parity:

- external request and response field names are preserved
- response exposes `context`, `token_count`, `scopes`, `version`, and `blocks`
- `phase` is translated into the compatibility scope catalog at the boundary
- `subtask_id` becomes a node-centric focus hint that reorders rendered sections
- `token_budget` is enforced as a render budget hint without leaking the external
  field into the domain model
- unexpected application errors map to gRPC `INTERNAL`

Implemented evidence:

- [`get_context.rs`](../../crates/rehydration-transport-grpc/src/transport/context_service_compatibility/rpc/get_context.rs)
- [`get_context_query.rs`](../../crates/rehydration-transport-grpc/src/transport/context_service_compatibility/request_mapping/get_context_query.rs)
- [`get_context_response.rs`](../../crates/rehydration-transport-grpc/src/transport/context_service_compatibility/response_mapping/get_context_response.rs)
- [`context_render_options.rs`](../../crates/rehydration-application/src/queries/context_render_options.rs)
- [`render_graph_bundle.rs`](../../crates/rehydration-application/src/queries/render_graph_bundle.rs)
- [`get_context.rs`](../../crates/rehydration-transport-grpc/tests/compatibility_golden/get_context.rs)

Residual drift:

- no evidence-backed DTO drift remains in the implemented read path

Decision:

- `GetContext` parity is considered closed for the implemented contract surface

### `RehydrateSession`

Boundary DTO status: `match`

Confirmed parity:

- external request and response field names are preserved
- `packs` and nested public DTOs are rendered at the boundary
- `timeline_events <= 0` now falls back to the frozen external default `50`
- `ttl_seconds <= 0` now falls back to the frozen external default `3600`
- per-request `ttl_seconds` now reaches the snapshot persistence boundary and is
  honored by the Valkey adapter ahead of endpoint defaults
- `generated_at_ms` is treated as nondeterministic and normalized only in
  golden tests

Implemented evidence:

- [`rehydrate_session.rs`](../../crates/rehydration-transport-grpc/src/transport/context_service_compatibility/rpc/rehydrate_session.rs)
- [`rehydrate_session_query.rs`](../../crates/rehydration-transport-grpc/src/transport/context_service_compatibility/request_mapping/rehydrate_session_query.rs)
- [`rehydrate_session_response.rs`](../../crates/rehydration-transport-grpc/src/transport/context_service_compatibility/response_mapping/rehydrate_session_response.rs)
- [`rehydrate_session.rs`](../../crates/rehydration-application/src/queries/rehydrate_session.rs)
- [`snapshot_save_options.rs`](../../crates/rehydration-domain/src/repositories/snapshot_save_options.rs)
- [`snapshot_store.rs`](../../crates/rehydration-adapter-valkey/src/adapter/snapshot_store.rs)
- [`rehydrate_session.rs`](../../crates/rehydration-transport-grpc/tests/compatibility_golden/rehydrate_session.rs)
- [`valkey_integration.rs`](../../crates/rehydration-adapter-valkey/tests/valkey_integration.rs)

Residual drift:

- no evidence-backed DTO drift remains in the implemented read path

Decision:

- `RehydrateSession` parity is considered closed for the implemented contract
  surface

### `ValidateScope`

Boundary DTO status: `match`

Confirmed parity:

- external request and response field names are preserved
- allowed and rejected flows render `allowed`, `missing`, `extra`, and
  `reason`
- the edge remains responsible for compatibility scope vocabulary

Implemented evidence:

- [`validate_scope.rs`](../../crates/rehydration-transport-grpc/src/transport/context_service_compatibility/rpc/validate_scope.rs)
- [`validate_scope_query.rs`](../../crates/rehydration-transport-grpc/src/transport/context_service_compatibility/request_mapping/validate_scope_query.rs)
- [`validate_scope_response.rs`](../../crates/rehydration-transport-grpc/src/transport/context_service_compatibility/response_mapping/validate_scope_response.rs)
- [`validate_scope.rs`](../../crates/rehydration-transport-grpc/tests/compatibility_golden/validate_scope.rs)

Residual drift:

- no evidence-backed DTO drift remains in the implemented allowed and rejected
  paths

### `GetGraphRelationships`

Boundary DTO status: `match`

Confirmed parity:

- external request and response field names are preserved
- `depth` is clamped to `1..3`
- invalid `node_type` now maps to gRPC `INVALID_ARGUMENT`
- missing node now maps to gRPC `INVALID_ARGUMENT`
- response rendering preserves `node`, `neighbors`, `relationships`, `success`,
  and `message`

Implemented evidence:

- [`get_graph_relationships.rs`](../../crates/rehydration-transport-grpc/src/transport/context_service_compatibility/rpc/get_graph_relationships.rs)
- [`get_graph_relationships_query.rs`](../../crates/rehydration-transport-grpc/src/transport/context_service_compatibility/request_mapping/get_graph_relationships_query.rs)
- [`get_graph_relationships_node_type.rs`](../../crates/rehydration-transport-grpc/src/transport/context_service_compatibility/request_mapping/get_graph_relationships_node_type.rs)
- [`get_graph_relationships_response.rs`](../../crates/rehydration-transport-grpc/src/transport/context_service_compatibility/response_mapping/get_graph_relationships_response.rs)
- [`get_graph_relationships.rs`](../../crates/rehydration-transport-grpc/tests/compatibility_golden/get_graph_relationships.rs)

Residual drift:

- no evidence-backed DTO drift remains in the implemented read path

Decision:

- `GetGraphRelationships` parity is considered closed for the implemented
  contract surface

## Phase 2 Closeout Decision

Phase 2 is closed.

What is already closed:

- response DTO shape for the implemented happy-path read RPCs
- golden coverage for `GetContext`, `RehydrateSession`, `ValidateScope`, and
  `GetGraphRelationships`
- frozen `GetGraphRelationships.depth` clamp
- frozen invalid `node_type` handling
- frozen missing-node handling for `GetGraphRelationships`
- frozen `RehydrateSession.timeline_events` defaulting
- frozen `RehydrateSession.ttl_seconds` defaulting and persistence propagation
- `GetContext` request semantics for `phase`, `subtask_id`, and `token_budget`

## Next Slice

The next slice moves to Phase 3:

1. implement async NATS request and reply parity
2. preserve `EventEnvelope` handling and `ack` or `nak` behavior
3. keep the NATS adapter split by routing, decoding, and publication concerns
