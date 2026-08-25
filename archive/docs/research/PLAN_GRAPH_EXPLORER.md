# Plan: Graph Explorer

> **Historical (superseded).** Point-in-time record; the `fleet.context.v1` compat surface and the `1..3` depth clamp described below were removed/refactored after this was written (native guardrail now caps traversal depth).

> Historical implementation plan. Completed and retained for traceability.
> Review the current explorer contracts and code before using this plan to
> justify new work.

**Status:** completed
**Priority:** P0
**Date:** 2026-03-18
**Completed:** 2026-03-19
**Related:**
- [`docs/REQUIREMENTS_GRAPH_EXPLORER.md`](./REQUIREMENTS_GRAPH_EXPLORER.md)

## Goal

Deliver a kernel-owned read surface that supports:

- graph traversal beyond 1 hop
- rehydration from any node in the graph
- node detail lookup for an interactive explorer
- a demoable end-to-end explorer journey against a real seeded graph

## Outcome

The explorer gap is closed in the kernel-native surface.

Delivered:

- multi-hop native traversal with a kernel guardrail instead of the old `1..3`
  hard cap
- native `GetContext.depth`
- native `GetNodeDetail` composed from Neo4j projection metadata and Valkey
  detail text
- deeper starship seed coverage for root, mid-level, and leaf explorer flows
- container-backed full-journey coverage for explorer navigation and node detail
- demoable cluster journey in both local port-forward and Kubernetes `Job` form
- Kubernetes `Job` support for gRPC TLS/mTLS and client-side NATS TLS settings

## Decisions

### 1. Native kernel API can evolve; compatibility API stays frozen

Implemented as planned.

- explorer reads use the kernel-native package, now canonically
  `underpass.rehydration.kernel.v1beta1`
- compatibility `fleet.context.v1` remains unchanged
- compatibility `GetGraphRelationships.depth` still clamps to `1..3`

### 2. Replace the hardcoded `1..3` kernel limit with a server guardrail

Implemented as planned.

- the kernel-owned path no longer uses the compatibility-era `1..3` bound
- native `GetContext(depth=0)` resolves to the kernel default depth
- traversal stays bounded by kernel-native transport/application guardrails

### 3. `GetNodeDetail` must compose stores

Implemented as planned.

- Neo4j remains the source of node metadata
- Valkey remains the source of expanded detail text
- `GetNodeDetail` returns a combined panel-oriented response

## Phase 1: Traversal Foundation

**Status:** complete

Delivered:

- variable-depth neighborhood loading in the domain/application path
- variable-depth Neo4j traversal query
- multi-hop bundle-producing reads
- relationship deduplication across expanded paths

Primary files:

- [`crates/rehydration-domain/src/repositories/graph_neighborhood_reader.rs`](../../../crates/rehydration-domain/src/repositories/graph_neighborhood_reader.rs)
- [`crates/rehydration-adapter-neo4j/src/adapter/load_neighborhood.rs`](../../../crates/rehydration-adapter-neo4j/src/adapter/load_neighborhood.rs)
- [`crates/rehydration-adapter-neo4j/src/adapter/queries/load_neighborhood_query.rs`](../../../crates/rehydration-adapter-neo4j/src/adapter/queries/load_neighborhood_query.rs)
- [`crates/rehydration-application/src/queries/get_context.rs`](../../../crates/rehydration-application/src/queries/get_context.rs)
- [`crates/rehydration-application/src/queries/rehydrate_session.rs`](../../../crates/rehydration-application/src/queries/rehydrate_session.rs)
- [`crates/rehydration-application/src/queries/bundle_snapshot.rs`](../../../crates/rehydration-application/src/queries/bundle_snapshot.rs)
- [`crates/rehydration-application/src/queries/rehydration_diagnostics.rs`](../../../crates/rehydration-application/src/queries/rehydration_diagnostics.rs)

## Phase 2: Native Explorer Read Contract

**Status:** complete

Delivered:

- native `GetContextRequest.depth`
- `depth=0` meaning "use kernel default depth"
- compatibility transport behavior preserved

Primary files:

