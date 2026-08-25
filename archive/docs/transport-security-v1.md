# Transport Security v1

Status: `implemented_with_follow_up_gap`

## Purpose

Define the first transport-security milestone for `rehydration-kernel`.

This milestone started with the most important gap:

- inbound gRPC transport security for the standalone kernel

The delivered result is a kernel that is deployable in less-trusted cluster
networks
without forcing a service mesh or sidecar proxy.

## Why This Matters

The kernel is now strong in:

- contract discipline
- packaging
- CI quality gates
- Helm deployment

The original transport gap is closed for the kernel-owned surfaces.

Current limitations:

- the chart does not yet expose Neo4j client identity as a first-class feature
- HTTP admin transport is still a placeholder, not a hardened public surface

Those are narrower follow-up items, not the original blocker.

## Current State

### Inbound gRPC

Current server implementation:

- [`crates/rehydration-transport-grpc/src/transport/grpc_server.rs`](../../crates/rehydration-transport-grpc/src/transport/grpc_server.rs)

It now uses:

- `tonic::transport::Server::builder()`

It now supports:

- plaintext mode
- server TLS
- mutual TLS with client CA verification

### Configuration

Current app config:

- [`crates/rehydration-config/src/app_config.rs`](../../crates/rehydration-config/src/app_config.rs)

It now exposes:

- `grpc_bind`
- backend URIs

It now includes:

- TLS mode
- cert path
- key path
- client CA path

### Helm

Current chart wiring:

- [`charts/rehydration-kernel/templates/deployment.yaml`](../../charts/rehydration-kernel/templates/deployment.yaml)
- [`charts/rehydration-kernel/values.yaml`](../../charts/rehydration-kernel/values.yaml)

It now supports:

- TLS secret mounts for inbound gRPC
- mTLS CA mounts for inbound gRPC
- transport mode selection
- outbound NATS TLS and outbound Valkey TLS wiring
- custom Neo4j CA wiring for secure inline `graphUri` values

It does not yet support:

- first-class Neo4j client identity wiring

Current progress:

- native plaintext / TLS / mTLS modes are implemented in the gRPC server
- config envs for certificate paths are implemented
- Helm wiring is implemented for inbound gRPC TLS and mTLS
- outbound NATS TLS runtime configuration is implemented
- outbound Valkey TLS and `rediss://` / `valkeys://` support are implemented
- the current chart slice is wiring certificate mounts and operator-facing
  values for outbound NATS and Valkey TLS
- custom Neo4j CA wiring is implemented in the chart for secure inline
  `graphUri` values
- cluster smoke coverage is implemented for gRPC TLS and mTLS plus outbound TLS
- container-backed full-journey TLS coverage is implemented across gRPC, NATS,
  Valkey, and Neo4j

## Decision

Chosen direction for v1:

- inbound TLS and mTLS are implemented natively in the kernel gRPC server
- plaintext remains available as a compatibility mode
- service mesh remains compatible, but is not required

Rationale:

- portability across clusters and environments
- less coupling to platform-specific infrastructure
- easier product packaging for external consumers

## Scope

In scope for v1:

1. server TLS for gRPC
2. optional client certificate validation for gRPC mTLS
3. config model for plaintext, TLS, and mTLS modes
4. Helm support for certificate secrets and mounts
5. tests for plaintext, TLS, and mTLS paths
6. operator documentation

Follow-on transport-security slices now extend this into outbound event and
cache connections:

- NATS client TLS and mTLS
- Valkey TLS via `rediss://` and `valkeys://`

Remaining follow-up beyond the delivered v1 slice:

- RBAC or authorization based on certificate identity
- SPIFFE or SPIRE integration
- automatic certificate rotation
- first-class Neo4j client identity wiring
- additional HTTP admin transport security

Those belong to later milestones.

## Security Modes

### Mode 1: Plaintext

Use case:

- local development
- trusted throwaway environments

Behavior:

- current behavior remains unchanged

### Mode 2: TLS

Use case:

- encrypted transport without client cert requirements

Behavior:

- server presents certificate and key
- clients validate server identity

### Mode 3: Mutual TLS

Use case:

- internal production traffic where the kernel should accept only known clients

Behavior:

- server presents certificate and key
- clients must present a certificate signed by a trusted CA
- server validates client certificate chain

## Proposed Config Shape

New config module:

- `grpc_tls_config.rs`

Proposed model:

- `mode = disabled | server | mutual`
- `server_cert_path`
- `server_key_path`
- `client_ca_cert_path`

Possible env vars:

- `REHYDRATION_GRPC_TLS_MODE`
- `REHYDRATION_GRPC_TLS_CERT_PATH`
- `REHYDRATION_GRPC_TLS_KEY_PATH`
- `REHYDRATION_GRPC_TLS_CLIENT_CA_PATH`

Rules:

- `disabled`: no TLS fields required
- `server`: cert and key required
- `mutual`: cert, key, and client CA required

## Proposed Code Changes

### Config

Add:

- [`crates/rehydration-config/src/grpc_tls_config.rs`](../../crates/rehydration-config/src)

Update:

- [`crates/rehydration-config/src/lib.rs`](../../crates/rehydration-config/src/lib.rs)
- [`crates/rehydration-config/src/app_config.rs`](../../crates/rehydration-config/src/app_config.rs)

### Server

Update:

- [`crates/rehydration-transport-grpc/src/transport/grpc_server.rs`](../../crates/rehydration-transport-grpc/src/transport/grpc_server.rs)

Expected change:

- build `Server::builder()` with `ServerTlsConfig` when TLS mode is enabled

### Server Bootstrap

Update:

- [`crates/rehydration-server/src/main.rs`](../../crates/rehydration-server/src/main.rs)

Expected change:

- pass TLS config through normal bootstrap

### Helm

Update:

- [`charts/rehydration-kernel/values.yaml`](../../charts/rehydration-kernel/values.yaml)
- [`charts/rehydration-kernel/templates/deployment.yaml`](../../charts/rehydration-kernel/templates/deployment.yaml)

Expected additions:

- `tls.mode`
- `tls.secretName`
- mount paths for certs and CA
- env vars for the selected mode

## Test Plan

Minimum required tests:

1. plaintext mode still serves existing gRPC traffic
2. TLS mode starts with valid cert and key
3. TLS mode rejects broken cert configuration
4. mTLS mode rejects clients without a certificate
5. mTLS mode accepts clients signed by the trusted CA
6. Helm renders correctly in all three modes

Test categories:

- unit tests for config parsing
- integration tests for transport startup
- targeted client/server transport tests
- Helm render checks

## Rollout Strategy

Phase 1:

- merge code with TLS disabled by default

Phase 2:

- enable server-only TLS in staging

Phase 3:

- enable mTLS for controlled internal clients

## Risks

- certificate file handling can become a source of configuration drift
- mTLS tests will increase transport test complexity
- operators may confuse app-level TLS with mesh-level TLS

## Exit Criteria

This milestone is complete when:

- the kernel supports plaintext, TLS, and mTLS for gRPC
- Helm can deploy all three modes
- documentation explains how to configure each mode
- the default behavior remains backward-compatible

## Follow-Up Milestones

After v1:

1. NATS TLS and mTLS
2. Valkey TLS via `rediss://`
3. stronger deployment guidance for service mesh compatibility