- [`api/proto/underpass/rehydration/kernel/v1beta1/query.proto`](../../../api/proto/underpass/rehydration/kernel/v1beta1/query.proto)
- [`crates/rehydration-transport-grpc/src/transport/query_grpc_service.rs`](../../../crates/rehydration-transport-grpc/src/transport/query_grpc_service.rs)
- [`crates/rehydration-transport-grpc/src/transport/tests.rs`](../../../crates/rehydration-transport-grpc/src/transport/tests.rs)

## Phase 3: `GetNodeDetail`

**Status:** complete

Delivered:

- native `ContextQueryService.GetNodeDetail`
- composed Neo4j + Valkey read path
- transport mapping for success and not-found

Primary files:

- [`api/proto/underpass/rehydration/kernel/v1beta1/query.proto`](../../../api/proto/underpass/rehydration/kernel/v1beta1/query.proto)
- [`crates/rehydration-application/src/queries/get_node_detail.rs`](../../../crates/rehydration-application/src/queries/get_node_detail.rs)
- [`crates/rehydration-transport-grpc/src/transport/query_grpc_service.rs`](../../../crates/rehydration-transport-grpc/src/transport/query_grpc_service.rs)
- [`crates/rehydration-transport-grpc/src/transport/tests.rs`](../../../crates/rehydration-transport-grpc/src/transport/tests.rs)

## Phase 4: Explorer E2E and Demo

**Status:** complete

Delivered:

- deeper starship explorer subtree with root, mid-level, checklist, and leaf
- container-backed explorer full journey
- container-backed explorer full journey over TLS
- local cluster demo via port-forward
- in-cluster explorer demo via Kubernetes `Job`
- in-cluster explorer demo validated over gRPC mutual TLS

Primary files:

- [`crates/rehydration-transport-grpc/src/starship_e2e.rs`](../../../crates/rehydration-transport-grpc/src/starship_e2e.rs)
- [`crates/rehydration-tests-kernel/tests/kernel_full_journey_integration.rs`](../../../crates/rehydration-tests-kernel/tests/kernel_full_journey_integration.rs)
- [`crates/rehydration-tests-kernel/tests/kernel_full_journey_tls_integration.rs`](../../../crates/rehydration-tests-kernel/tests/kernel_full_journey_tls_integration.rs)
- [`crates/rehydration-transport-grpc/src/bin/starship_cluster_journey/main.rs`](../../../crates/rehydration-transport-grpc/src/bin/starship_cluster_journey/main.rs)
- [`scripts/demo/run-starship-cluster-journey.sh`](../../../scripts/demo/run-starship-cluster-journey.sh)
- [`scripts/demo/run-starship-demo-k8s-job.sh`](../../../scripts/demo/run-starship-demo-k8s-job.sh)

## Validation

Representative validation that closed this plan:

- `cargo test -p rehydration-transport-grpc projection_messages_render_expected_starship_graph`
- `cargo test -p rehydration-tests-kernel --features container-tests --test kernel_full_journey_integration --no-run`
- `cargo test -p rehydration-tests-kernel --features container-tests --test kernel_full_journey_tls_integration --no-run`
- `bash scripts/demo/run-starship-cluster-journey.sh`
- `bash scripts/demo/run-starship-demo-k8s-job.sh`
- `IMAGE_TAG=<tag> bash scripts/ci/kubernetes-transport-smoke.sh grpc-mutual`
- `GRPC_TLS_MODE=mutual GRPC_TLS_SECRET_NAME=<secret> bash scripts/demo/run-starship-demo-k8s-job.sh`

The last two commands were used together to validate the explorer Kubernetes
`Job` against a temporary gRPC mTLS release.

## Out Of Scope

Still out of scope for this slice:

- changing the external `fleet.context.v1` contract
- adding write-side explorer mutations
- moving all node metadata into Valkey
- UI implementation in this repo

## Residual Notes

- the compatibility shell remains intentionally conservative on depth
- the Kubernetes explorer demo supports gRPC TLS/mTLS today
- client-side NATS TLS support exists in the explorer job path, but the shared
  `underpass-runtime` environment still runs NATS in plaintext unless a
  TLS-enabled NATS endpoint is provided

## Exit Criteria

- [x] native `GetContext` supports explicit depth
- [x] bundle-producing reads no longer stop at 1 hop
- [x] native `GetNodeDetail` exists and is transport-tested
- [x] explorer scenario passes on a deep seeded graph
- [x] compatibility depth behavior remains unchanged
